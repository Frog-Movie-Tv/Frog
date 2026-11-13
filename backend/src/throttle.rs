//! Counting attempts so a door can shut for a while.
//!
//! In memory is the right place for this: one process, and a restart clearing
//! it is harmless. Memory is also the thing to guard, because what fills it is
//! chosen by whoever is knocking. So a key is a fixed-size hash, however long
//! the name or header it came from; a refused attempt stores nothing; and the
//! whole map has a ceiling.
//!
//! An attempt is counted as it starts, not once it has failed. Checking first
//! and counting after would let a burst through whole: every request in it
//! passes the check before the slow password work of the first one is done.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A hash rather than the text, so a key costs the same whatever was sent.
pub type Key = [u8; 32];

type Attempts = HashMap<Key, Vec<Instant>>;

/// How many keys are held at once. About 2 MB at its fullest. Each caller
/// address can only add a handful of keys a window, so reaching it takes
/// well over a thousand addresses.
const MAX_KEYS: usize = 10_000;

/// Sign-ins allowed before the door shuts for a while.
const SIGN_IN_ATTEMPTS: usize = 8;
const SIGN_IN_WINDOW: Duration = Duration::from_secs(10 * 60);

/// Tries of a reset link, per caller address. The code cannot be guessed, so
/// this only has to stop anyone trying.
const RESET_ATTEMPTS: usize = 8;
const RESET_WINDOW: Duration = Duration::from_secs(10 * 60);

/// Wrong extension tokens, per caller address. A token is 256 random bits, so
/// this, like the reset link's, only has to stop anyone trying. Every request
/// is admitted and a good token hands its attempt back, so the room is for an
/// extension's own bursts.
const TOKEN_ATTEMPTS: usize = 20;
const TOKEN_WINDOW: Duration = Duration::from_secs(10 * 60);

fn key(kind: &str, value: &str) -> Key {
    Sha256::new()
        .chain_update(kind)
        .chain_update(":")
        .chain_update(value)
        .finalize()
        .into()
}

/// Who to throttle at sign-in. Both are checked: the account stops someone
/// guessing one password from everywhere, and the address stops one caller
/// working through every account.
pub fn sign_in_keys(username: &str, client: &str) -> [Key; 2] {
    [
        key("user", &username.trim().to_lowercase()),
        key("ip", client),
    ]
}

/// Who to throttle at a reset link: a code names nobody until it is right, so
/// the address is all there is to count.
pub fn reset_keys(client: &str) -> [Key; 1] {
    [key("reset", client)]
}

/// Who to throttle when a token is wrong: it names nobody until it is right.
pub fn token_keys(client: &str) -> [Key; 1] {
    [key("token", client)]
}

pub struct Throttle {
    limit: usize,
    window: Duration,
    cap: usize,
    attempts: Mutex<Attempts>,
}

