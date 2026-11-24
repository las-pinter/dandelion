-- Browser push subscriptions. The endpoint is a capability URL for one
-- browser, so it is never written to the audit log.

CREATE TABLE push_subscriptions (
    endpoint TEXT PRIMARY KEY,
    p256dh TEXT NOT NULL,
    auth TEXT NOT NULL,
    created_ms INTEGER NOT NULL,
    last_success_ms INTEGER,
    failure_count INTEGER NOT NULL DEFAULT 0
) STRICT;
