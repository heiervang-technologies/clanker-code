CREATE TABLE phase2_scope_outputs (
    scope_key TEXT NOT NULL,
    clanker_id TEXT NOT NULL,
    project_key TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    source_updated_at INTEGER NOT NULL,
    PRIMARY KEY (scope_key, thread_id)
);

CREATE INDEX idx_phase2_scope_outputs_thread
    ON phase2_scope_outputs(thread_id, scope_key);
