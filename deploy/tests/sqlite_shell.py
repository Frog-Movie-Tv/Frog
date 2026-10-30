"""A stand-in for the sqlite3 shell: the two things deploy/update.sh asks of it."""
import sqlite3
import sys

db, command = sys.argv[1], sys.argv[2]
con = sqlite3.connect(db)
if command.startswith(".backup"):
    target = command.split(" ", 1)[1].strip().strip("'")
    out = sqlite3.connect(target)
    con.backup(out)
    out.close()
elif command.strip().rstrip(";").upper() == "PRAGMA USER_VERSION":
    print(con.execute("PRAGMA user_version").fetchone()[0])
else:
    sys.exit(f"unexpected: {command}")
