-- Migration 3: indexes for bounded authenticated expiry purges.

CREATE INDEX rate_limit_events_occurred_at_idx ON rate_limit_events (occurred_at);
CREATE INDEX rate_counters_window_start_idx ON rate_counters (window_start);
