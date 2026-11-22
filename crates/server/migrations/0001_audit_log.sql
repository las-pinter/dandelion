-- Append-only audit log for every control action.
CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at INTEGER NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT NOT NULL,
    detail TEXT
);

CREATE INDEX audit_log_occurred_at_idx ON audit_log (occurred_at);
