ALTER TABLE project_search_records
    ADD COLUMN record_key TEXT;

UPDATE project_search_records
SET record_key = path || '::' || kind || '::' || record_id
WHERE record_key IS NULL;

ALTER TABLE project_search_records
    ALTER COLUMN record_key SET NOT NULL;

ALTER TABLE project_search_records
    DROP CONSTRAINT project_search_records_pkey;

ALTER TABLE project_search_records
    ADD CONSTRAINT project_search_records_pkey PRIMARY KEY (project_key, base_branch, record_key);

ALTER TABLE project_sync_states
    ADD COLUMN base_sha TEXT,
    ADD COLUMN head_sha TEXT,
    ADD COLUMN observed_base_sha TEXT,
    ADD COLUMN rebase_required BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN conflict_detail TEXT;

CREATE INDEX project_sync_rebase_required_idx
    ON project_sync_states (project_key, rebase_required)
    WHERE rebase_required = TRUE;
