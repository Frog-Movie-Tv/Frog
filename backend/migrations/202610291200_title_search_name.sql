-- Shared: a title's name and original name with case and accents taken out
-- ("pokemon" for "Pokémon"), so the library filter finds it however it is
-- typed. SQL cannot take accents out, so the server fills it: on every write
-- of a title, and at startup for any row still without one (every row, the
-- first time).
ALTER TABLE title ADD COLUMN search_name TEXT;
