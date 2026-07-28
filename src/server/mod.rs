mod auth;
mod config;
mod db;
mod error;
mod github;
mod repository;
mod search;
mod sync;
mod worker;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use tokio::{net::TcpListener, sync::RwLock};

use self::{
    auth::{login, logout, me},
    config::Config,
    db::Database,
    error::ServerError,
    github::{GithubAppClient, GithubClients, refresh_search_index},
    sync::refresh_sync_provenance,
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) config: Config,
    pub(crate) database: Database,
    pub(crate) github: Arc<RwLock<GithubClients>>,
}

/// Runs the configured Susumu API server.
///
/// # Errors
///
/// Returns an error when configuration, database connection, migrations, socket binding,
/// or server execution fails.
pub async fn run() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let database = Database::connect(&config.database_url).await?;
    database.migrate().await?;
    database.bootstrap_admin(&config).await?;

    let address: SocketAddr = config.bind.parse()?;
    let github_config = database.github_app_config(&config).await?;
    let default_github =
        GithubAppClient::from_config(github_config.as_ref(), &config.github_api_url);
    let github_connections = database.github_app_connections(&config).await?;
    let github = Arc::new(RwLock::new(GithubClients::from_records(
        default_github,
        github_connections,
        &config.github_api_url,
    )));
    let state = AppState {
        config,
        database,
        github,
    };
    tokio::spawn(search_index_refresh_loop(state.clone()));
    let router = Router::new()
        .route("/healthz", get(health))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/me", get(me))
        .route(
            "/api/projects",
            get(repository::list).post(repository::create),
        )
        .route(
            "/api/projects/{project_key}/sync",
            post(sync::queue).put(sync::rebase),
        )
        .route(
            "/api/projects/{project_key}/sync/conflict",
            get(sync::conflict).post(sync::resolve_conflict),
        )
        .route(
            "/api/projects/{project_key}/github/validate",
            post(github::validate_installation),
        )
        .route("/api/github/repositories", get(github::repositories))
        .route("/api/github/branches", get(github::branches))
        .route("/api/github/connections", get(github::connections))
        .route("/api/github/setup", post(github::setup))
        .route(
            "/api/projects/{project_key}/github/inspect",
            post(github::inspect_branch),
        )
        .route("/api/projects/{project_key}/search", get(search::search))
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024))
        .with_state(state);
    let listener = TcpListener::bind(address).await?;
    println!("Susumu API listening on http://{address}");
    axum::serve(listener, router).await?;
    Ok(())
}

async fn search_index_refresh_loop(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let Ok(projects) = state.database.list_projects().await else {
            continue;
        };
        for project in projects {
            let client = state
                .github
                .read()
                .await
                .client(project.github_connection_id.as_deref());
            let Some(client) = client else {
                continue;
            };
            for branch in project.allowed_base_branches {
                if let Err(error) =
                    refresh_search_index(&state.database, &client, &project.project_key, &branch)
                        .await
                {
                    eprintln!(
                        "search index refresh failed for {}/{}: {error:#}",
                        project.project_key, branch
                    );
                }
                if let Err(error) =
                    refresh_sync_provenance(&state.database, &client, &project.project_key, &branch)
                        .await
                {
                    eprintln!(
                        "synchronization provenance refresh failed for {}/{}: {error:#}",
                        project.project_key, branch
                    );
                }
            }
        }
    }
}

async fn health(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<axum::Json<serde_json::Value>, ServerError> {
    state.database.health().await?;
    Ok(axum::Json(serde_json::json!({
        "status": "ok",
        "database": "ok",
        "github_app": state.github.read().await.default.as_ref().is_some_and(GithubAppClient::is_configured)
            || !state.github.read().await.connections.is_empty(),
        "credential_encryption": state.config.credential_key.is_some(),
    })))
}
