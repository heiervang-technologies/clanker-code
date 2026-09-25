CREATE TABLE thread_memory_scopes (
    thread_id TEXT PRIMARY KEY,
    clanker_id TEXT,
    project_key TEXT NOT NULL,
    parent_thread_id TEXT,
    recorded_at INTEGER NOT NULL
);

ALTER TABLE stage1_outputs ADD COLUMN clanker_id TEXT;
ALTER TABLE stage1_outputs ADD COLUMN project_key TEXT;
ALTER TABLE stage1_outputs ADD COLUMN visibility TEXT NOT NULL DEFAULT 'anonymous_legacy'
    CHECK (visibility IN (
        'private_character',
        'project_shared',
        'global_user_preference',
        'anonymous_legacy'
    ));
ALTER TABLE stage1_outputs ADD COLUMN parent_thread_id TEXT;
ALTER TABLE stage1_outputs ADD COLUMN citation_path TEXT;

CREATE INDEX idx_stage1_outputs_character_project
    ON stage1_outputs(
        clanker_id,
        project_key,
        visibility,
        source_updated_at DESC,
        thread_id ASC
    );

CREATE INDEX idx_stage1_outputs_project_visibility
    ON stage1_outputs(
        project_key,
        visibility,
        source_updated_at DESC,
        thread_id ASC
    );
