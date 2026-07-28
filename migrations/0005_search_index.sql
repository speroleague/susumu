CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE project_search_records (
    project_key TEXT NOT NULL REFERENCES projects(project_key) ON DELETE CASCADE,
    base_branch TEXT NOT NULL,
    record_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    comment_kind TEXT,
    status TEXT NOT NULL,
    title TEXT NOT NULL,
    detail TEXT NOT NULL,
    owner TEXT,
    anchor TEXT,
    parent TEXT,
    expectation_id TEXT,
    path TEXT NOT NULL,
    source_line INTEGER,
    head_sha TEXT NOT NULL,
    search_vector TSVECTOR GENERATED ALWAYS AS (
        to_tsvector(
            'simple',
            coalesce(record_id, '') || ' ' || coalesce(kind, '') || ' ' ||
            coalesce(status, '') || ' ' || coalesce(title, '') || ' ' ||
            coalesce(detail, '') || ' ' || coalesce(owner, '') || ' ' ||
            coalesce(comment_kind, '') || ' ' ||
            coalesce(anchor, '') || ' ' || coalesce(parent, '') || ' ' ||
            coalesce(expectation_id, '') || ' ' || coalesce(path, '')
        )
    ) STORED,
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (project_key, base_branch, record_id)
);

CREATE INDEX project_search_records_vector_idx
    ON project_search_records USING GIN (search_vector);
CREATE INDEX project_search_records_title_trgm_idx
    ON project_search_records USING GIN (title gin_trgm_ops);
CREATE INDEX project_search_records_detail_trgm_idx
    ON project_search_records USING GIN (detail gin_trgm_ops);
CREATE INDEX project_search_records_scope_idx
    ON project_search_records (project_key, base_branch, kind, status, path);
