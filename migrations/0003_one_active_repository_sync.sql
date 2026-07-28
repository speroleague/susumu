CREATE UNIQUE INDEX project_sync_one_active_lifecycle_idx
    ON project_sync_states (project_key)
    WHERE status IN ('syncing', 'pending');
