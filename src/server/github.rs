use anyhow::{Context, Result, bail};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::{
    AppState,
    auth::{authenticated_user, require_csrf},
    config::GithubAppConfig,
    db::{Database, GithubAppConnectionRecord},
    error::ServerError,
    search::documents_for_sidecar,
};

const API_VERSION: &str = "2022-11-28";
const USER_AGENT_VALUE: &str = "susumu-api";

#[derive(Debug)]
pub(crate) struct InstallationToken {
    pub(crate) token: String,
    pub(crate) expires_at: String,
}

#[derive(Debug, Serialize)]
struct AppClaims<'a> {
    iat: i64,
    exp: i64,
    iss: &'a str,
}

#[derive(Debug, Deserialize)]
struct InstallationTokenResponse {
    token: String,
    expires_at: String,
}

#[derive(Debug, Deserialize)]
struct InstallationResponse {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct GithubAccount {
    login: String,
}

#[derive(Debug, Deserialize)]
struct InstallationRepositoriesResponse {
    repositories: Vec<RepositoryResponse>,
}

#[derive(Debug, Deserialize)]
struct RepositoryResponse {
    name: String,
    full_name: String,
    private: bool,
    default_branch: Option<String>,
    owner: GithubAccount,
}

#[derive(Debug, Deserialize)]
struct BranchResponse {
    name: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DiscoveredRepository {
    pub(crate) github_connection_id: String,
    pub(crate) installation_id: i64,
    pub(crate) repository_owner: String,
    pub(crate) repository_name: String,
    pub(crate) full_name: String,
    pub(crate) private: bool,
    pub(crate) default_branch: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct GithubClients {
    pub(crate) default: Option<GithubAppClient>,
    pub(crate) connections: HashMap<String, GithubAppClient>,
}

impl GithubClients {
    pub(crate) fn client(&self, connection_id: Option<&str>) -> Option<GithubAppClient> {
        connection_id
            .and_then(|id| self.connections.get(id))
            .cloned()
            .or_else(|| self.default.clone())
    }

    pub(crate) fn from_records(
        default: Option<GithubAppClient>,
        records: Vec<GithubAppConnectionRecord>,
        api_url: &str,
    ) -> Self {
        Self {
            default,
            connections: records
                .into_iter()
                .filter_map(|record| {
                    GithubAppClient::from_config(Some(&record.config), api_url)
                        .map(|client| (record.id, client))
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RefResponse {
    object: RefObject,
}

#[derive(Debug, Deserialize)]
struct RefObject {
    sha: String,
}

#[derive(Debug, Deserialize)]
struct ContentResponse {
    sha: String,
    content: Option<String>,
    encoding: Option<String>,
}

#[derive(Debug)]
pub(crate) struct RepositoryFile {
    pub(crate) sha: String,
    pub(crate) content: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct CreateRefRequest {
    #[serde(rename = "ref")]
    reference: String,
    sha: String,
}

#[derive(Debug, Serialize)]
struct UpsertFileRequest<'a> {
    message: &'a str,
    content: String,
    branch: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct PullRequestRequest<'a> {
    title: &'a str,
    head: &'a str,
    base: &'a str,
    body: &'a str,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PullRequestResult {
    pub(crate) number: i64,
    pub(crate) html_url: String,
}

/// Server-side GitHub App credentials and provider boundary.
///
/// This type intentionally does not expose key material to handlers or API responses. Network
/// authentication and repository operations will be added behind this boundary.
#[derive(Clone)]
pub(crate) struct GithubAppClient {
    app_id: u64,
    private_key_pem: String,
    api_url: String,
    http: reqwest::Client,
}

pub(crate) async fn validate_installation(
    State(state): State<AppState>,
    Path(project_key): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ServerError> {
    authenticated_user(&state.database, &headers).await?;
    require_csrf(&headers)?;
    let repository = state
        .database
        .project_repository(&project_key)
        .await?
        .ok_or(ServerError::NotFound)?;
    let installation_id = repository.installation_id;
    let client = state
        .github
        .read()
        .await
        .client(repository.github_connection_id.as_deref())
        .ok_or_else(|| {
            ServerError::ServiceUnavailable("GitHub App is not configured".to_owned())
        })?;
    let InstallationToken { token, expires_at } = client
        .installation_token(installation_id)
        .await
        .map_err(ServerError::Internal)?;
    drop(token);
    Ok(Json(json!({
        "status": "reachable",
        "project_key": project_key,
        "installation_id": installation_id,
        "expires_at": expires_at,
    })))
}

pub(crate) async fn repositories(
    State(state): State<AppState>,
    Query(query): Query<ConnectionQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<DiscoveredRepository>>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    if !user.roles.iter().any(|role| role == "admin") {
        return Err(ServerError::Forbidden);
    }
    let client = state
        .github
        .read()
        .await
        .client(query.connection_id.as_deref())
        .ok_or_else(|| {
            ServerError::ServiceUnavailable("GitHub App is not configured".to_owned())
        })?;
    Ok(Json(
        client
            .discover_repositories(query.connection_id.as_deref().unwrap_or_default())
            .await
            .map_err(ServerError::Internal)?,
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct BranchQuery {
    pub(crate) connection_id: String,
    pub(crate) owner: String,
    pub(crate) repository: String,
}

pub(crate) async fn branches(
    State(state): State<AppState>,
    Query(query): Query<BranchQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<String>>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    if !user.roles.iter().any(|role| role == "admin") {
        return Err(ServerError::Forbidden);
    }
    let client = state
        .github
        .read()
        .await
        .client(Some(&query.connection_id))
        .ok_or_else(|| {
            ServerError::ServiceUnavailable("GitHub App connection is not configured".to_owned())
        })?;
    Ok(Json(
        client
            .list_branches(&query.owner, &query.repository)
            .await
            .map_err(ServerError::Internal)?,
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ConnectionQuery {
    pub(crate) connection_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConnectionResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) app_id: u64,
}

pub(crate) async fn connections(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ConnectionResponse>>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    if !user.roles.iter().any(|role| role == "admin") {
        return Err(ServerError::Forbidden);
    }
    let connections = state.database.github_app_connections(&state.config).await?;
    if connections.is_empty()
        && let Some(config) = &state.config.github_app
    {
        return Ok(Json(vec![ConnectionResponse {
            id: String::new(),
            name: "Environment GitHub App".to_owned(),
            app_id: config.app_id,
        }]));
    }
    Ok(Json(
        connections
            .into_iter()
            .map(|connection| ConnectionResponse {
                id: connection.id,
                name: connection.name,
                app_id: connection.config.app_id,
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetupRequest {
    #[serde(default)]
    name: Option<String>,
    app_id: u64,
    private_key_pem: String,
}

pub(crate) async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SetupRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    require_csrf(&headers)?;
    if !user.roles.iter().any(|role| role == "admin") {
        return Err(ServerError::Forbidden);
    }
    if request.app_id == 0 {
        return Err(ServerError::BadRequest(
            "app_id must be a positive integer".to_owned(),
        ));
    }
    let private_key_pem = request.private_key_pem.trim();
    if private_key_pem.len() > 64 * 1024 {
        return Err(ServerError::BadRequest(
            "private_key_pem must be at most 64 KiB".to_owned(),
        ));
    }
    if !private_key_pem.contains("-----BEGIN") || !private_key_pem.contains("PRIVATE KEY-----") {
        return Err(ServerError::BadRequest(
            "private_key_pem must contain a PEM private key".to_owned(),
        ));
    }
    if state.config.credential_key.is_none() {
        return Err(ServerError::ServiceUnavailable(
            "credential encryption is not configured for this deployment".to_owned(),
        ));
    }
    let name = request.name.as_deref().unwrap_or("GitHub App").trim();
    if name.is_empty() || name.len() > 120 {
        return Err(ServerError::BadRequest(
            "name must contain between 1 and 120 characters".to_owned(),
        ));
    }
    let connection_id = state
        .database
        .save_github_app_connection(
            &state.config,
            name,
            request.app_id,
            private_key_pem,
            &user.id,
        )
        .await
        .map_err(ServerError::Internal)?;
    let client = GithubAppClient::from_config(
        Some(&GithubAppConfig {
            app_id: request.app_id,
            private_key_pem: private_key_pem.to_owned(),
        }),
        &state.config.github_api_url,
    )
    .expect("validated GitHub App config");
    state
        .github
        .write()
        .await
        .connections
        .insert(connection_id.clone(), client);
    Ok(Json(
        json!({ "status": "configured", "connection_id": connection_id }),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct InspectRequest {
    pub(crate) base_branch: String,
}

pub(crate) async fn inspect_branch(
    State(state): State<AppState>,
    Path(project_key): Path<String>,
    headers: HeaderMap,
    Json(request): Json<InspectRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let user = authenticated_user(&state.database, &headers).await?;
    require_csrf(&headers)?;
    let _ = user;
    let repository = state
        .database
        .project_repository(&project_key)
        .await?
        .ok_or(ServerError::NotFound)?;
    let base_branch = request.base_branch.trim();
    if !repository
        .allowed_base_branches
        .iter()
        .any(|branch| branch == base_branch)
    {
        return Err(ServerError::BadRequest(
            "base_branch is not configured for this repository".to_owned(),
        ));
    }
    let client = state
        .github
        .read()
        .await
        .client(repository.github_connection_id.as_deref())
        .ok_or_else(|| {
            ServerError::ServiceUnavailable("GitHub App is not configured".to_owned())
        })?;
    let InstallationToken { token, .. } = client
        .installation_token(repository.installation_id)
        .await
        .map_err(|error| {
            ServerError::ServiceUnavailable(format!("GitHub repository scan failed: {error:#}"))
        })?;
    let sha = client
        .branch_sha(
            &token,
            &repository.repository_owner,
            &repository.repository_name,
            base_branch,
        )
        .await
        .map_err(|error| {
            ServerError::ServiceUnavailable(format!("GitHub repository scan failed: {error:#}"))
        })?;
    let mut files = Vec::with_capacity(repository.sidecar_paths.len());
    let mut search_records = Vec::new();
    for path in &repository.sidecar_paths {
        let file = client
            .read_file(
                &token,
                &repository.repository_owner,
                &repository.repository_name,
                path,
                base_branch,
            )
            .await
            .map_err(|error| {
                ServerError::ServiceUnavailable(format!("GitHub repository scan failed: {error:#}"))
            })?;
        files.push(json!({
            "path": path,
            "present": file.is_some(),
            "sha": file.as_ref().map(|file| file.sha.as_str()),
            "content": file.and_then(|file| String::from_utf8(file.content).ok()),
        }));
        if let Some(content) = files.last().and_then(|file| file["content"].as_str())
            && let Ok(records) = documents_for_sidecar(path, content)
        {
            search_records.extend(records);
        }
    }
    state
        .database
        .replace_search_records(&project_key, base_branch, &sha, &search_records)
        .await?;
    drop(token);
    Ok(Json(json!({
        "project_key": project_key,
        "repository_owner": repository.repository_owner,
        "repository_name": repository.repository_name,
        "base_branch": base_branch,
        "head_sha": sha,
        "files": files,
    })))
}

/// Refreshes searchable record summaries for one configured repository branch.
/// Raw sidecar content never enters the search table.
pub(crate) async fn refresh_search_index(
    database: &Database,
    client: &GithubAppClient,
    project_key: &str,
    base_branch: &str,
) -> Result<Option<String>> {
    let Some(repository) = database.project_repository(project_key).await? else {
        return Ok(None);
    };
    if !repository
        .allowed_base_branches
        .iter()
        .any(|branch| branch == base_branch)
    {
        return Ok(None);
    }
    let InstallationToken { token, .. } = client
        .installation_token(repository.installation_id)
        .await?;
    let sha = client
        .branch_sha(
            &token,
            &repository.repository_owner,
            &repository.repository_name,
            base_branch,
        )
        .await?;
    let mut records = Vec::new();
    for path in &repository.sidecar_paths {
        if let Some(file) = client
            .read_file(
                &token,
                &repository.repository_owner,
                &repository.repository_name,
                path,
                base_branch,
            )
            .await?
            && let Ok(content) = String::from_utf8(file.content)
            && let Ok(parsed) = documents_for_sidecar(path, &content)
        {
            records.extend(parsed);
        }
    }
    database
        .replace_search_records(project_key, base_branch, &sha, &records)
        .await?;
    Ok(Some(sha))
}

impl GithubAppClient {
    pub(crate) fn from_config(config: Option<&GithubAppConfig>, api_url: &str) -> Option<Self> {
        config.and_then(|config| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .build()
                .ok()
                .map(|http| Self {
                    app_id: config.app_id,
                    private_key_pem: config.private_key_pem.clone(),
                    api_url: api_url.trim_end_matches('/').to_owned(),
                    http,
                })
        })
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.app_id > 0 && !self.private_key_pem.is_empty()
    }

    pub(crate) async fn discover_repositories(
        &self,
        connection_id: &str,
    ) -> Result<Vec<DiscoveredRepository>> {
        let jwt = self.app_jwt()?;
        let installations_url = format!("{}/app/installations?per_page=100", self.api_url);
        let response = self
            .http
            .get(installations_url)
            .headers(Self::jwt_headers(&jwt)?)
            .send()
            .await
            .context("could not request GitHub App installations")?;
        let status = response.status();
        if !status.is_success() {
            bail!("GitHub App installations request failed with HTTP {status}");
        }
        let installations = response
            .json::<Vec<InstallationResponse>>()
            .await
            .context("could not parse GitHub App installations response")?;
        let mut repositories = Vec::new();
        for installation in installations {
            let InstallationToken { token, .. } = self.installation_token(installation.id).await?;
            let url = format!("{}/installation/repositories?per_page=100", self.api_url);
            let response = self
                .http
                .get(url)
                .headers(Self::api_headers(&token)?)
                .send()
                .await
                .context("could not request GitHub installation repositories")?;
            let status = response.status();
            if !status.is_success() {
                bail!("GitHub installation repositories request failed with HTTP {status}");
            }
            let page = response
                .json::<InstallationRepositoriesResponse>()
                .await
                .context("could not parse GitHub installation repositories response")?;
            repositories.extend(page.repositories.into_iter().filter_map(|repository| {
                (!repository.name.trim().is_empty()
                    && !repository.full_name.trim().is_empty()
                    && !repository.owner.login.trim().is_empty())
                .then_some(DiscoveredRepository {
                    github_connection_id: connection_id.to_owned(),
                    installation_id: installation.id,
                    repository_owner: repository.owner.login,
                    repository_name: repository.name,
                    full_name: repository.full_name,
                    private: repository.private,
                    default_branch: repository.default_branch,
                })
            }));
        }
        repositories.sort_by(|left, right| left.full_name.cmp(&right.full_name));
        Ok(repositories)
    }

    pub(crate) async fn list_branches(&self, owner: &str, repository: &str) -> Result<Vec<String>> {
        let installations = self.discover_repositories("").await?;
        let installation_id = installations
            .into_iter()
            .find(|item| {
                item.repository_owner.eq_ignore_ascii_case(owner)
                    && item.repository_name.eq_ignore_ascii_case(repository)
            })
            .map(|item| item.installation_id)
            .context("repository is not available through this GitHub App connection")?;
        let token = self.installation_token(installation_id).await?;
        let url = format!(
            "{}/repos/{}/{}/branches?per_page=100",
            self.api_url,
            encode_path_segment(owner),
            encode_path_segment(repository)
        );
        let response = self
            .http
            .get(url)
            .headers(Self::api_headers(&token.token)?)
            .send()
            .await
            .context("could not request GitHub branches")?;
        if !response.status().is_success() {
            bail!(
                "GitHub branches request failed with HTTP {}",
                response.status()
            );
        }
        let mut branches = response
            .json::<Vec<BranchResponse>>()
            .await
            .context("could not parse GitHub branches response")?
            .into_iter()
            .map(|branch| branch.name)
            .collect::<Vec<_>>();
        branches.sort();
        Ok(branches)
    }

    pub(crate) async fn installation_token(
        &self,
        installation_id: i64,
    ) -> Result<InstallationToken> {
        if installation_id <= 0 {
            bail!("GitHub installation id must be positive");
        }
        let jwt = self.app_jwt()?;
        let url = format!(
            "{}/app/installations/{installation_id}/access_tokens",
            self.api_url
        );
        let response = self
            .http
            .post(url)
            .header(ACCEPT, "application/vnd.github+json")
            .header(AUTHORIZATION, format!("Bearer {jwt}"))
            .header("X-GitHub-Api-Version", API_VERSION)
            .header(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE))
            .send()
            .await
            .context("could not request GitHub installation token")?;
        let status = response.status();
        if !status.is_success() {
            bail!("GitHub installation token request failed with HTTP {status}");
        }
        let token = response
            .json::<InstallationTokenResponse>()
            .await
            .context("could not parse GitHub installation token response")?;
        if token.token.is_empty() || token.expires_at.is_empty() {
            bail!("GitHub installation token response was incomplete");
        }
        Ok(InstallationToken {
            token: token.token,
            expires_at: token.expires_at,
        })
    }

    pub(crate) async fn branch_sha(
        &self,
        installation_token: &str,
        owner: &str,
        repository: &str,
        branch: &str,
    ) -> Result<String> {
        let url = format!(
            "{}/repos/{}/{}/git/ref/heads/{}",
            self.api_url,
            encode_path_segment(owner),
            encode_path_segment(repository),
            encode_branch(branch),
        );
        let response = self
            .http
            .get(url)
            .header(ACCEPT, "application/vnd.github+json")
            .header(AUTHORIZATION, format!("Bearer {installation_token}"))
            .header("X-GitHub-Api-Version", API_VERSION)
            .header(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE))
            .send()
            .await
            .context("could not request GitHub branch ref")?;
        let status = response.status();
        if !status.is_success() {
            bail!("GitHub branch ref request failed with HTTP {status}");
        }
        let reference = response
            .json::<RefResponse>()
            .await
            .context("could not parse GitHub branch ref response")?;
        if reference.object.sha.is_empty() {
            bail!("GitHub branch ref response was incomplete");
        }
        Ok(reference.object.sha)
    }

    pub(crate) async fn file_sha(
        &self,
        installation_token: &str,
        owner: &str,
        repository: &str,
        path: &str,
        branch: &str,
    ) -> Result<Option<String>> {
        validate_repository_path(path)?;
        let url = format!(
            "{}/repos/{}/{}/contents/{}?ref={}",
            self.api_url,
            encode_path_segment(owner),
            encode_path_segment(repository),
            encode_repository_path(path),
            encode_branch(branch),
        );
        let response = self
            .http
            .get(url)
            .headers(Self::api_headers(installation_token)?)
            .send()
            .await
            .context("could not request GitHub repository file")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            bail!(
                "GitHub repository file request failed with HTTP {}",
                response.status()
            );
        }
        let file = response
            .json::<ContentResponse>()
            .await
            .context("could not parse GitHub repository file response")?;
        if file.sha.is_empty() {
            bail!("GitHub repository file response was incomplete");
        }
        Ok(Some(file.sha))
    }

    pub(crate) async fn read_file(
        &self,
        installation_token: &str,
        owner: &str,
        repository: &str,
        path: &str,
        branch: &str,
    ) -> Result<Option<RepositoryFile>> {
        validate_repository_path(path)?;
        let url = format!(
            "{}/repos/{}/{}/contents/{}?ref={}",
            self.api_url,
            encode_path_segment(owner),
            encode_path_segment(repository),
            encode_repository_path(path),
            encode_branch(branch),
        );
        let response = self
            .http
            .get(url)
            .headers(Self::api_headers(installation_token)?)
            .send()
            .await
            .context("could not read GitHub repository file")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            bail!(
                "GitHub repository file read failed with HTTP {}",
                response.status()
            );
        }
        let file = response
            .json::<ContentResponse>()
            .await
            .context("could not parse GitHub repository file content")?;
        let Some(content) = file.content else {
            bail!("GitHub repository file response did not include content");
        };
        if file.encoding.as_deref() != Some("base64") {
            bail!("GitHub repository file content used an unsupported encoding");
        }
        let content = STANDARD
            .decode(
                content
                    .bytes()
                    .filter(|byte| !byte.is_ascii_whitespace())
                    .collect::<Vec<_>>(),
            )
            .context("could not decode GitHub repository file content")?;
        Ok(Some(RepositoryFile {
            sha: file.sha,
            content,
        }))
    }

    #[allow(dead_code, clippy::too_many_arguments)]
    pub(crate) async fn create_branch(
        &self,
        installation_token: &str,
        owner: &str,
        repository: &str,
        branch: &str,
        from_sha: &str,
    ) -> Result<()> {
        if branch.trim().is_empty() || from_sha.trim().is_empty() {
            bail!("GitHub branch and source SHA are required");
        }
        let url = format!(
            "{}/repos/{}/{}/git/refs",
            self.api_url,
            encode_path_segment(owner),
            encode_path_segment(repository)
        );
        let response = self
            .http
            .post(url)
            .headers(Self::api_headers(installation_token)?)
            .json(&CreateRefRequest {
                reference: format!("refs/heads/{}", branch.trim()),
                sha: from_sha.to_owned(),
            })
            .send()
            .await
            .context("could not create GitHub branch")?;
        ensure_success(&response, "create GitHub branch")
    }

    #[allow(dead_code, clippy::too_many_arguments)]
    pub(crate) async fn upsert_file(
        &self,
        installation_token: &str,
        owner: &str,
        repository: &str,
        path: &str,
        branch: &str,
        message: &str,
        content: &[u8],
        existing_sha: Option<&str>,
    ) -> Result<()> {
        if path.trim().is_empty() || branch.trim().is_empty() || message.trim().is_empty() {
            bail!("GitHub file path, branch, and commit message are required");
        }
        validate_repository_path(path)?;
        let url = format!(
            "{}/repos/{}/{}/contents/{}",
            self.api_url,
            encode_path_segment(owner),
            encode_path_segment(repository),
            encode_repository_path(path),
        );
        let response = self
            .http
            .put(url)
            .headers(Self::api_headers(installation_token)?)
            .json(&UpsertFileRequest {
                message,
                content: STANDARD.encode(content),
                branch,
                sha: existing_sha,
            })
            .send()
            .await
            .context("could not write GitHub repository file")?;
        ensure_success(&response, "write GitHub repository file")
    }

    #[allow(dead_code, clippy::too_many_arguments)]
    pub(crate) async fn create_pull_request(
        &self,
        installation_token: &str,
        owner: &str,
        repository: &str,
        title: &str,
        head: &str,
        base: &str,
        body: &str,
    ) -> Result<PullRequestResult> {
        if title.trim().is_empty() || head.trim().is_empty() || base.trim().is_empty() {
            bail!("GitHub pull request title, head, and base are required");
        }
        let url = format!(
            "{}/repos/{}/{}/pulls",
            self.api_url,
            encode_path_segment(owner),
            encode_path_segment(repository)
        );
        let response = self
            .http
            .post(url)
            .headers(Self::api_headers(installation_token)?)
            .json(&PullRequestRequest {
                title,
                head,
                base,
                body,
            })
            .send()
            .await
            .context("could not create GitHub pull request")?;
        parse_pull_request(response, "create GitHub pull request").await
    }

    #[allow(dead_code)]
    pub(crate) async fn update_pull_request(
        &self,
        installation_token: &str,
        owner: &str,
        repository: &str,
        number: i64,
        title: &str,
        body: &str,
    ) -> Result<PullRequestResult> {
        if number <= 0 || title.trim().is_empty() {
            bail!("GitHub pull request number and title are required");
        }
        let url = format!(
            "{}/repos/{}/{}/pulls/{number}",
            self.api_url,
            encode_path_segment(owner),
            encode_path_segment(repository)
        );
        let response = self
            .http
            .patch(url)
            .headers(Self::api_headers(installation_token)?)
            .json(&serde_json::json!({ "title": title, "body": body }))
            .send()
            .await
            .context("could not update GitHub pull request")?;
        parse_pull_request(response, "update GitHub pull request").await
    }

    pub(crate) async fn update_pull_request_branch(
        &self,
        installation_token: &str,
        owner: &str,
        repository: &str,
        number: i64,
        expected_head_sha: &str,
    ) -> Result<()> {
        if number <= 0 || expected_head_sha.trim().is_empty() {
            bail!("GitHub pull request number and expected head SHA are required");
        }
        let url = format!(
            "{}/repos/{}/{}/pulls/{number}/update-branch",
            self.api_url,
            encode_path_segment(owner),
            encode_path_segment(repository)
        );
        let response = self
            .http
            .put(url)
            .headers(Self::api_headers(installation_token)?)
            .json(&serde_json::json!({ "expected_head_sha": expected_head_sha }))
            .send()
            .await
            .context("could not update GitHub pull request branch")?;
        ensure_success(&response, "update GitHub pull request branch")
    }

    fn api_headers(installation_token: &str) -> Result<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::try_from(format!("Bearer {installation_token}"))
                .context("invalid GitHub installation token header")?,
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static(API_VERSION),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
        Ok(headers)
    }

    fn jwt_headers(jwt: &str) -> Result<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::try_from(format!("Bearer {jwt}"))
                .context("invalid GitHub App JWT header")?,
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static(API_VERSION),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
        Ok(headers)
    }

    fn app_jwt(&self) -> Result<String> {
        let now = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("system clock is before Unix epoch")?
                .as_secs(),
        )
        .context("system clock value is too large")?;
        let claims = AppClaims {
            iat: now - 60,
            exp: now + 540,
            iss: &self.app_id.to_string(),
        };
        let key = EncodingKey::from_rsa_pem(self.private_key_pem.as_bytes())
            .context("could not parse GitHub App RSA private key")?;
        encode(&Header::new(Algorithm::RS256), &claims, &key)
            .context("could not sign GitHub App JWT")
    }
}

fn ensure_success(response: &reqwest::Response, operation: &str) -> Result<()> {
    let status = response.status();
    if !status.is_success() {
        bail!("{operation} failed with HTTP {status}");
    }
    Ok(())
}

async fn parse_pull_request(
    response: reqwest::Response,
    operation: &str,
) -> Result<PullRequestResult> {
    let status = response.status();
    if !status.is_success() {
        bail!("{operation} failed with HTTP {status}");
    }
    let pull_request = response
        .json::<PullRequestResult>()
        .await
        .context("could not parse GitHub pull request response")?;
    if pull_request.number <= 0 || pull_request.html_url.is_empty() {
        bail!("GitHub pull request response was incomplete");
    }
    Ok(pull_request)
}

fn encode_path_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn encode_branch(branch: &str) -> String {
    branch
        .split('/')
        .map(encode_path_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_repository_path(path: &str) -> String {
    path.split('/')
        .map(encode_path_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn validate_repository_path(path: &str) -> Result<()> {
    if path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        bail!("GitHub repository path is unsafe");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        Json, Router,
        extract::{Path, State},
        http::StatusCode,
        response::IntoResponse,
        routing::{get, put},
    };
    use serde_json::Value;
    use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};

    use super::{
        CreateRefRequest, GithubAppClient, encode_branch, encode_path_segment,
        encode_repository_path, validate_repository_path,
    };
    use crate::server::config::GithubAppConfig;

    #[derive(Debug)]
    struct FakeGithub {
        base_sha: String,
        head_sha: String,
        expected_head_sha: Option<String>,
        reject_update: bool,
    }

    async fn fake_ref(
        State(state): State<Arc<Mutex<FakeGithub>>>,
        Path((_owner, _repository, branch)): Path<(String, String, String)>,
    ) -> Json<Value> {
        let state = state.lock().await;
        let sha = if branch == "main" {
            state.base_sha.clone()
        } else {
            state.head_sha.clone()
        };
        Json(serde_json::json!({ "object": { "sha": sha } }))
    }

    async fn fake_update_branch(
        State(state): State<Arc<Mutex<FakeGithub>>>,
        Path((_owner, _repository, _number)): Path<(String, String, i64)>,
        Json(payload): Json<Value>,
    ) -> impl IntoResponse {
        let mut state = state.lock().await;
        state.expected_head_sha = payload["expected_head_sha"].as_str().map(str::to_owned);
        if state.reject_update {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "message": "This branch has conflicts" })),
            )
                .into_response();
        }
        state.head_sha = "head-after-update".to_owned();
        Json(serde_json::json!({ "message": "Branch was updated" })).into_response()
    }

    async fn start_fake_github(
        reject_update: bool,
    ) -> (GithubAppClient, Arc<Mutex<FakeGithub>>, JoinHandle<()>) {
        let state = Arc::new(Mutex::new(FakeGithub {
            base_sha: "base-before".to_owned(),
            head_sha: "head-before".to_owned(),
            expected_head_sha: None,
            reject_update,
        }));
        let app = Router::new()
            .route(
                "/repos/{owner}/{repository}/git/ref/heads/{branch}",
                get(fake_ref),
            )
            .route(
                "/repos/{owner}/{repository}/pulls/{number}/update-branch",
                put(fake_update_branch),
            )
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fake GitHub bind");
        let address = listener.local_addr().expect("fake GitHub address");
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("fake GitHub serve");
        });
        let config = GithubAppConfig {
            app_id: 42,
            private_key_pem: "test-key".to_owned(),
        };
        let client = GithubAppClient::from_config(Some(&config), &format!("http://{address}"))
            .expect("fake GitHub client");
        (client, state, task)
    }

    #[tokio::test]
    async fn local_github_update_branch_scenario_succeeds_with_expected_head() {
        let (client, state, task) = start_fake_github(false).await;
        assert_eq!(
            client
                .branch_sha("token", "acme", "memory", "main")
                .await
                .expect("base SHA"),
            "base-before"
        );
        client
            .update_pull_request_branch("token", "acme", "memory", 7, "head-before")
            .await
            .expect("update branch");
        assert_eq!(
            client
                .branch_sha("token", "acme", "memory", "susumu-memory")
                .await
                .expect("updated head SHA"),
            "head-after-update"
        );
        assert_eq!(
            state.lock().await.expected_head_sha.as_deref(),
            Some("head-before")
        );
        task.abort();
    }

    #[tokio::test]
    async fn local_github_update_branch_scenario_preserves_conflict() {
        let (client, state, task) = start_fake_github(true).await;
        let error = client
            .update_pull_request_branch("token", "acme", "memory", 7, "head-before")
            .await
            .expect_err("conflicting update");
        assert!(error.to_string().contains("HTTP 422"));
        assert_eq!(
            state.lock().await.expected_head_sha.as_deref(),
            Some("head-before")
        );
        task.abort();
    }

    #[test]
    fn client_keeps_credentials_inside_the_server_boundary() {
        let config = GithubAppConfig {
            app_id: 42,
            private_key_pem: "private-key".to_owned(),
        };
        let client =
            GithubAppClient::from_config(Some(&config), "https://api.github.com").expect("client");
        assert!(client.is_configured());
    }

    #[tokio::test]
    async fn invalid_key_fails_before_network_access() {
        let config = GithubAppConfig {
            app_id: 42,
            private_key_pem: "not a key".to_owned(),
        };
        let client =
            GithubAppClient::from_config(Some(&config), "http://127.0.0.1:1").expect("client");
        let error = client.installation_token(7).await.expect_err("invalid key");
        assert!(error.to_string().contains("private key"));
    }

    #[tokio::test]
    async fn installation_id_is_validated_before_signing() {
        let config = GithubAppConfig {
            app_id: 42,
            private_key_pem: "not a key".to_owned(),
        };
        let client =
            GithubAppClient::from_config(Some(&config), "http://127.0.0.1:1").expect("client");
        let error = client.installation_token(0).await.expect_err("invalid id");
        assert!(error.to_string().contains("must be positive"));
    }

    #[test]
    fn branch_paths_encode_each_segment_without_losing_slashes() {
        assert_eq!(encode_branch("release/2026 beta"), "release/2026%20beta");
        assert_eq!(encode_path_segment("owner name"), "owner%20name");
        assert_eq!(
            encode_repository_path(".susu/project file"),
            ".susu/project%20file"
        );
    }

    #[test]
    fn write_payloads_keep_git_refs_unencoded_for_json() {
        let payload = serde_json::to_value(CreateRefRequest {
            reference: "refs/heads/review/thread-1".to_owned(),
            sha: "abc123".to_owned(),
        })
        .expect("payload");
        assert_eq!(payload["ref"], "refs/heads/review/thread-1");
        assert_eq!(payload["sha"], "abc123");
    }

    #[test]
    fn repository_paths_reject_traversal_shapes() {
        assert!(validate_repository_path(".susu/project.susu").is_ok());
        assert!(validate_repository_path(".susu/../secrets").is_err());
        assert!(validate_repository_path("/absolute/path").is_err());
        assert!(validate_repository_path(".susu\\project.susu").is_err());
    }
}
