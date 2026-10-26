//! Web Push, with no third-party service in between.
//!
//! The browser's own push service (FCM for Chrome, Mozilla's for Firefox) is
//! free and needs no account: the server proves who it is with a VAPID key
//! pair it generates itself, and encrypts each message for the one browser that
//! subscribed (RFC 8291, aes128gcm). Everything here is built on `ring`, which
//! is already in the binary for TLS.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use chrono::Utc;
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};
use ring::{aead, agreement, hkdf};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

/// The server's VAPID identity. Generated once, kept in SQLite.
pub struct Vapid {
    pair: EcdsaKeyPair,
}

impl Vapid {
    /// Load the key pair, creating it on first use.
    pub async fn load_or_create(pool: &SqlitePool) -> AppResult<Self> {
        let rng = SystemRandom::new();
        let row: Option<(Vec<u8>,)> =
            sqlx::query_as("SELECT private_pkcs8 FROM push_vapid WHERE id = 1")
                .fetch_optional(pool)
                .await?;
        let pkcs8 = match row {
            Some((bytes,)) => bytes,
            None => {
                let doc = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
                    .map_err(|_| {
                        AppError::Other(anyhow::anyhow!("could not generate a VAPID key"))
                    })?;
                // Two first requests at once must agree on one key.
                sqlx::query(
                    "INSERT INTO push_vapid (id, private_pkcs8, created_at) VALUES (1, ?, ?)
                     ON CONFLICT (id) DO NOTHING",
                )
                .bind(doc.as_ref())
                .bind(Utc::now().to_rfc3339())
                .execute(pool)
                .await?;
                let (bytes,): (Vec<u8>,) =
                    sqlx::query_as("SELECT private_pkcs8 FROM push_vapid WHERE id = 1")
                        .fetch_one(pool)
                        .await?;
                bytes
            }
        };
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &pkcs8, &rng)
            .map_err(|_| AppError::Other(anyhow::anyhow!("stored VAPID key is unreadable")))?;
        Ok(Self { pair })
    }

    /// Uncompressed P-256 point, base64url: the `applicationServerKey`.
    pub fn public_key(&self) -> String {
        B64.encode(self.pair.public_key().as_ref())
    }

    /// `Authorization` header for one push service, valid for twelve hours.
    pub fn authorization(&self, endpoint: &str) -> AppResult<String> {
        let audience = origin_of(endpoint)
            .ok_or_else(|| AppError::BadRequest("push endpoint is not a URL".into()))?;
        let header = B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = serde_json::json!({
            "aud": audience,
            "exp": Utc::now().timestamp() + 12 * 3600,
            // Push services ask for a contact; this instance has no public one.
            "sub": "mailto:frog@localhost",
        });
        let claims = B64.encode(claims.to_string());
        let signing_input = format!("{header}.{claims}");
        let signature = self
            .pair
            .sign(&SystemRandom::new(), signing_input.as_bytes())
            .map_err(|_| AppError::Other(anyhow::anyhow!("could not sign the VAPID token")))?;
        Ok(format!(
            "vapid t={signing_input}.{}, k={}",
            B64.encode(signature.as_ref()),
            self.public_key()
        ))
    }
}

/// `https://fcm.googleapis.com/fcm/send/abc` → `https://fcm.googleapis.com`.
fn origin_of(endpoint: &str) -> Option<String> {
    let rest = endpoint.strip_prefix("https://")?;
    let host = rest.split('/').next().filter(|h| !h.is_empty())?;
    Some(format!("https://{host}"))
}

/// Only real push services: https, and never this machine or its network.
pub fn endpoint_is_acceptable(endpoint: &str) -> bool {
    let Some(origin) = origin_of(endpoint) else {
        return false;
    };
    let host = origin
        .trim_start_matches("https://")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let private = host == "localhost"
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || host.starts_with('[')
        || host.parse::<std::net::Ipv4Addr>().is_ok();
    !private && host.contains('.')
}

struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn hkdf_expand(prk: &hkdf::Prk, info: &[u8], len: usize) -> AppResult<Vec<u8>> {
    let mut out = vec![0u8; len];
    prk.expand(&[info], Len(len))
        .and_then(|okm| okm.fill(&mut out))
        .map_err(|_| AppError::Other(anyhow::anyhow!("key derivation failed")))?;
    Ok(out)
}

