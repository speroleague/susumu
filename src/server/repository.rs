use std::path::Path;

use axum::{Json, extract::State, http::HeaderMap};
use serde::{Deserialize, Serialize};

use super::{
    AppState,
    auth::{authenticated_user, require_csrf},
    db::{NewProject, ProjectRecord, SyncStateRecord},
    error::ServerError,
};

#[derive(Debug, Deserialize)]
pub(crate) struct CreateProjectRequest {
    project_key: String,
    display_name: String,
    provider: String,
    repository_owner: String,
    repository_name: String,
    installation_id: i64,
    #[serde(default)]
    github_connection_id: Option<String>,
    allowed_base_branches: Vec<String>,
    #[serde(default = "default_sidecar_paths")]
    sidecar_paths: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectResponse {
    project_key: String,
    display_name: String,
    provider: String,
    repository_owner: String,
    repository_name: String,
    installation_id: i64,
    github_connection_id: Option<String>,
    allowed_base_branches: Vec<String>,
    sidecar_paths: Vec<String>,
    sync: Vec<SyncStateResponse>,
}

#[derive(Debug, Serialize)]
struct SyncStateResponse {
    base_branch: String,
    status: String,
    active_branch: Option<String>,
    pull_request_number: Option<i64>,
    last_error: Option<String>,
    base_sha: Option<String>,
    head_sha: Option<String>,
    observed_base_sha: Option<String>,
    rebase_required: bool,
    conflict_detail: Option<String>,
}

pub(crate) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ProjectResponse>>, ServerError> {
    authenticated_user(&state.database, &headers).await?;
    let projects = state.database.list_projects().await?;
    let mut responses = Vec::with_capacity(projects.len());
    for project in projects {
        let sync = state
            .database
            .list_sync_states(&project.project_key)
            .await?;
        responses.push(ProjectResponse::from_records(project, sync));
    }
    Ok(Json(responses))
}

pub(crate) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateProjectRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    require_csrf(&headers)?;
    if !user.roles.iter().any(|role| role == "admin") {
        return Err(ServerError::Forbidden);
    }
    let request = validate(request)?;
    if let Some(connection_id) = request.github_connection_id.as_deref()
        && !state
            .database
            .github_connection_exists(connection_id)
            .await?
    {
        return Err(ServerError::BadRequest(
            "github_connection_id does not identify an active connection".to_owned(),
        ));
    }
    let project_key = request.project_key.clone();
    state
        .database
        .create_project(&NewProject {
            project_key,
            display_name: request.display_name,
            provider: request.provider,
            repository_owner: request.repository_owner,
            repository_name: request.repository_name,
            installation_id: request.installation_id,
            github_connection_id: request.github_connection_id,
            allowed_base_branches: request.allowed_base_branches,
            sidecar_paths: request.sidecar_paths,
            created_by_user_id: user.id,
        })
        .await?;
    Ok(Json(
        serde_json::json!({ "status": "created", "project_key": request.project_key }),
    ))
}

fn default_sidecar_paths() -> Vec<String> {
    vec![
        "expectations.susu".to_owned(),
        "work.susu".to_owned(),
        "verifications.susu".to_owned(),
        "decisions.susu".to_owned(),
        "review.susu".to_owned(),
    ]
}

