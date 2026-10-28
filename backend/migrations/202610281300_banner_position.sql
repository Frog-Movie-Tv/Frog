-- Which part of a banner shows, as a CSS position ("50% 30%"): a banner is
-- far wider than it is tall, so most pictures need moving to look right.
ALTER TABLE app_user ADD COLUMN banner_position TEXT;