/// Encrypt one message for one subscription (RFC 8291 + RFC 8188, a single
/// record). `p256dh` and `auth` are the base64url keys the browser handed us.
pub fn encrypt(p256dh: &str, auth: &str, plaintext: &[u8]) -> AppResult<Vec<u8>> {
    let bad = || AppError::BadRequest("push subscription keys are malformed".into());
    let ua_public = B64
        .decode(p256dh.trim_end_matches('='))
        .map_err(|_| bad())?;
    let auth_secret = B64.decode(auth.trim_end_matches('=')).map_err(|_| bad())?;
    if ua_public.len() != 65 || auth_secret.len() != 16 {
        return Err(bad());
    }

    let rng = SystemRandom::new();
    let fail = |what: &str| AppError::Other(anyhow::anyhow!("push encryption: {what}"));

    let ephemeral = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng)
        .map_err(|_| fail("key generation"))?;
    let as_public = ephemeral
        .compute_public_key()
        .map_err(|_| fail("public key"))?
        .as_ref()
        .to_vec();
    let shared = agreement::agree_ephemeral(
        ephemeral,
        &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, &ua_public),
        |secret| secret.to_vec(),
    )
    .map_err(|_| bad())?;

    // IKM = HKDF(auth_secret, ecdh_secret, "WebPush: info" || 0 || ua || as)
    let prk_key = hkdf::Salt::new(hkdf::HKDF_SHA256, &auth_secret).extract(&shared);
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(&ua_public);
    key_info.extend_from_slice(&as_public);
    let ikm = hkdf_expand(&prk_key, &key_info, 32)?;

    let mut salt = [0u8; 16];
    rng.fill(&mut salt).map_err(|_| fail("salt"))?;
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &salt).extract(&ikm);
    let cek = hkdf_expand(&prk, b"Content-Encoding: aes128gcm\0", 16)?;
    let nonce = hkdf_expand(&prk, b"Content-Encoding: nonce\0", 12)?;

    // One record: the message, then the 0x02 delimiter that marks it last.
    let mut record = plaintext.to_vec();
    record.push(2);
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_128_GCM, &cek).map_err(|_| fail("cipher key"))?,
    );
    let nonce = aead::Nonce::try_assume_unique_for_key(&nonce).map_err(|_| fail("nonce"))?;
    key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut record)
        .map_err(|_| fail("seal"))?;

    let mut body = Vec::with_capacity(16 + 4 + 1 + 65 + record.len());
    body.extend_from_slice(&salt);
    body.extend_from_slice(&4096u32.to_be_bytes());
    body.push(as_public.len() as u8);
    body.extend_from_slice(&as_public);
    body.extend_from_slice(&record);
    Ok(body)
}

/// What happened to one delivery.
#[derive(Debug, PartialEq)]
pub enum Delivery {
    Sent,
    /// The browser unsubscribed or the subscription expired: forget it.
    Gone,
    Failed(String),
}

pub async fn send(
    http: &reqwest::Client,
    vapid: &Vapid,
    endpoint: &str,
    p256dh: &str,
    auth: &str,
    payload: &serde_json::Value,
) -> AppResult<Delivery> {
    let body = encrypt(p256dh, auth, payload.to_string().as_bytes())?;
    let res = http
        .post(endpoint)
        .header("authorization", vapid.authorization(endpoint)?)
        .header("content-encoding", "aes128gcm")
        .header("content-type", "application/octet-stream")
        // A new-episode alert is stale after a day.
        .header("ttl", "86400")
        .header("urgency", "normal")
        .body(body)
        .send()
        .await;
    Ok(match res {
        Ok(r) if r.status().is_success() => Delivery::Sent,
        Ok(r) if r.status().as_u16() == 404 || r.status().as_u16() == 410 => {
            tracing::info!(status = %r.status(), "push service says the subscription is gone");
            Delivery::Gone
        }
        Ok(r) => {
            let status = r.status();
            let text = r.text().await.unwrap_or_default();
            Delivery::Failed(format!(
                "{status}: {}",
                text.chars().take(200).collect::<String>()
            ))
        }
        Err(e) => Delivery::Failed(e.to_string()),
    })
}

/// How one message fared across every subscribed browser.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Broadcast {
    pub sent: usize,
    pub gone: usize,
    pub failed: usize,
}

/// A subscription that keeps failing for this many sends in a row is
/// abandoned; a browser that was simply offline recovers well before that.
const MAX_FAILURES: i64 = 20;

