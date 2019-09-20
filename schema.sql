CREATE TABLE backup_requests (
id         TEXT PRIMARY KEY,
token      TEXT NOT NULL,
created_at TEXT NOT NULL,
file       TEXT,
last_error TEXT,
status     TEXT NOT NULL
)
;