impl Throttle {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self::with_cap(limit, window, MAX_KEYS)
    }

    pub fn for_sign_in() -> Self {
        Self::new(SIGN_IN_ATTEMPTS, SIGN_IN_WINDOW)
    }

    pub fn for_resets() -> Self {
        Self::new(RESET_ATTEMPTS, RESET_WINDOW)
    }

    pub fn for_tokens() -> Self {
        Self::new(TOKEN_ATTEMPTS, TOKEN_WINDOW)
    }

    fn with_cap(limit: usize, window: Duration, cap: usize) -> Self {
        Self {
            limit,
            window,
            cap,
            attempts: Mutex::new(HashMap::new()),
        }
    }

    /// Count an attempt, or, when any key has had too many, say how many
    /// seconds to wait and count nothing. One step under one lock, so requests
    /// arriving together cannot all slip through.
    pub fn admit(&self, keys: &[Key]) -> Result<(), u64> {
        self.admit_at(keys, Instant::now())
    }

    fn admit_at(&self, keys: &[Key], now: Instant) -> Result<(), u64> {
        let Ok(mut attempts) = self.attempts.lock() else {
            return Ok(());
        };
        let mut wait = None;
        for key in keys {
            let Some(times) = attempts.get_mut(key) else {
                continue;
            };
            times.retain(|t| now.duration_since(*t) < self.window);
            if times.is_empty() {
                attempts.remove(key);
            } else if times.len() >= self.limit {
                let left = (self.window - now.duration_since(times[0]))
                    .as_secs()
                    .max(1);
                wait = Some(wait.map_or(left, |w: u64| w.max(left)));
            }
        }
        if let Some(seconds) = wait {
            return Err(seconds);
        }
        for key in keys {
            if attempts.len() >= self.cap && !attempts.contains_key(key) {
                self.make_room(&mut attempts, now);
            }
            attempts.entry(*key).or_default().push(now);
        }
        Ok(())
    }

    /// Take back the attempt just admitted, because it was not a failed one:
    /// the password was right and only the next step is left.
    pub fn release(&self, keys: &[Key]) {
        if let Ok(mut attempts) = self.attempts.lock() {
            for key in keys {
                if let Some(times) = attempts.get_mut(key) {
                    times.pop();
                    if times.is_empty() {
                        attempts.remove(key);
                    }
                }
            }
        }
    }

    /// Expired keys first. If every key is still live, the one that has been
    /// quiet longest goes, so a flood of new names reaches the account somebody
    /// is working on right now last of all.
    fn make_room(&self, attempts: &mut Attempts, now: Instant) {
        Self::drop_expired(attempts, self.window, now);
        if attempts.len() >= self.cap {
            let quietest = attempts
                .iter()
                .min_by_key(|(_, times)| times.last().copied())
                .map(|(key, _)| *key);
            if let Some(key) = quietest {
                attempts.remove(&key);
            }
        }
    }

    fn drop_expired(attempts: &mut Attempts, window: Duration, now: Instant) {
        attempts.retain(|_, times| {
            times.retain(|t| now.duration_since(*t) < window);
            !times.is_empty()
        });
    }

    pub fn clear(&self, keys: &[Key]) {
        if let Ok(mut attempts) = self.attempts.lock() {
            for key in keys {
                attempts.remove(key);
            }
        }
    }

    /// Remove what has expired. The worker calls this, so keys nobody asks
    /// about again do not sit here until the ceiling pushes them out.
    pub fn sweep(&self) {
        self.sweep_at(Instant::now());
    }

    fn sweep_at(&self, now: Instant) {
        if let Ok(mut attempts) = self.attempts.lock() {
            Self::drop_expired(&mut attempts, self.window, now);
        }
    }

    pub fn len(&self) -> usize {
        self.attempts.lock().map_or(0, |a| a.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(600);

    #[test]
    fn each_account_and_address_gets_its_own_key() {
        let a = sign_in_keys("Zjef", "10.0.0.1");
        let b = sign_in_keys(" zjef ", "10.0.0.2");
        // Case does not make a second account key...
        assert_eq!(a[0], b[0]);
        // ...but a different address is counted separately.
        assert_ne!(a[1], b[1]);
        // A name and an address that read alike are not the same key.
        assert_ne!(sign_in_keys("x", "x")[0], sign_in_keys("x", "x")[1]);
    }

    #[test]
    fn the_door_shuts_at_the_limit_and_opens_after_the_window() {
        let throttle = Throttle::new(3, WINDOW);
        let keys = [key("user", "a")];
        let start = Instant::now();
        for _ in 0..3 {
            assert_eq!(throttle.admit_at(&keys, start), Ok(()));
        }
        assert_eq!(throttle.admit_at(&keys, start), Err(600));
        assert_eq!(
            throttle.admit_at(&keys, start + Duration::from_secs(590)),
            Err(10)
        );
        assert_eq!(throttle.admit_at(&keys, start + WINDOW), Ok(()));
    }

    #[test]
    fn requests_arriving_together_cannot_pass_the_limit() {
        let throttle = std::sync::Arc::new(Throttle::new(8, WINDOW));
        let keys = [key("user", "a"), key("ip", "b")];
        let admitted: usize = std::thread::scope(|scope| {
            let threads: Vec<_> = (0..32)
                .map(|_| {
                    let throttle = throttle.clone();
                    scope.spawn(move || (0..10).filter(|_| throttle.admit(&keys).is_ok()).count())
                })
                .collect();
            threads.into_iter().map(|t| t.join().unwrap()).sum()
        });
        assert_eq!(admitted, 8);
    }

    #[test]
    fn a_refused_attempt_stores_nothing() {
        let throttle = Throttle::new(1, WINDOW);
        let address = key("ip", "a");
        assert_eq!(throttle.admit(&[key("user", "first"), address]), Ok(()));
        for n in 0..500 {
            assert!(throttle
                .admit(&[key("user", &n.to_string()), address])
                .is_err());
        }
        assert_eq!(throttle.len(), 2);
    }

    #[test]
    fn a_shut_door_on_one_key_counts_nothing_on_the_other() {
        let throttle = Throttle::new(1, WINDOW);
        let account = key("user", "a");
        let (first, second) = (key("ip", "b"), key("ip", "c"));
        assert_eq!(throttle.admit(&[account, first]), Ok(()));
        assert!(throttle.admit(&[account, second]).is_err());
        assert_eq!(throttle.admit(&[second]), Ok(()));
    }

    #[test]
    fn a_released_attempt_does_not_count() {
        let throttle = Throttle::new(2, WINDOW);
        let keys = [key("user", "a")];
        for _ in 0..10 {
            assert_eq!(throttle.admit(&keys), Ok(()));
            throttle.release(&keys);
        }
        assert!(throttle.is_empty());
    }

    #[test]
    fn the_sweep_removes_what_has_expired_and_keeps_the_rest() {
        let throttle = Throttle::new(3, WINDOW);
        let start = Instant::now();
        let (old, new) = ([key("user", "old")], [key("user", "new")]);
        assert_eq!(throttle.admit_at(&old, start), Ok(()));
        assert_eq!(
            throttle.admit_at(&new, start + Duration::from_secs(300)),
            Ok(())
        );
        throttle.sweep_at(start + WINDOW);
        assert_eq!(throttle.len(), 1);
        assert!(throttle.attempts.lock().unwrap().contains_key(&new[0]));
    }

    #[test]
    fn the_map_never_passes_its_ceiling_and_the_quietest_key_goes_first() {
        let throttle = Throttle::with_cap(3, WINDOW, 100);
        let start = Instant::now();
        let oldest = [key("user", "oldest")];
        let target = [key("user", "target")];
        assert_eq!(throttle.admit_at(&oldest, start), Ok(()));
        for n in 0..98u64 {
            let at = start + Duration::from_millis(n + 1);
            assert_eq!(
                throttle.admit_at(&[key("user", &n.to_string())], at),
                Ok(())
            );
        }
        let at = start + Duration::from_secs(1);
        assert_eq!(throttle.admit_at(&target, at), Ok(()));
        assert_eq!(throttle.len(), 100);

        assert_eq!(throttle.admit_at(&[key("user", "new")], at), Ok(()));
        assert_eq!(throttle.len(), 100);
        let attempts = throttle.attempts.lock().unwrap();
        assert!(!attempts.contains_key(&oldest[0]));
        assert!(attempts.contains_key(&target[0]));
        drop(attempts);

        for n in 0..5_000u64 {
            let at = start + Duration::from_secs(2) + Duration::from_millis(n);
            let _ = throttle.admit_at(&[key("flood", &n.to_string())], at);
            assert!(throttle.len() <= 100);
        }
    }

    #[test]
    fn at_the_ceiling_expired_keys_go_before_live_ones() {
        let throttle = Throttle::with_cap(3, WINDOW, 2);
        let start = Instant::now();
        let (old, live, new) = (
            [key("user", "old")],
            [key("user", "live")],
            [key("user", "new")],
        );
        assert_eq!(throttle.admit_at(&old, start), Ok(()));
        assert_eq!(
            throttle.admit_at(&live, start + Duration::from_secs(599)),
            Ok(())
        );
        assert_eq!(throttle.admit_at(&new, start + WINDOW), Ok(()));
        let attempts = throttle.attempts.lock().unwrap();
        assert!(attempts.contains_key(&live[0]));
        assert!(attempts.contains_key(&new[0]));
    }
}
