-- A badge made here can be earned on its own, like Frog's: the
-- administrator picks what is counted and how far, as JSON
-- ({"count": "films", "at": 25}). Without one it is only ever given by hand.
ALTER TABLE badge ADD COLUMN condition TEXT;
