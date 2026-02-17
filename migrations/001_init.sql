CREATE TABLE IF NOT EXISTS runs (
    id          TEXT PRIMARY KEY,
    ts          INTEGER NOT NULL,
    project     TEXT NOT NULL,
    env         TEXT NOT NULL,
    request_name TEXT NOT NULL,
    method      TEXT NOT NULL,
    url         TEXT NOT NULL,
    status      INTEGER,
    duration_ms INTEGER NOT NULL,
    error       TEXT
);

CREATE TABLE IF NOT EXISTS payloads (
    run_id            TEXT PRIMARY KEY REFERENCES runs(id),
    request_headers   TEXT NOT NULL,
    request_body      TEXT,
    response_headers  TEXT NOT NULL,
    response_body     TEXT
);

CREATE INDEX IF NOT EXISTS idx_runs_ts ON runs(ts);
CREATE INDEX IF NOT EXISTS idx_runs_request_ts ON runs(request_name, ts);
CREATE INDEX IF NOT EXISTS idx_runs_status_ts ON runs(status, ts);
