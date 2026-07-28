CREATE TABLE github_app_credentials (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    app_id BIGINT NOT NULL CHECK (app_id > 0),
    private_key_ciphertext BYTEA NOT NULL,
    private_key_nonce BYTEA NOT NULL,
    created_by_user_id TEXT NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
