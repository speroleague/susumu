CREATE TABLE projects (
    project_key TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    provider TEXT NOT NULL CHECK (provider = 'github'),
    repository_owner TEXT NOT NULL,
    repository_name TEXT NOT NULL,
    installation_id BIGINT NOT NULL,
    allowed_base_branches TEXT[] NOT NULL,
    sidecar_paths TEXT[] NOT NULL DEFAULT ARRAY['expectations.susu', 'work.susu'],
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_by_user_id TEXT NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, repository_owner, repository_name)
);

CREATE TABLE project_sync_states (
    project_key TEXT NOT NULL REFERENCES projects(project_key) ON DELETE CASCADE,
    base_branch TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'idle',
    active_branch TEXT,
    pull_request_number BIGINT,
    last_error TEXT,
    last_successful_sync_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (project_key, base_branch),
    CHECK (status IN ('idle', 'queued', 'syncing', 'pending', 'conflict', 'error', 'merged'))
);

CREATE INDEX projects_active_idx ON projects(active);
CREATE INDEX project_sync_states_status_idx ON project_sync_states(status);
