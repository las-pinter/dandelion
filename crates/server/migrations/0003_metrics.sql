-- Second-resolution points for one hour and minute aggregates for one week.
-- Payloads are JSON so the sysmon types stay the single source of shape.

CREATE TABLE metric_seconds (
    recorded_at INTEGER PRIMARY KEY,
    payload TEXT NOT NULL
) STRICT;

CREATE TABLE metric_minutes (
    minute_start INTEGER PRIMARY KEY,
    payload TEXT NOT NULL
) STRICT;