fn validate(mut request: CreateProjectRequest) -> Result<CreateProjectRequest, ServerError> {
    request.project_key = normalized_identifier(&request.project_key, "project_key")?;
    request.provider = request.provider.trim().to_lowercase();
    if request.provider != "github" {
        return Err(ServerError::BadRequest(
            "provider must be github".to_owned(),
        ));
    }
    request.repository_owner =
        normalized_identifier(&request.repository_owner, "repository_owner")?;
    request.repository_name = normalized_identifier(&request.repository_name, "repository_name")?;
    request.display_name = required_text(&request.display_name, "display_name")?;
    if request.installation_id <= 0 {
        return Err(ServerError::BadRequest(
            "installation_id must be positive".to_owned(),
        ));
    }
    if request.allowed_base_branches.is_empty() || request.allowed_base_branches.len() > 32 {
        return Err(ServerError::BadRequest(
            "allowed_base_branches must contain between 1 and 32 branches".to_owned(),
        ));
    }
    request.allowed_base_branches = request
        .allowed_base_branches
        .into_iter()
        .map(|branch| required_text(&branch, "base branch"))
        .collect::<Result<Vec<_>, _>>()?;
    request.sidecar_paths = request
        .sidecar_paths
        .into_iter()
        .map(|path| {
            let path = required_text(&path, "sidecar path")?;
            let candidate = Path::new(&path);
            if candidate.is_absolute()
                || candidate
                    .components()
                    .any(|component| component == std::path::Component::ParentDir)
            {
                return Err(ServerError::BadRequest(
                    "sidecar paths must remain inside the connected repository".to_owned(),
                ));
            }
            Ok(path)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if request.sidecar_paths.is_empty() {
        return Err(ServerError::BadRequest(
            "sidecar_paths cannot be empty".to_owned(),
        ));
    }
    Ok(request)
}

fn required_text(value: &str, field: &str) -> Result<String, ServerError> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.len() > 200 {
        return Err(ServerError::BadRequest(format!(
            "{field} must contain between 1 and 200 characters"
        )));
    }
    Ok(value)
}

fn normalized_identifier(value: &str, field: &str) -> Result<String, ServerError> {
    let value = value.trim().to_lowercase();
    if value.is_empty()
        || value.len() > 100
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
    {
        return Err(ServerError::BadRequest(format!(
            "{field} contains invalid characters"
        )));
    }
    Ok(value)
}

impl ProjectResponse {
    fn from_records(project: ProjectRecord, sync: Vec<SyncStateRecord>) -> Self {
        Self {
            project_key: project.project_key,
            display_name: project.display_name,
            provider: project.provider,
            repository_owner: project.repository_owner,
            repository_name: project.repository_name,
            installation_id: project.installation_id,
            github_connection_id: project.github_connection_id,
            allowed_base_branches: project.allowed_base_branches,
            sidecar_paths: project.sidecar_paths,
            sync: sync.into_iter().map(SyncStateResponse::from).collect(),
        }
    }
}

impl From<SyncStateRecord> for SyncStateResponse {
    fn from(state: SyncStateRecord) -> Self {
        Self {
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
    use super::{CreateProjectRequest, validate};

    fn valid_request() -> CreateProjectRequest {
        CreateProjectRequest {
            project_key: "Operations".to_owned(),
            display_name: "Operations memory".to_owned(),
            provider: "GitHub".to_owned(),
            repository_owner: "Acme".to_owned(),
            repository_name: "Portal".to_owned(),
            installation_id: 42,
            github_connection_id: None,
            allowed_base_branches: vec!["main".to_owned(), "release/1".to_owned()],
            sidecar_paths: vec!["expectations.susu".to_owned()],
        }
    }

    #[test]
    fn normalizes_allowlisted_repository_configuration() {
        let result = validate(valid_request()).expect("valid repository");
        assert_eq!(result.project_key, "operations");
        assert_eq!(result.provider, "github");
        assert_eq!(result.repository_owner, "acme");
    }

    #[test]
    fn rejects_paths_that_escape_the_repository() {
        let mut request = valid_request();
        request.sidecar_paths = vec!["../secrets.susu".to_owned()];
        assert!(validate(request).is_err());
    }

    #[test]
    fn rejects_non_github_providers_and_empty_branches() {
        let mut request = valid_request();
        request.provider = "gitlab".to_owned();
        assert!(validate(request).is_err());
        let mut request = valid_request();
        request.allowed_base_branches.clear();
        assert!(validate(request).is_err());
    }
}
