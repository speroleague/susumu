use std::collections::{BTreeMap, HashMap};

use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};

use super::{
    AppState,
    auth::{authenticated_user, require_csrf},
    db::{ProjectRepositoryRecord, SearchRecord, SyncStateRecord},
    error::ServerError,
    github::RepositoryFile,
    search::documents_for_sidecar,
    worker::{MaterializedChange, synchronize},
};

pub(crate) async fn refresh_sync_provenance(
    database: &super::db::Database,
    github: &super::github::GithubAppClient,
    project_key: &str,
    base_branch: &str,
) -> anyhow::Result<()> {
    let Some(sync) = database.sync_state(project_key, base_branch).await? else {
        return Ok(());
    };
    if sync.status != "pending" {
        return Ok(());
    }
    let Some(base_sha) = sync.base_sha.as_deref() else {
        return Ok(());
    };
    let Some(repository) = database.project_repository(project_key).await? else {
        return Ok(());
    };
    let token = github
        .installation_token(repository.installation_id)
        .await?;
    let observed = github
        .branch_sha(
            &token.token,
            &repository.repository_owner,
            &repository.repository_name,
            base_branch,
        )
        .await?;
    if observed != base_sha {
        database
            .mark_base_advanced(
                project_key,
                base_branch,
                &observed,
                &format!("base branch advanced from {base_sha} to {observed}"),
            )
            .await?;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub(crate) struct ConflictResponse {
    project_key: String,
    base_branch: String,
    active_branch: String,
    pull_request_number: i64,
    files: Vec<ConflictFileResponse>,
}

#[derive(Debug, Serialize)]
struct ConflictFileResponse {
    path: String,
    records: Vec<ConflictRecordResponse>,
}

#[derive(Debug, Serialize)]
struct ConflictRecordResponse {
    id: String,
    base: Option<ConflictRecordSide>,
    active: Option<ConflictRecordSide>,
    changed: bool,
}

#[derive(Debug, Serialize)]
struct ConflictRecordSide {
    kind: String,
    status: String,
    title: String,
    detail: String,
    owner: Option<String>,
    anchor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResolveConflictRequest {
    base_branch: String,
    choices: Vec<ConflictChoiceRequest>,
}

#[derive(Debug, Deserialize)]
struct ConflictChoiceRequest {
    path: String,
    record_id: String,
    choice: String,
}

#[derive(Debug)]
struct ConflictFileSnapshot {
    path: String,
    base: Option<RepositoryFile>,
    active: Option<RepositoryFile>,
}

pub(crate) async fn conflict(
    State(state): State<AppState>,
    Path(project_key): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ConflictResponse>, ServerError> {
    authenticated_user(&state.database, &headers).await?;
    let (sync, _, files) = load_conflict_snapshot(&state, &project_key).await?;
    Ok(Json(ConflictResponse {
        project_key,
        base_branch: sync.base_branch,
        active_branch: sync.active_branch.ok_or(ServerError::NotFound)?,
        pull_request_number: sync.pull_request_number.ok_or(ServerError::NotFound)?,
        files: files.into_iter().map(conflict_file_response).collect(),
    }))
}

pub(crate) async fn resolve_conflict(
    State(state): State<AppState>,
    Path(project_key): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ResolveConflictRequest>,
) -> Result<Json<SyncResponse>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    require_csrf(&headers)?;
    let (sync, repository, files) = load_conflict_snapshot(&state, &project_key).await?;
    if sync.base_branch != request.base_branch {
        return Err(ServerError::BadRequest(
            "base_branch does not match the active conflict".to_owned(),
        ));
    }
    let choices = request
        .choices
        .into_iter()
        .map(|choice| ((choice.path, choice.record_id), choice.choice))
        .collect::<HashMap<_, _>>();
    let changes = files
        .iter()
        .map(|file| {
            Ok(MaterializedChange {
                path: file.path.clone(),
                content: merge_conflict_file(file, &choices)
                    .map_err(|error| ServerError::BadRequest(error.to_string()))?,
            })
        })
        .collect::<Result<Vec<_>, ServerError>>()?;
    let github = state
        .github
        .read()
        .await
        .client(repository.github_connection_id.as_deref())
        .ok_or_else(|| {
            ServerError::ServiceUnavailable("GitHub App is not configured".to_owned())
        })?;
    let token = github
        .installation_token(repository.installation_id)
        .await
        .map_err(ServerError::Internal)?;
    let base_sha = github
        .branch_sha(
            &token.token,
            &repository.repository_owner,
            &repository.repository_name,
            &sync.base_branch,
        )
        .await
        .map_err(ServerError::Internal)?;
    state
        .database
        .prepare_conflict_resolution(&project_key, &sync.base_branch, &base_sha, &user.id)
        .await?;
    synchronize(
        &state.database,
        &github,
        &project_key,
        &sync.base_branch,
        &changes,
        Some(&user.id),
    )
    .await
    .map_err(ServerError::Internal)?;
    let next = state
        .database
        .sync_state(&project_key, &sync.base_branch)
        .await?
        .ok_or(ServerError::NotFound)?;
    Ok(Json(SyncResponse::from_parts(project_key, next)))
}

async fn load_conflict_snapshot(
    state: &AppState,
    project_key: &str,
) -> Result<
    (
        SyncStateRecord,
        ProjectRepositoryRecord,
        Vec<ConflictFileSnapshot>,
    ),
    ServerError,
> {
    let sync = state
        .database
        .list_sync_states(project_key)
        .await?
        .into_iter()
        .find(|state| state.rebase_required)
        .ok_or_else(|| {
            ServerError::Conflict("this repository has no synchronization conflict".to_owned())
        })?;
    let active_branch = sync.active_branch.clone().ok_or(ServerError::NotFound)?;
    let repository = state
        .database
        .project_repository(project_key)
        .await?
        .ok_or(ServerError::NotFound)?;
    let github = state
        .github
        .read()
        .await
        .client(repository.github_connection_id.as_deref())
        .ok_or_else(|| {
            ServerError::ServiceUnavailable("GitHub App is not configured".to_owned())
        })?;
    let token = github
        .installation_token(repository.installation_id)
        .await
        .map_err(ServerError::Internal)?;
    let mut files = Vec::with_capacity(repository.sidecar_paths.len());
    for path in &repository.sidecar_paths {
        let base = github
            .read_file(
                &token.token,
                &repository.repository_owner,
                &repository.repository_name,
                path,
                &sync.base_branch,
            )
            .await
            .map_err(ServerError::Internal)?;
        let active = github
            .read_file(
                &token.token,
                &repository.repository_owner,
                &repository.repository_name,
                path,
                &active_branch,
            )
            .await
            .map_err(ServerError::Internal)?;
        files.push(ConflictFileSnapshot {
            path: path.clone(),
            base,
            active,
        });
    }
    Ok((sync, repository, files))
}

fn conflict_file_response(file: ConflictFileSnapshot) -> ConflictFileResponse {
    let base_records = records_for_file(&file.path, file.base.as_ref());
    let active_records = records_for_file(&file.path, file.active.as_ref());
    let mut records = BTreeMap::new();
    for record in base_records {
        let id = record.record_id.clone();
        records.entry(id).or_insert((None, None)).0 = Some(record);
    }
    for record in active_records {
        let id = record.record_id.clone();
        records.entry(id).or_insert((None, None)).1 = Some(record);
    }
    ConflictFileResponse {
        path: file.path,
        records: records
            .into_iter()
            .map(|(id, (base, active))| {
                let changed =
                    base.as_ref().map(record_signature) != active.as_ref().map(record_signature);
                ConflictRecordResponse {
                    id,
                    base: base.map(conflict_record_side),
                    active: active.map(conflict_record_side),
                    changed,
                }
            })
            .collect(),
    }
}

fn records_for_file(path: &str, file: Option<&RepositoryFile>) -> Vec<SearchRecord> {
    file.and_then(|file| String::from_utf8(file.content.clone()).ok())
        .and_then(|content| documents_for_sidecar(path, &content).ok())
        .unwrap_or_default()
}

fn record_signature(record: &SearchRecord) -> (&str, &str, &str, &str) {
    (&record.kind, &record.status, &record.title, &record.detail)
}

fn conflict_record_side(record: SearchRecord) -> ConflictRecordSide {
    ConflictRecordSide {
        kind: record.kind,
        status: record.status,
        title: record.title,
        detail: record.detail,
        owner: record.owner,
        anchor: record.anchor,
    }
}

fn merge_conflict_file(
    file: &ConflictFileSnapshot,
    choices: &HashMap<(String, String), String>,
) -> anyhow::Result<Vec<u8>> {
    let base = sidecar_lines(file.base.as_ref())?;
    let active = sidecar_lines(file.active.as_ref())?;
    let mut ids = BTreeMap::new();
    for (id, line) in base {
        ids.entry(id).or_insert((None, None)).0 = Some(line);
    }
    for (id, line) in active {
        ids.entry(id).or_insert((None, None)).1 = Some(line);
    }
    let mut output = String::new();
    for (id, (base, active)) in ids {
        let line = match (base, active) {
            (Some(line), None) | (None, Some(line)) => line,
            (Some(base), Some(active)) if base == active => active,
            (Some(base), Some(active)) => match choices
                .get(&(file.path.clone(), id.clone()))
                .map(String::as_str)
            {
                Some("base") => base,
                Some("active") => active,
                Some("both") => {
                    anyhow::bail!("record {id} has the same ID on both sides; choose one version")
                }
                _ => anyhow::bail!("choose which version to keep for record {id}"),
            },
            (None, None) => continue,
        };
        output.push_str(&line);
        if !line.ends_with('\n') {
            output.push('\n');
        }
    }
    Ok(output.into_bytes())
}

fn sidecar_lines(file: Option<&RepositoryFile>) -> anyhow::Result<BTreeMap<String, String>> {
    let Some(file) = file else {
        return Ok(BTreeMap::new());
    };
    let content = String::from_utf8(file.content.clone())?;
    let mut lines = BTreeMap::new();
    for line in content.lines() {
        let mut fields = line.split_whitespace();
        let _kind = fields.next();
        let Some(id) = fields.next() else {
            continue;
        };
        lines.insert(id.to_owned(), format!("{line}\n"));
    }
    Ok(lines)
}

#[derive(Debug, Deserialize)]
pub(crate) struct QueueSyncRequest {
    base_branch: String,
    #[serde(default)]
    changes: Vec<MaterializedChangeRequest>,
}

#[derive(Debug, Deserialize)]
struct MaterializedChangeRequest {
    path: String,
    content: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncResponse {
    pub(crate) project_key: String,
    pub(crate) base_branch: String,
    pub(crate) status: String,
    pub(crate) active_branch: Option<String>,
    pub(crate) pull_request_number: Option<i64>,
    pub(crate) last_error: Option<String>,
    pub(crate) base_sha: Option<String>,
    pub(crate) head_sha: Option<String>,
    pub(crate) observed_base_sha: Option<String>,
    pub(crate) rebase_required: bool,
    pub(crate) conflict_detail: Option<String>,
}

pub(crate) async fn submit_changes(
    state: &AppState,
    project_key: &str,
    base_branch: &str,
    actor_user_id: &str,
    changes: Vec<MaterializedChange>,
) -> Result<SyncResponse, ServerError> {
    let has_changes = !changes.is_empty();
    let Some(sync) = state
        .database
        .queue_sync(project_key, base_branch, actor_user_id, has_changes)
        .await?
    else {
        return Err(ServerError::NotFound);
    };
    if sync.rebase_required {
        return Err(ServerError::Conflict(
            "project memory is locked while this repository conflict is unresolved; resolve the conflict before adding more work".to_owned(),
        ));
    }
    if has_changes && sync.status == "syncing" {
        return Err(ServerError::Conflict(
            "repository synchronization is already in progress; retry after it finishes".to_owned(),
        ));
    }
    let repository = state.database.project_repository(project_key).await?;
    let github = match repository.as_ref() {
        Some(repository) => state
            .github
            .read()
            .await
            .client(repository.github_connection_id.as_deref()),
        None => None,
    };
    if !changes.is_empty()
        && let Some(github) = github.as_ref()
    {
        synchronize(
            &state.database,
            github,
            project_key,
            base_branch,
            &changes,
            Some(actor_user_id),
        )
        .await
        .map_err(ServerError::Internal)?;
        let indexed_records = changes
            .iter()
            .flat_map(|change| {
                String::from_utf8(change.content.clone())
                    .ok()
                    .and_then(|content| documents_for_sidecar(&change.path, &content).ok())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let paths = changes
            .iter()
            .map(|change| change.path.clone())
            .collect::<Vec<_>>();
        state
            .database
            .upsert_search_records(
                project_key,
                base_branch,
                &format!("pending:{base_branch}"),
                &indexed_records,
                &paths,
            )
            .await?;
    }
    let sync = state
        .database
        .sync_state(project_key, base_branch)
        .await?
        .unwrap_or(sync);
    Ok(SyncResponse::from_parts(project_key.to_owned(), sync))
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn queue(
    State(state): State<AppState>,
    Path(project_key): Path<String>,
    headers: HeaderMap,
    Json(request): Json<QueueSyncRequest>,
) -> Result<Json<SyncResponse>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    require_csrf(&headers)?;
    let base_branch = request.base_branch.trim();
    if base_branch.is_empty() || base_branch.len() > 200 {
        return Err(ServerError::BadRequest(
            "base_branch must contain between 1 and 200 characters".to_owned(),
        ));
    }
    if request.changes.len() > 32 {
        return Err(ServerError::BadRequest(
            "changes must contain at most 32 files".to_owned(),
        ));
    }
    let changes = request
        .changes
        .into_iter()
        .map(|change| {
            if change.path.len() > 400 {
                return Err(ServerError::BadRequest(
                    "change paths must contain at most 400 characters".to_owned(),
                ));
            }
            if change.content.len() > 512 * 1024 {
                return Err(ServerError::BadRequest(
                    "change contents must contain at most 512 KiB".to_owned(),
                ));
            }
            Ok(MaterializedChange {
                path: change.path,
                content: change.content.into_bytes(),
            })
        })
        .collect::<Result<Vec<_>, ServerError>>()?;
    Ok(Json(
        submit_changes(&state, &project_key, base_branch, &user.id, changes).await?,
    ))
}

pub(crate) async fn rebase(
    State(state): State<AppState>,
    Path(project_key): Path<String>,
    headers: HeaderMap,
) -> Result<Json<SyncResponse>, ServerError> {
    authenticated_user(&state.database, &headers).await?;
    require_csrf(&headers)?;
    let base_branch = state
        .database
        .list_sync_states(&project_key)
        .await?
        .into_iter()
        .find(|state| state.rebase_required)
        .ok_or(ServerError::Conflict(
            "this repository has no synchronization conflict to update".to_owned(),
        ))?
        .base_branch;
    let sync = state
        .database
        .sync_state(&project_key, &base_branch)
        .await?
        .ok_or(ServerError::NotFound)?;
    let (Some(active_branch), Some(pull_request_number), Some(expected_head_sha)) = (
        sync.active_branch.clone(),
        sync.pull_request_number,
        sync.head_sha.clone(),
    ) else {
        return Err(ServerError::Conflict(
            "the active synchronization does not have enough provenance to update safely"
                .to_owned(),
        ));
    };
    let repository = state
        .database
        .project_repository(&project_key)
        .await?
        .ok_or(ServerError::NotFound)?;
    let github = state
        .github
        .read()
        .await
        .client(repository.github_connection_id.as_deref())
        .ok_or_else(|| {
            ServerError::ServiceUnavailable("GitHub App is not configured".to_owned())
        })?;
    let token = github
        .installation_token(repository.installation_id)
        .await
        .map_err(ServerError::Internal)?;
    let base_sha = github
        .branch_sha(
            &token.token,
            &repository.repository_owner,
            &repository.repository_name,
            &base_branch,
        )
        .await
        .map_err(ServerError::Internal)?;
    github
        .update_pull_request_branch(
            &token.token,
            &repository.repository_owner,
            &repository.repository_name,
            pull_request_number,
            &expected_head_sha,
        )
        .await
        .map_err(|error| {
            ServerError::Conflict(format!(
                "GitHub could not update the active pull request branch. Resolve the conflict in GitHub, then retry: {error}"
            ))
        })?;
    let head_sha = github
        .branch_sha(
            &token.token,
            &repository.repository_owner,
            &repository.repository_name,
            &active_branch,
        )
        .await
        .map_err(ServerError::Internal)?;
    state
        .database
        .complete_rebase(&project_key, &base_branch, &base_sha, &head_sha)
        .await?;
    let sync = state
        .database
        .sync_state(&project_key, &base_branch)
        .await?
        .ok_or(ServerError::NotFound)?;
    Ok(Json(SyncResponse::from_parts(project_key, sync)))
}

impl SyncResponse {
    fn from_parts(project_key: String, state: SyncStateRecord) -> Self {
        Self {
            project_key,
            base_branch: state.base_branch,
            status: state.status,
            active_branch: state.active_branch,
            pull_request_number: state.pull_request_number,
            last_error: state.last_error,
            base_sha: state.base_sha,
            head_sha: state.head_sha,
            observed_base_sha: state.observed_base_sha,
            rebase_required: state.rebase_required,
            conflict_detail: state.conflict_detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{ConflictFileSnapshot, QueueSyncRequest, merge_conflict_file};
    use crate::server::github::RepositoryFile;

    #[test]
    fn queue_request_keeps_the_base_branch_explicit() {
        let request: QueueSyncRequest =
            serde_json::from_str(r#"{"base_branch":"release/1"}"#).expect("queue request");
        assert_eq!(request.base_branch, "release/1");
        assert!(request.changes.is_empty());
    }

    #[test]
    fn queue_request_accepts_materialized_text_changes() {
        let request: QueueSyncRequest = serde_json::from_str(
            r#"{"base_branch":"main","changes":[{"path":"work.susu","content":"work ..."}]}"#,
        )
        .expect("queue request");
        assert_eq!(request.changes[0].path, "work.susu");
        assert_eq!(request.changes[0].content, "work ...");
    }

    #[test]
    fn conflict_merge_keeps_records_unique_to_both_sides() {
        let file = ConflictFileSnapshot {
            path: "expectations.susu".to_owned(),
            base: Some(RepositoryFile {
                sha: "base".to_owned(),
                content: b"expectation e_base target=project subject=- status=accepted source=\"test\" title=\"Base\" detail=\"Base record.\";\n".to_vec(),
            }),
            active: Some(RepositoryFile {
                sha: "active".to_owned(),
                content: b"expectation e_active target=project subject=- status=accepted source=\"test\" title=\"Active\" detail=\"Active record.\";\n".to_vec(),
            }),
        };
        let merged = merge_conflict_file(&file, &HashMap::new()).expect("merge");
        let merged = String::from_utf8(merged).expect("utf8");
        assert!(merged.contains("e_base"));
        assert!(merged.contains("e_active"));
    }

    #[test]
    fn conflict_merge_requires_a_choice_for_same_record_ids() {
        let file = ConflictFileSnapshot {
            path: "work.susu".to_owned(),
            base: Some(RepositoryFile {
                sha: "base".to_owned(),
                content:
                    b"work w_same kind=feature status=planned title=\"Base\" detail=\"Base\";\n"
                        .to_vec(),
            }),
            active: Some(RepositoryFile {
                sha: "active".to_owned(),
                content:
                    b"work w_same kind=feature status=planned title=\"Active\" detail=\"Active\";\n"
                        .to_vec(),
            }),
        };
        assert!(merge_conflict_file(&file, &HashMap::new()).is_err());
        let mut choices = HashMap::new();
        choices.insert(
            ("work.susu".to_owned(), "w_same".to_owned()),
            "active".to_owned(),
        );
        let merged = merge_conflict_file(&file, &choices).expect("selected merge");
        assert!(
            String::from_utf8(merged)
                .expect("utf8")
                .contains("title=\"Active\"")
        );
    }
}
