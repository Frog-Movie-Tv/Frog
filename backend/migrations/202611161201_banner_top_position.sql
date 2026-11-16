-- Which band of the banner shows across the top, kept apart from the
-- column's strip (banner_position) so each layout keeps its own. Nothing
-- falls back to banner_position, which both layouts shared before.
ALTER TABLE app_user ADD COLUMN banner_top_position TEXT;
