use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};

use crate::{
    model::ReviewThread, parse_decisions, parse_expectations, parse_review_threads,
    parse_verifications, parse_works,
};

use super::{
    AppState,
    auth::authenticated_user,
    db::{SearchRecord, SearchRequest},
    error::ServerError,
};

#[derive(Debug, Deserialize)]
pub(crate) struct SearchQuery {
    #[serde(default)]
    q: String,
    base_branch: Option<String>,
    kind: Option<String>,
    status: Option<String>,
    owner: Option<String>,
    path: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct SearchResponse {
    results: Vec<SearchResult>,
    total: i64,
    limit: i64,
    offset: i64,
    base_branch: String,
    indexed_head_sha: Option<String>,
}

#[derive(Debug, Serialize)]
struct SearchResult {
    id: String,
    kind: String,
    status: String,
    title: String,
    detail: String,
    comment_kind: Option<String>,
    owner: Option<String>,
    anchor: Option<String>,
    parent: Option<String>,
    expectation_id: Option<String>,
    path: String,
    source_line: Option<i32>,
}

pub(crate) async fn search(
    State(state): State<AppState>,
    Path(project_key): Path<String>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, ServerError> {
    authenticated_user(&state.database, &headers).await?;
    let repository = state
        .database
        .project_repository(&project_key)
        .await?
        .ok_or(ServerError::NotFound)?;
    let base_branch = query.base_branch.unwrap_or_else(|| {
        repository
            .allowed_base_branches
            .first()
            .cloned()
            .unwrap_or_default()
    });
    if !repository
        .allowed_base_branches
        .iter()
        .any(|branch| branch == &base_branch)
    {
        return Err(ServerError::BadRequest(
            "base_branch is not configured for this repository".to_owned(),
        ));
    }
    let limit = query.limit.clamp(1, 50);
    let offset = query.offset.clamp(0, 10_000);
    let request = SearchRequest {
        project_key: project_key.clone(),
        base_branch: base_branch.clone(),
        query: query.q.trim().chars().take(200).collect(),
        kind: bounded_filter(query.kind),
        status: bounded_filter(query.status),
        owner: bounded_filter(query.owner),
        path: bounded_filter(query.path),
        limit,
        offset,
    };
    let (records, total, indexed_head_sha) = state.database.search_records(&request).await?;
    Ok(Json(SearchResponse {
        results: records.into_iter().map(SearchResult::from).collect(),
        total,
        limit,
        offset,
        base_branch,
        indexed_head_sha,
    }))
}

pub(crate) fn documents_for_sidecar(
    path: &str,
    content: &str,
) -> anyhow::Result<Vec<SearchRecord>> {
    let mut records = Vec::new();
    match path.rsplit('/').next().unwrap_or(path) {
        "expectations.susu" => {
            records.extend(parse_expectations(content)?.into_iter().map(|record| {
                SearchRecord::new(
                    path,
                    record.id,
                    "expectation",
                    record.status.to_string(),
                    record.title,
                    record.detail,
                )
            }));
        }
        "verifications.susu" => {
            records.extend(parse_verifications(content)?.into_iter().map(|record| {
                let id = record.id.clone();
                SearchRecord::new(
                    path,
                    record.id,
                    "verification",
                    record.status.to_string(),
                    id,
                    record.detail,
                )
                .with_expectation(Some(record.expectation_id))
            }));
        }
        "work.susu" => records.extend(parse_works(content)?.into_iter().map(|record| {
            SearchRecord::new(
                path,
                record.id,
                "work",
                record.status.to_string(),
                record.title,
                record.detail,
            )
            .with_expectation(record.expectation_id)
        })),
        "decisions.susu" => records.extend(parse_decisions(content)?.into_iter().map(|record| {
            SearchRecord::new(
                path,
                record.id,
                "decision",
                record.status.to_string(),
                record.title,
                record.detail,
            )
        })),
        "review.susu" | "reviews.susu" => records.extend(
            parse_review_threads(content)?
                .into_iter()
                .map(|record| review_record(path, record)),
        ),
        _ => {}
    }
    Ok(records)
}

fn review_record(path: &str, record: ReviewThread) -> SearchRecord {
    SearchRecord::new(
        path,
        record.id,
        "review",
        record.status.to_string(),
        record.title,
        record.detail,
    )
    .with_owner(record.owner)
    .with_anchor(record.anchor.map(|anchor| anchor.to_string()))
    .with_parent(record.parent)
    .with_kind(record.kind)
}

fn bounded_filter(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().chars().take(120).collect::<String>();
        (!value.is_empty()).then_some(value)
    })
}

fn default_limit() -> i64 {
    25
}

impl From<SearchRecord> for SearchResult {
    fn from(record: SearchRecord) -> Self {
        Self {
            id: record.record_id,
            kind: record.kind,
            status: record.status,
            title: record.title,
            detail: record.detail,
            comment_kind: record.comment_kind,
            owner: record.owner,
            anchor: record.anchor,
            parent: record.parent,
            expectation_id: record.expectation_id,
            path: record.path,
            source_line: record.source_line,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::documents_for_sidecar;

    #[test]
    fn indexes_typed_review_records_without_raw_content() {
        let records = documents_for_sidecar(
            "review.susu",
            "review r_one target=project subject=- anchor=expectation:e_one parent=- kind=question status=open owner=\"ops\" source=\"human:test\" title=\"Clarify ownership\" detail=\"Who owns this?\";",
        )
        .expect("review parses");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record_id, "r_one");
        assert_eq!(records[0].anchor.as_deref(), Some("expectation:e_one"));
        assert!(!records[0].detail.contains("review r_one"));
    }

    #[test]
    fn ignores_unconfigured_sidecars() {
        assert!(
            documents_for_sidecar("notes.txt", "anything")
                .unwrap()
                .is_empty()
        );
    }
}
