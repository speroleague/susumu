CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE users (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    roles TEXT[] NOT NULL DEFAULT ARRAY['reviewer'],
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX sessions_user_id_idx ON sessions(user_id);
CREATE INDEX sessions_expires_at_idx ON sessions(expires_at);

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    actor_user_id TEXT REFERENCES users(id),
    action TEXT NOT NULL,
    project_key TEXT,
    target TEXT,
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX audit_events_project_key_idx ON audit_events(project_key);
CREATE INDEX audit_events_created_at_idx ON audit_events(created_at);
