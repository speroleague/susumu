CREATE TABLE github_app_connections (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    name TEXT NOT NULL,
    app_id BIGINT NOT NULL CHECK (app_id > 0),
    private_key_ciphertext BYTEA NOT NULL,
    private_key_nonce BYTEA NOT NULL,
    created_by_user_id TEXT NOT NULL REFERENCES users(id),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE projects ADD COLUMN github_connection_id TEXT REFERENCES github_app_connections(id);

INSERT INTO github_app_connections (name, app_id, private_key_ciphertext, private_key_nonce, created_by_user_id)
SELECT 'Existing GitHub App', app_id, private_key_ciphertext, private_key_nonce, created_by_user_id
FROM github_app_credentials
WHERE id = TRUE
  AND NOT EXISTS (SELECT 1 FROM github_app_connections);

UPDATE projects
SET github_connection_id = (SELECT id FROM github_app_connections ORDER BY created_at LIMIT 1)
WHERE github_connection_id IS NULL;