/// Send one message to every subscribed browser, forgetting the ones whose
/// push service says they are gone.
pub async fn broadcast(
    state: &crate::AppState,
    payload: &serde_json::Value,
) -> AppResult<Broadcast> {
    let subs: Vec<(String, String, String)> =
        sqlx::query_as("SELECT endpoint, p256dh, auth FROM push_subscription")
            .fetch_all(&state.pool)
            .await?;
    let mut out = Broadcast::default();
    if subs.is_empty() {
        return Ok(out);
    }
    let vapid = Vapid::load_or_create(&state.pool).await?;
    for (endpoint, p256dh, auth) in subs {
        let result = send(&state.http, &vapid, &endpoint, &p256dh, &auth, payload).await;
        match result {
            Ok(Delivery::Sent) => {
                out.sent += 1;
                sqlx::query(
                    "UPDATE push_subscription SET last_ok_at = ?, failures = 0 WHERE endpoint = ?",
                )
                .bind(Utc::now().to_rfc3339())
                .bind(&endpoint)
                .execute(&state.pool)
                .await?;
            }
            Ok(Delivery::Gone) => {
                out.gone += 1;
                tracing::info!("a browser unsubscribed; forgetting it");
                sqlx::query("DELETE FROM push_subscription WHERE endpoint = ?")
                    .bind(&endpoint)
                    .execute(&state.pool)
                    .await?;
            }
            Ok(Delivery::Failed(why)) | Err(AppError::BadRequest(why)) => {
                out.failed += 1;
                tracing::warn!(error = %why, "push delivery failed");
                sqlx::query(
                    "UPDATE push_subscription SET failures = failures + 1 WHERE endpoint = ?",
                )
                .bind(&endpoint)
                .execute(&state.pool)
                .await?;
            }
            Err(e) => return Err(e),
        }
    }
    sqlx::query("DELETE FROM push_subscription WHERE failures >= ?")
        .bind(MAX_FAILURES)
        .execute(&state.pool)
        .await?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decrypt as the browser would, with the browser's own key pair.
    fn decrypt(
        body: &[u8],
        ua_private: agreement::EphemeralPrivateKey,
        ua_public: &[u8],
        auth: &[u8],
    ) -> Vec<u8> {
        let salt = &body[..16];
        let rs = u32::from_be_bytes(body[16..20].try_into().unwrap());
        assert_eq!(rs, 4096);
        let idlen = body[20] as usize;
        let as_public = &body[21..21 + idlen];
        let mut record = body[21 + idlen..].to_vec();

        let shared = agreement::agree_ephemeral(
            ua_private,
            &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, as_public),
            |s| s.to_vec(),
        )
        .unwrap();
        let prk_key = hkdf::Salt::new(hkdf::HKDF_SHA256, auth).extract(&shared);
        let mut info = b"WebPush: info\0".to_vec();
        info.extend_from_slice(ua_public);
        info.extend_from_slice(as_public);
        let ikm = hkdf_expand(&prk_key, &info, 32).unwrap();
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(&ikm);
        let cek = hkdf_expand(&prk, b"Content-Encoding: aes128gcm\0", 16).unwrap();
        let nonce = hkdf_expand(&prk, b"Content-Encoding: nonce\0", 12).unwrap();
        let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_128_GCM, &cek).unwrap());
        let plain = key
            .open_in_place(
                aead::Nonce::try_assume_unique_for_key(&nonce).unwrap(),
                aead::Aad::empty(),
                &mut record,
            )
            .unwrap();
        assert_eq!(plain.last(), Some(&2), "last-record delimiter");
        plain[..plain.len() - 1].to_vec()
    }

    #[test]
    fn a_message_round_trips_through_rfc_8291() {
        let rng = SystemRandom::new();
        let ua = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let ua_public = ua.compute_public_key().unwrap().as_ref().to_vec();
        let mut auth = [0u8; 16];
        rng.fill(&mut auth).unwrap();

        let message = br#"{"title":"Severance","body":"S2E1 is out"}"#;
        let body = encrypt(&B64.encode(&ua_public), &B64.encode(auth), message).unwrap();
        assert_eq!(decrypt(&body, ua, &ua_public, &auth), message);
    }

    #[test]
    fn malformed_keys_are_refused_not_panicked_on() {
        assert!(encrypt("short", "short", b"x").is_err());
        assert!(encrypt("!!!", "!!!", b"x").is_err());
    }

    #[test]
    fn only_public_https_push_services_are_accepted() {
        assert!(endpoint_is_acceptable(
            "https://fcm.googleapis.com/fcm/send/abc"
        ));
        assert!(endpoint_is_acceptable(
            "https://updates.push.services.mozilla.com/wpush/v2/x"
        ));
        assert!(!endpoint_is_acceptable("http://fcm.googleapis.com/x"));
        assert!(!endpoint_is_acceptable("https://localhost/x"));
        assert!(!endpoint_is_acceptable("https://127.0.0.1/x"));
        assert!(!endpoint_is_acceptable("https://192.168.1.2/x"));
        assert!(!endpoint_is_acceptable("https://[::1]/x"));
        assert!(!endpoint_is_acceptable("https://router/x"));
    }

    #[test]
    fn the_vapid_token_is_scoped_to_the_push_service() {
        let rng = SystemRandom::new();
        let doc = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let vapid = Vapid {
            pair: EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, doc.as_ref(), &rng)
                .unwrap(),
        };
        let header = vapid
            .authorization("https://fcm.googleapis.com/fcm/send/abc")
            .unwrap();
        let token = header
            .strip_prefix("vapid t=")
            .unwrap()
            .split(',')
            .next()
            .unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: serde_json::Value =
            serde_json::from_slice(&B64.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");

        // The signature verifies against the public key we hand to browsers.
        let public = B64.decode(vapid.public_key()).unwrap();
        let signature = B64.decode(parts[2]).unwrap();
        ring::signature::UnparsedPublicKey::new(&ring::signature::ECDSA_P256_SHA256_FIXED, public)
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
        assert_eq!(B64.decode(vapid.public_key()).unwrap().len(), 65);
    }
}
