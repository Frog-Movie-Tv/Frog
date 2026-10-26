-- Two-factor sign-in with an authenticator app (TOTP, RFC 6238).
-- totp_secret is set only once a code from the app has been confirmed;
-- totp_pending holds the secret shown during setup until then.
ALTER TABLE app_user ADD COLUMN totp_secret TEXT;
ALTER TABLE app_user ADD COLUMN totp_pending TEXT;
-- The last 30-second step a code was accepted for, so a code cannot be
-- replayed within its window.
ALTER TABLE app_user ADD COLUMN totp_last_step INTEGER NOT NULL DEFAULT 0;

-- One-time codes for when the phone is lost. Only hashes are stored.
CREATE TABLE recovery_code (
    code_hash TEXT PRIMARY KEY,
    used_at   TEXT
);
