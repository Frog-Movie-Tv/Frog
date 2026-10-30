-- When each account last signed in. Administration used to read this off the
-- live sessions, so anyone who had signed out showed as never having signed in.
ALTER TABLE app_user ADD COLUMN last_sign_in_at TEXT;

UPDATE app_user
   SET last_sign_in_at = (SELECT MAX(s.created_at) FROM session s WHERE s.user_id = app_user.id);
