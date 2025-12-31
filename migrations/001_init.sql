CREATE TABLE IF NOT EXISTS checks (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  url TEXT NOT NULL,
  interval_seconds INTEGER NOT NULL,
  alert_email TEXT,
  is_active INTEGER NOT NULL DEFAULT 1,
  last_status TEXT,
  last_checked_at TEXT
);

CREATE TABLE IF NOT EXISTS check_results (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  check_id TEXT NOT NULL,
  checked_at TEXT NOT NULL,
  status TEXT NOT NULL,
  http_status INTEGER,
  latency_ms INTEGER,
  error TEXT,
  FOREIGN KEY(check_id) REFERENCES checks(id)
);

CREATE INDEX IF NOT EXISTS idx_results_check_time ON check_results(check_id, checked_at);
