use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    model::{ExpectationTarget, ReviewAnchor, ReviewCommentKind, ReviewStatus, ReviewThread},
    parse_review_threads, write_review_threads,
};

use super::{
    AppState,
    auth::{authenticated_user, require_csrf},
    error::ServerError,
    sync::{self, SyncResponse},
    worker::MaterializedChange,
};

const REVIEW_PATH: &str = "review.susu";

#[derive(Debug, Deserialize)]
pub(crate) struct CreateThreadRequest {
    base_branch: String,
    #[serde(default)]
    anchor: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    status: Option<String>,
    owner: String,
    title: String,
    detail: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReplyRequest {
    base_branch: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    status: Option<String>,
    owner: String,
    title: String,
    detail: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActionRequest {
    base_branch: String,
    action: String,
    #[serde(default)]
    owner: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ThreadResponse {
    record_id: String,
    #[serde(flatten)]
    sync: SyncResponse,
}

#[derive(Debug, Serialize)]
pub(crate) struct ThreadListResponse {
    project_key: String,
    threads: Vec<ReviewThreadView>,
}

#[derive(Debug, Serialize)]
struct ReviewThreadView {
    id: String,
    anchor: Option<String>,
    parent: Option<String>,
    kind: String,
    status: String,
    owner: Option<String>,
    source: String,
    title: String,
    detail: String,
}

pub(crate) async fn list(
    State(state): State<AppState>,
    Path(project_key): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ThreadListResponse>, ServerError> {
    authenticated_user(&state.database, &headers).await?;
    let (content, _) = current_review_file(&state, &project_key, None).await?;
    let threads = parse_review_threads(&content)
        .map_err(|error| ServerError::Internal(anyhow::anyhow!(error)))?
        .into_iter()
        .map(ReviewThreadView::from)
        .collect();
    Ok(Json(ThreadListResponse {
        project_key,
        threads,
    }))
}

pub(crate) async fn create(
    State(state): State<AppState>,
    Path(project_key): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateThreadRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    require_csrf(&headers)?;
    let (content, path) =
        current_review_file(&state, &project_key, Some(&request.base_branch)).await?;
    let mut threads = parse_review_threads(&content)
        .map_err(|error| ServerError::BadRequest(format!("review sidecar is invalid: {error}")))?;
    let parent = request.parent.filter(|value| !value.trim().is_empty());
    if let Some(parent_id) = parent.as_deref() {
        ensure_parent(&threads, parent_id)?;
    }
    let anchor = request
        .anchor
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::parse)
        .transpose()
        .map_err(|error: String| ServerError::BadRequest(error))?;
    let thread = new_thread(
        anchor,
        parent,
        request.kind.as_deref().unwrap_or("comment"),
        request.status.as_deref().unwrap_or("open"),
        &request.owner,
        &request.title,
        &request.detail,
    )?;
    let record_id = thread.id.clone();
    threads.push(thread);
    let change = sidecar_change(&path, &threads)?;
    state
        .database
        .record_audit_event(
            Some(&user.id),
            "review_thread_created",
            &project_key,
            &record_id,
            serde_json::json!({ "parent": threads.last().and_then(|thread| thread.parent.clone()) }),
        )
        .await?;
    let sync = sync::submit_changes(
        &state,
        &project_key,
        &request.base_branch,
        &user.id,
        vec![change],
    )
    .await?;
    Ok(Json(ThreadResponse { record_id, sync }))
}

pub(crate) async fn reply(
    State(state): State<AppState>,
    Path((project_key, thread_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(request): Json<ReplyRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    require_csrf(&headers)?;
    let (content, path) =
        current_review_file(&state, &project_key, Some(&request.base_branch)).await?;
    let mut threads = parse_review_threads(&content)
        .map_err(|error| ServerError::BadRequest(format!("review sidecar is invalid: {error}")))?;
    let parent = threads
        .iter()
        .find(|thread| thread.id == thread_id)
        .ok_or_else(|| {
            ServerError::BadRequest(format!("review thread {thread_id} does not exist"))
        })?
        .clone();
    let thread = new_thread(
        parent.anchor,
        Some(thread_id.clone()),
        request.kind.as_deref().unwrap_or("comment"),
        request.status.as_deref().unwrap_or("open"),
        &request.owner,
        &request.title,
        &request.detail,
    )?;
    let record_id = thread.id.clone();
    threads.push(thread);
    let change = sidecar_change(&path, &threads)?;
    state
        .database
        .record_audit_event(
            Some(&user.id),
            "review_thread_replied",
            &project_key,
            &record_id,
            serde_json::json!({ "parent": thread_id }),
        )
        .await?;
    let sync = sync::submit_changes(
        &state,
        &project_key,
        &request.base_branch,
        &user.id,
        vec![change],
    )
    .await?;
    Ok(Json(ThreadResponse { record_id, sync }))
}

pub(crate) async fn action(
    State(state): State<AppState>,
    Path((project_key, thread_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(request): Json<ActionRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    require_csrf(&headers)?;
    let (content, path) =
        current_review_file(&state, &project_key, Some(&request.base_branch)).await?;
    let mut threads = parse_review_threads(&content)
        .map_err(|error| ServerError::BadRequest(format!("review sidecar is invalid: {error}")))?;
    let thread = threads
        .iter_mut()
        .find(|thread| thread.id == thread_id)
        .ok_or_else(|| ServerError::NotFound)?;
    apply_action(thread, &request.action, request.owner.as_deref())?;
    let change = sidecar_change(&path, &threads)?;
    state
        .database
        .record_audit_event(
            Some(&user.id),
            "review_thread_actioned",
            &project_key,
            &thread_id,
            serde_json::json!({ "action": request.action, "owner": request.owner }),
        )
        .await?;
    let sync = sync::submit_changes(
        &state,
        &project_key,
        &request.base_branch,
        &user.id,
        vec![change],
    )
    .await?;
    Ok(Json(ThreadResponse {
        record_id: thread_id,
        sync,
    }))
}

async fn current_review_file(
    state: &AppState,
    project_key: &str,
    base_branch: Option<&str>,
) -> Result<(String, String), ServerError> {
    let repository = state
        .database
        .project_repository(project_key)
        .await?
        .ok_or(ServerError::NotFound)?;
    let base_branch = match base_branch {
        Some(branch)
            if repository
                .allowed_base_branches
                .iter()
                .any(|allowed| allowed == branch) =>
        {
            branch.to_owned()
        }
        Some(_) => {
            return Err(ServerError::BadRequest(
                "base_branch is not configured for this repository".to_owned(),
            ));
        }
        None => repository
            .allowed_base_branches
            .first()
            .cloned()
            .ok_or(ServerError::NotFound)?,
    };
    if !repository
        .sidecar_paths
        .iter()
        .any(|path| path == REVIEW_PATH)
    {
        return Err(ServerError::BadRequest(
            "review.susu is not configured for this repository".to_owned(),
        ));
    }
    let sync_state = state
        .database
        .sync_state(project_key, &base_branch)
        .await?
        .ok_or(ServerError::NotFound)?;
    if sync_state.rebase_required {
        return Err(ServerError::Conflict(
            "project memory is locked while this repository conflict is unresolved".to_owned(),
        ));
    }
    let branch = sync_state.active_branch.as_deref().unwrap_or(&base_branch);
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
    let file = github
        .read_file(
            &token.token,
            &repository.repository_owner,
            &repository.repository_name,
            REVIEW_PATH,
            branch,
        )
        .await
        .map_err(ServerError::Internal)?;
    let content = file
        .map(|file| {
            String::from_utf8(file.content)
                .map_err(|_| ServerError::BadRequest("review.susu is not valid UTF-8".to_owned()))
        })
        .transpose()?
        .unwrap_or_default();
    Ok((content, REVIEW_PATH.to_owned()))
}

fn new_thread(
    anchor: Option<ReviewAnchor>,
    parent: Option<String>,
    kind: &str,
    status: &str,
    owner: &str,
    title: &str,
    detail: &str,
) -> Result<ReviewThread, ServerError> {
    let kind = kind
        .parse::<ReviewCommentKind>()
        .map_err(ServerError::BadRequest)?;
    let status = status
        .parse::<ReviewStatus>()
        .map_err(ServerError::BadRequest)?;
    let owner = bounded_required(owner, "owner", 200)?;
    let title = bounded_required(title, "title", 240)?;
    let detail = bounded_required(detail, "detail", 4000)?;
    let seed = format!("{anchor:?}|{parent:?}|{kind}|{status}|{owner}|{title}|{detail}");
    let id = format!("r_{:x}", Sha256::digest(seed.as_bytes()));
    Ok(ReviewThread {
        id,
        target: ExpectationTarget::Project,
        subject: None,
        anchor,
        parent,
        kind,
        status,
        owner: Some(owner),
        source: "human:portal".to_owned(),
        title,
        detail,
    })
}

fn apply_action(
    thread: &mut ReviewThread,
    action: &str,
    owner: Option<&str>,
) -> Result<(), ServerError> {
    match action {
        "assign" => thread.owner = Some(bounded_required(owner.unwrap_or_default(), "owner", 200)?),
        "resolve" => thread.status = ReviewStatus::Resolved,
        "reopen" => thread.status = ReviewStatus::Open,
        "accept" => thread.status = ReviewStatus::Accepted,
        "reject" => thread.status = ReviewStatus::Rejected,
        _ => {
            return Err(ServerError::BadRequest(
                "action must be assign, resolve, reopen, accept, or reject".to_owned(),
            ));
        }
    }
    Ok(())
}

fn ensure_parent(threads: &[ReviewThread], parent: &str) -> Result<(), ServerError> {
    if threads.iter().any(|thread| thread.id == parent) {
        Ok(())
    } else {
        Err(ServerError::BadRequest(format!(
            "review thread {parent} does not exist"
        )))
    }
}

fn bounded_required(value: &str, field: &str, max: usize) -> Result<String, ServerError> {
    let value = value.trim();
    if value.is_empty() || value.len() > max {
        return Err(ServerError::BadRequest(format!(
            "{field} must contain between 1 and {max} characters"
        )));
    }
    Ok(value.to_owned())
}

fn sidecar_change(path: &str, threads: &[ReviewThread]) -> Result<MaterializedChange, ServerError> {
    Ok(MaterializedChange {
        path: path.to_owned(),
        content: write_review_threads(threads, false)
            .map_err(ServerError::Internal)?
            .into_bytes(),
    })
}

impl From<ReviewThread> for ReviewThreadView {
    fn from(thread: ReviewThread) -> Self {
        Self {
            id: thread.id,
            anchor: thread.anchor.map(|anchor| anchor.to_string()),
            parent: thread.parent,
            kind: thread.kind.to_string(),
            status: thread.status.to_string(),
            owner: thread.owner,
            source: thread.source,
            title: thread.title,
            detail: thread.detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_action, new_thread};
    use crate::model::{ReviewAnchor, ReviewStatus};

    #[test]
    fn semantic_thread_ids_are_stable_and_anchors_are_preserved() {
        let first = new_thread(
            Some(ReviewAnchor::Expectation("e_demo".to_owned())),
            None,
            "question",
            "open",
            "team-platform",
            "Clarify ownership",
            "Who owns this follow-up?",
        )
        .expect("thread");
        let second = new_thread(
            Some(ReviewAnchor::Expectation("e_demo".to_owned())),
            None,
            "question",
            "open",
            "team-platform",
            "Clarify ownership",
            "Who owns this follow-up?",
        )
        .expect("thread");
        assert_eq!(first.id, second.id);
        assert_eq!(
            first.anchor,
            Some(ReviewAnchor::Expectation("e_demo".to_owned()))
        );
    }

    #[test]
    fn thread_actions_change_only_the_requested_review_state() {
        let mut thread =
            new_thread(None, None, "comment", "open", "team", "Title", "Detail").expect("thread");
        apply_action(&mut thread, "assign", Some("another-team")).expect("assign");
        assert_eq!(thread.owner.as_deref(), Some("another-team"));
        assert_eq!(thread.status, ReviewStatus::Open);
        apply_action(&mut thread, "resolve", None).expect("resolve");
        assert_eq!(thread.status, ReviewStatus::Resolved);
        assert!(apply_action(&mut thread, "unknown", None).is_err());
    }
}
