-- Shared: when fanart.tv was last asked for a title's logo, and when it may
-- be asked again. A title with a logo is never asked; one fanart.tv had
-- nothing for waits a month.
CREATE TABLE title_logo_check (
    title_id   INTEGER PRIMARY KEY REFERENCES title (id) ON DELETE CASCADE,
    checked_at TEXT NOT NULL,
    next_at    TEXT NOT NULL
);
CREATE INDEX idx_title_logo_check_next ON title_logo_check (next_at);
