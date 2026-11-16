-- What kind of device a session is on ("phone", "tablet" or "computer"),
-- when the browser says more than its user agent does: Safari on an iPad
-- calls itself a Mac. Nothing means the user agent decides.
ALTER TABLE session ADD COLUMN device_form TEXT;
