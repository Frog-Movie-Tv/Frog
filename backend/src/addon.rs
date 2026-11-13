//! The signed Firefox build of the browser extension, served from Frog itself.
//!
//! Firefox only offers to install an add-on straight from a link when the
//! site sends it as `application/x-xpinstall`. GitHub sends release files as
//! plain downloads, so Frog fetches the build for its own version once, keeps
//! it in memory, and serves it with that type. A restart fetches it again.
//!
//! Nothing is served without an account, but Firefox may fetch the file
//! without the session cookie, so a signed-in page asks for a link first:
//! the file answers anyone holding one of those for an hour. When the build
//! cannot be had (no release carries it yet, GitHub is away), the link sends
//! the browser to the release page's own copy instead.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rand::RngCore;

/// The name every release attaches the signed build under.
pub const FILE: &str = "frog-extension-firefox.xpi";
pub const CONTENT_TYPE: &str = "application/x-xpinstall";

/// How long a link from Settings works.
const LINK_LIFE: Duration = Duration::from_secs(60 * 60);
/// Links held at once; the oldest go first past it.
const MAX_LINKS: usize = 1_000;
/// An add-on is tens of kilobytes; anything near this is not one.
const MAX_BYTES: usize = 20 * 1024 * 1024;

#[derive(Default)]
pub struct Addon {
    /// (version, the signed file)
    file: tokio::sync::Mutex<Option<(String, Vec<u8>)>>,
    links: Mutex<HashMap<String, Instant>>,
}

impl Addon {
    /// A key for one link, good for `LINK_LIFE`.
    pub fn issue(&self) -> String {
        self.issue_at(Instant::now())
    }

    fn issue_at(&self, now: Instant) -> String {
        let mut raw = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut raw);
        let key = hex::encode(raw);
        if let Ok(mut links) = self.links.lock() {
            links.retain(|_, until| *until > now);
            if links.len() >= MAX_LINKS {
                if let Some(oldest) = links
                    .iter()
                    .min_by_key(|(_, t)| **t)
                    .map(|(k, _)| k.clone())
                {
                    links.remove(&oldest);
                }
            }
            links.insert(key.clone(), now + LINK_LIFE);
        }
        key
    }

    pub fn admits(&self, key: &str) -> bool {
        self.admits_at(key, Instant::now())
    }

    fn admits_at(&self, key: &str, now: Instant) -> bool {
        self.links
            .lock()
            .map(|links| links.get(key).is_some_and(|until| *until > now))
            .unwrap_or(false)
    }

    /// The signed build for `version`, from memory or from the releases at
    /// `releases`: that version's own, else the newest. None when neither
    /// answers with an add-on.
    pub async fn file(
        &self,
        http: &reqwest::Client,
        releases: &str,
        version: &str,
    ) -> Option<Vec<u8>> {
        let mut held = self.file.lock().await;
        if let Some((v, bytes)) = held.as_ref() {
            if v == version {
                return Some(bytes.clone());
            }
        }
        let base = releases.trim_end_matches('/');
        for url in [
            format!("{base}/download/v{version}/{FILE}"),
            latest(releases),
        ] {
            if let Some(bytes) = fetch(http, &url).await {
                *held = Some((version.to_string(), bytes.clone()));
                return Some(bytes);
            }
        }
        None
    }
}

/// The newest release's copy, which is where a browser goes when Frog has none.
pub fn latest(releases: &str) -> String {
    format!("{}/latest/download/{FILE}", releases.trim_end_matches('/'))
}

async fn fetch(http: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let res = match http.get(url).send().await {
        Ok(res) if res.status().is_success() => res,
        Ok(res) => {
            tracing::debug!(status = %res.status(), "no extension build at {url}");
            return None;
        }
        Err(e) => {
            tracing::warn!(error = %e, "could not fetch the extension build");
            return None;
        }
    };
    let bytes = res.bytes().await.ok()?;
    // An add-on is a zip; a release page or an error page is not.
    (bytes.starts_with(b"PK\x03\x04") && bytes.len() <= MAX_BYTES).then(|| bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_works_for_an_hour_and_no_longer() {
        let addon = Addon::default();
        let start = Instant::now();
        let key = addon.issue_at(start);
        assert!(addon.admits_at(&key, start + Duration::from_secs(60)));
        assert!(!addon.admits_at(&key, start + LINK_LIFE + Duration::from_secs(1)));
        assert!(!addon.admits_at("made-up", start));
        assert_ne!(addon.issue_at(start), key);
    }

    #[test]
    fn links_never_pile_up() {
        let addon = Addon::default();
        let start = Instant::now();
        let first = addon.issue_at(start);
        for n in 0..MAX_LINKS + 50 {
            addon.issue_at(start + Duration::from_millis(n as u64 + 1));
        }
        assert!(addon.links.lock().unwrap().len() <= MAX_LINKS);
        assert!(!addon.admits_at(&first, start), "the oldest went first");
    }

    #[test]
    fn the_newest_release_is_the_fallback() {
        assert_eq!(
            latest("https://github.com/o/r/releases/"),
            "https://github.com/o/r/releases/latest/download/frog-extension-firefox.xpi"
        );
    }
}
