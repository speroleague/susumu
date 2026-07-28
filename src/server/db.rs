use anyhow::{Context, Result};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use sqlx::{PgPool, postgres::PgPoolOptions};

use super::config::{Config, GithubAppConfig};

#[derive(Clone)]
pub(crate) struct Database {
    pub(crate) pool: PgPool,
}

impl Database {
    pub(crate) async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .context("could not connect to PostgreSQL")?;
        Ok(Self { pool })
    }

    pub(crate) async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .context("could not run PostgreSQL migrations")?;
        Ok(())
    }

    pub(crate) async fn health(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    pub(crate) async fn github_app_config(
        &self,
        config: &Config,
    ) -> Result<Option<GithubAppConfig>> {
        if let Some(github_app) = &config.github_app {
            return Ok(Some(github_app.clone()));
        }
        let Some((app_id, ciphertext, nonce)) = sqlx::query_as::<_, (i64, Vec<u8>, Vec<u8>)>(
            "SELECT app_id, private_key_ciphertext, private_key_nonce FROM github_app_connections WHERE active = TRUE ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };
        let app_id = u64::try_from(app_id).context("stored GitHub App ID is invalid")?;
        let private_key_pem = config.decrypt_private_key(&ciphertext, &nonce)?;
        Ok(Some(GithubAppConfig {
            app_id,
            private_key_pem,
        }))
    }

    pub(crate) async fn github_app_connections(
        &self,
        config: &Config,
    ) -> Result<Vec<GithubAppConnectionRecord>> {
        let rows = sqlx::query_as::<_, (String, String, i64, Vec<u8>, Vec<u8>)>(
            "SELECT id, name, app_id, private_key_ciphertext, private_key_nonce FROM github_app_connections WHERE active = TRUE ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(id, name, app_id, ciphertext, nonce)| {
                Ok(GithubAppConnectionRecord {
                    id,
                    name,
                    config: GithubAppConfig {
                        app_id: u64::try_from(app_id).context("stored GitHub App ID is invalid")?,
                        private_key_pem: config.decrypt_private_key(&ciphertext, &nonce)?,
                    },
                })
            })
            .collect()
    }

    pub(crate) async fn save_github_app_connection(
        &self,
        config: &Config,
        name: &str,
        app_id: u64,
        private_key_pem: &str,
        actor_user_id: &str,
    ) -> Result<String> {
        let (ciphertext, nonce) = config.encrypt_private_key(private_key_pem)?;
        let id = sqlx::query_scalar::<_, String>(
            "INSERT INTO github_app_connections (name, app_id, private_key_ciphertext, private_key_nonce, created_by_user_id) VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(name)
        .bind(i64::try_from(app_id).context("GitHub App ID is too large")?)
        .bind(ciphertext)
        .bind(nonce)
        .bind(actor_user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    pub(crate) async fn github_connection_exists(&self, connection_id: &str) -> Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM github_app_connections WHERE id = $1 AND active = TRUE)",
        )
        .bind(connection_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub(crate) async fn bootstrap_admin(&self, config: &Config) -> Result<()> {
        let (Some(email), Some(password)) = (&config.admin_email, &config.admin_password) else {
            return Ok(());
        };
        let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        if exists.is_some() {
            return Ok(());
        }
        let password_hash = hash_password(password)?;
        sqlx::query(
            "INSERT INTO users (email, display_name, password_hash, roles) VALUES ($1, $2, $3, ARRAY['admin'])",
        )
        .bind(email)
        .bind(email)
        .bind(password_hash)
        .execute(&self.pool)
        .await
        .context("could not bootstrap administrator")?;
        Ok(())
    }

    pub(crate) async fn find_user(&self, email: &str) -> Result<Option<UserRecord>> {
        Ok(sqlx::query_as::<_, UserRecord>(
            "SELECT id, email, display_name, password_hash, roles FROM users WHERE email = $1 AND active = TRUE",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn create_session(&self, user_id: &str, token_hash: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, now() + interval '30 days')",
        )
        .bind(token_hash)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn delete_session(&self, token_hash: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn session_user(&self, token_hash: &str) -> Result<Option<SessionUser>> {
        Ok(sqlx::query_as::<_, SessionUser>(
            "SELECT u.id, u.email, u.display_name, u.roles FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token_hash = $1 AND s.expires_at > now() AND u.active = TRUE",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn list_projects(&self) -> Result<Vec<ProjectRecord>> {
        Ok(sqlx::query_as::<_, ProjectRecord>(
            "SELECT project_key, display_name, provider, repository_owner, repository_name, installation_id, github_connection_id, allowed_base_branches, sidecar_paths FROM projects WHERE active = TRUE ORDER BY display_name",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn list_sync_states(&self, project_key: &str) -> Result<Vec<SyncStateRecord>> {
        Ok(sqlx::query_as::<_, SyncStateRecord>(
            "SELECT base_branch, status, active_branch, pull_request_number, last_error, base_sha, head_sha, observed_base_sha, rebase_required, conflict_detail FROM project_sync_states WHERE project_key = $1 ORDER BY base_branch",
        )
        .bind(project_key)
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn sync_state(
        &self,
        project_key: &str,
        base_branch: &str,
    ) -> Result<Option<SyncStateRecord>> {
        Ok(sqlx::query_as::<_, SyncStateRecord>(
            "SELECT base_branch, status, active_branch, pull_request_number, last_error, base_sha, head_sha, observed_base_sha, rebase_required, conflict_detail FROM project_sync_states WHERE project_key = $1 AND base_branch = $2",
        )
        .bind(project_key)
        .bind(base_branch)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn project_repository(
        &self,
        project_key: &str,
    ) -> Result<Option<ProjectRepositoryRecord>> {
        Ok(sqlx::query_as::<_, ProjectRepositoryRecord>(
            "SELECT repository_owner, repository_name, installation_id, github_connection_id, allowed_base_branches, sidecar_paths FROM projects WHERE project_key = $1 AND active = TRUE",
        )
        .bind(project_key)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn create_project(&self, input: &NewProject) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO projects (project_key, display_name, provider, repository_owner, repository_name, installation_id, github_connection_id, allowed_base_branches, sidecar_paths, created_by_user_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(&input.project_key)
        .bind(&input.display_name)
        .bind(&input.provider)
        .bind(&input.repository_owner)
        .bind(&input.repository_name)
        .bind(input.installation_id)
        .bind(&input.github_connection_id)
        .bind(&input.allowed_base_branches)
        .bind(&input.sidecar_paths)
        .bind(&input.created_by_user_id)
        .execute(&mut *transaction)
        .await?;
        for branch in &input.allowed_base_branches {
            sqlx::query(
                "INSERT INTO project_sync_states (project_key, base_branch) VALUES ($1, $2)",
            )
            .bind(&input.project_key)
            .bind(branch)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn queue_sync(
        &self,
        project_key: &str,
        base_branch: &str,
        actor_user_id: &str,
        materialize: bool,
    ) -> Result<Option<SyncStateRecord>> {
        let mut transaction = self.pool.begin().await?;
        let configured: Option<(String,)> = sqlx::query_as(
            "SELECT project_key FROM projects WHERE project_key = $1 AND active = TRUE AND $2 = ANY(allowed_base_branches)",
        )
        .bind(project_key)
        .bind(base_branch)
        .fetch_optional(&mut *transaction)
        .await?;
        if configured.is_none() {
            return Ok(None);
        }
        let existing_conflict = sqlx::query_as::<_, SyncStateRecord>(
            "SELECT base_branch, status, active_branch, pull_request_number, last_error, base_sha, head_sha, observed_base_sha, rebase_required, conflict_detail FROM project_sync_states WHERE project_key = $1 AND base_branch = $2 AND rebase_required = TRUE",
        )
        .bind(project_key)
        .bind(base_branch)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(state) = existing_conflict {
            transaction.commit().await?;
            return Ok(Some(state));
        }
        let state = sqlx::query_as::<_, SyncStateRecord>(
            "INSERT INTO project_sync_states (project_key, base_branch, status) VALUES ($1, $2, 'queued') ON CONFLICT (project_key, base_branch) DO UPDATE SET status = CASE WHEN $3 = TRUE AND project_sync_states.status = 'pending' THEN 'queued' WHEN project_sync_states.status IN ('syncing', 'pending') THEN project_sync_states.status ELSE 'queued' END, active_branch = CASE WHEN project_sync_states.status = 'merged' THEN NULL ELSE project_sync_states.active_branch END, pull_request_number = CASE WHEN project_sync_states.status = 'merged' THEN NULL ELSE project_sync_states.pull_request_number END, last_error = NULL, conflict_detail = NULL, rebase_required = FALSE, updated_at = now() RETURNING base_branch, status, active_branch, pull_request_number, last_error, base_sha, head_sha, observed_base_sha, rebase_required, conflict_detail",
        )
        .bind(project_key)
        .bind(base_branch)
        .bind(materialize)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events (actor_user_id, action, project_key, target, detail) VALUES ($1, 'sync_requested', $2, $3, $4)",
        )
        .bind(actor_user_id)
        .bind(project_key)
        .bind(base_branch)
        .bind(sqlx::types::Json(serde_json::json!({ "status": state.status })))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(Some(state))
    }

    pub(crate) async fn prepare_conflict_resolution(
        &self,
        project_key: &str,
        base_branch: &str,
        base_sha: &str,
        actor_user_id: &str,
    ) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "UPDATE project_sync_states SET status = 'queued', base_sha = $3, observed_base_sha = $3, rebase_required = FALSE, conflict_detail = NULL, last_error = NULL, updated_at = now() WHERE project_key = $1 AND base_branch = $2 AND status = 'conflict'",
        )
        .bind(project_key)
        .bind(base_branch)
        .bind(base_sha)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events (actor_user_id, action, project_key, target, detail) VALUES ($1, 'sync_conflict_resolved', $2, $3, $4)",
        )
        .bind(actor_user_id)
        .bind(project_key)
        .bind(base_branch)
        .bind(sqlx::types::Json(serde_json::json!({ "base_sha": base_sha })))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn claim_sync(
        &self,
        project_key: &str,
        base_branch: &str,
    ) -> Result<Option<SyncClaim>> {
        let mut transaction = self.pool.begin().await?;
        let claim = sqlx::query_as::<_, SyncClaim>(
            "SELECT p.project_key, p.repository_owner, p.repository_name, p.installation_id, p.sidecar_paths, s.base_branch, s.active_branch, s.pull_request_number, s.base_sha FROM project_sync_states s JOIN projects p ON p.project_key = s.project_key WHERE p.project_key = $1 AND p.active = TRUE AND s.base_branch = $2 AND s.status = 'queued' AND NOT EXISTS (SELECT 1 FROM project_sync_states active WHERE active.project_key = s.project_key AND active.status IN ('syncing', 'pending')) FOR UPDATE SKIP LOCKED",
        )
        .bind(project_key)
        .bind(base_branch)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(claim) = claim else {
            transaction.commit().await?;
            return Ok(None);
        };
        sqlx::query(
            "UPDATE project_sync_states SET status = 'syncing', last_error = NULL, updated_at = now() WHERE project_key = $1 AND base_branch = $2",
        )
        .bind(project_key)
        .bind(base_branch)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(Some(claim))
    }

    pub(crate) async fn complete_sync(
        &self,
        project_key: &str,
        base_branch: &str,
        active_branch: &str,
        pull_request_number: i64,
        base_sha: &str,
        head_sha: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE project_sync_states SET status = 'pending', active_branch = $3, pull_request_number = $4, base_sha = $5, head_sha = $6, observed_base_sha = $5, rebase_required = FALSE, conflict_detail = NULL, last_error = NULL, last_successful_sync_at = now(), updated_at = now() WHERE project_key = $1 AND base_branch = $2 AND status = 'syncing'",
        )
        .bind(project_key)
        .bind(base_branch)
        .bind(active_branch)
        .bind(pull_request_number)
        .bind(base_sha)
        .bind(head_sha)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn fail_sync(
        &self,
        project_key: &str,
        base_branch: &str,
        error: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE project_sync_states SET status = 'error', last_error = $3, updated_at = now() WHERE project_key = $1 AND base_branch = $2 AND status = 'syncing'",
        )
        .bind(project_key)
        .bind(base_branch)
        .bind(error.chars().take(1000).collect::<String>())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn mark_sync_conflict(
        &self,
        project_key: &str,
        base_branch: &str,
        observed_base_sha: &str,
        detail: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE project_sync_states SET status = 'conflict', observed_base_sha = $3, rebase_required = TRUE, conflict_detail = $4, last_error = $4, updated_at = now() WHERE project_key = $1 AND base_branch = $2 AND status = 'syncing'",
        )
        .bind(project_key)
        .bind(base_branch)
        .bind(observed_base_sha)
        .bind(detail.chars().take(1000).collect::<String>())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn complete_rebase(
        &self,
        project_key: &str,
        base_branch: &str,
        base_sha: &str,
        head_sha: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE project_sync_states SET status = 'pending', base_sha = $3, head_sha = $4, observed_base_sha = $3, rebase_required = FALSE, conflict_detail = NULL, last_error = NULL, updated_at = now() WHERE project_key = $1 AND base_branch = $2 AND active_branch IS NOT NULL AND pull_request_number IS NOT NULL",
        )
        .bind(project_key)
        .bind(base_branch)
        .bind(base_sha)
        .bind(head_sha)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn mark_base_advanced(
        &self,
        project_key: &str,
        base_branch: &str,
        observed_base_sha: &str,
        detail: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE project_sync_states SET status = 'conflict', observed_base_sha = $3, rebase_required = TRUE, conflict_detail = $4, last_error = $4, updated_at = now() WHERE project_key = $1 AND base_branch = $2 AND status = 'pending' AND base_sha IS DISTINCT FROM $3",
        )
        .bind(project_key)
        .bind(base_branch)
        .bind(observed_base_sha)
        .bind(detail.chars().take(1000).collect::<String>())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn record_audit_event(
        &self,
        actor_user_id: Option<&str>,
        action: &str,
        project_key: &str,
        target: &str,
        detail: serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit_events (actor_user_id, action, project_key, target, detail) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(actor_user_id)
        .bind(action)
        .bind(project_key)
        .bind(target)
        .bind(sqlx::types::Json(detail))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn replace_search_records(
        &self,
        project_key: &str,
        base_branch: &str,
        head_sha: &str,
        records: &[SearchRecord],
    ) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1 || ':' || $2, 0))")
            .bind(project_key)
            .bind(base_branch)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "DELETE FROM project_search_records WHERE project_key = $1 AND base_branch = $2",
        )
        .bind(project_key)
        .bind(base_branch)
        .execute(&mut *transaction)
        .await?;
        for record in records {
            sqlx::query(
                "INSERT INTO project_search_records (project_key, base_branch, record_key, record_id, kind, comment_kind, status, title, detail, owner, anchor, parent, expectation_id, path, source_line, head_sha) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
            )
            .bind(project_key)
            .bind(base_branch)
            .bind(record.key())
            .bind(&record.record_id)
            .bind(&record.kind)
            .bind(&record.comment_kind)
            .bind(&record.status)
            .bind(&record.title)
            .bind(&record.detail)
            .bind(&record.owner)
            .bind(&record.anchor)
            .bind(&record.parent)
            .bind(&record.expectation_id)
            .bind(&record.path)
            .bind(record.source_line)
            .bind(head_sha)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn upsert_search_records(
        &self,
        project_key: &str,
        base_branch: &str,
        head_sha: &str,
        records: &[SearchRecord],
        paths: &[String],
    ) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1 || ':' || $2, 0))")
            .bind(project_key)
            .bind(base_branch)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "DELETE FROM project_search_records WHERE project_key = $1 AND base_branch = $2 AND path = ANY($3)",
        )
        .bind(project_key)
        .bind(base_branch)
        .bind(paths)
        .execute(&mut *transaction)
        .await?;
        for record in records {
            sqlx::query(
                "INSERT INTO project_search_records (project_key, base_branch, record_key, record_id, kind, comment_kind, status, title, detail, owner, anchor, parent, expectation_id, path, source_line, head_sha) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
            )
            .bind(project_key)
            .bind(base_branch)
            .bind(record.key())
            .bind(&record.record_id)
            .bind(&record.kind)
            .bind(&record.comment_kind)
            .bind(&record.status)
            .bind(&record.title)
            .bind(&record.detail)
            .bind(&record.owner)
            .bind(&record.anchor)
            .bind(&record.parent)
            .bind(&record.expectation_id)
            .bind(&record.path)
            .bind(record.source_line)
            .bind(head_sha)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn search_records(
        &self,
        request: &SearchRequest,
    ) -> Result<(Vec<SearchRecord>, i64, Option<String>)> {
        let rows = sqlx::query_as::<_, SearchRow>(
            "SELECT record_id, kind, comment_kind, status, title, detail, owner, anchor, parent, expectation_id, path, source_line, count(*) OVER() AS total_count FROM project_search_records WHERE project_key = $1 AND base_branch = $2 AND ($3 = '' OR search_vector @@ websearch_to_tsquery('simple', $3) OR title % $3 OR detail % $3 OR record_id % $3) AND ($4::text IS NULL OR kind = $4) AND ($5::text IS NULL OR status = $5) AND ($6::text IS NULL OR owner = $6) AND ($7::text IS NULL OR path = $7) ORDER BY CASE WHEN $3 = '' THEN 0.0 ELSE GREATEST(similarity(title, $3), similarity(detail, $3), similarity(record_id, $3)) END DESC, kind, record_id LIMIT $8 OFFSET $9",
        )
        .bind(&request.project_key)
        .bind(&request.base_branch)
        .bind(&request.query)
        .bind(&request.kind)
        .bind(&request.status)
        .bind(&request.owner)
        .bind(&request.path)
        .bind(request.limit)
        .bind(request.offset)
        .fetch_all(&self.pool)
        .await?;
        let total = rows.first().map_or(0, |row| row.total_count);
        let head_sha = sqlx::query_scalar::<_, String>(
            "SELECT head_sha FROM project_search_records WHERE project_key = $1 AND base_branch = $2 LIMIT 1",
        )
        .bind(&request.project_key)
        .bind(&request.base_branch)
        .fetch_optional(&self.pool)
        .await?;
        let records = rows.into_iter().map(SearchRecord::from).collect();
        Ok((records, total, head_sha))
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct UserRecord {
    pub(crate) id: String,
    pub(crate) email: String,
    pub(crate) display_name: String,
    pub(crate) password_hash: String,
    pub(crate) roles: Vec<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SessionUser {
    pub(crate) id: String,
    pub(crate) email: String,
    pub(crate) display_name: String,
    pub(crate) roles: Vec<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ProjectRecord {
    pub(crate) project_key: String,
    pub(crate) display_name: String,
    pub(crate) provider: String,
    pub(crate) repository_owner: String,
    pub(crate) repository_name: String,
    pub(crate) installation_id: i64,
    pub(crate) github_connection_id: Option<String>,
    pub(crate) allowed_base_branches: Vec<String>,
    pub(crate) sidecar_paths: Vec<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ProjectRepositoryRecord {
    pub(crate) repository_owner: String,
    pub(crate) repository_name: String,
    pub(crate) installation_id: i64,
    pub(crate) github_connection_id: Option<String>,
    pub(crate) allowed_base_branches: Vec<String>,
    pub(crate) sidecar_paths: Vec<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SyncStateRecord {
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

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SyncClaim {
    pub(crate) project_key: String,
    pub(crate) repository_owner: String,
    pub(crate) repository_name: String,
    pub(crate) installation_id: i64,
    pub(crate) sidecar_paths: Vec<String>,
    pub(crate) base_branch: String,
    pub(crate) active_branch: Option<String>,
    pub(crate) pull_request_number: Option<i64>,
    pub(crate) base_sha: Option<String>,
}

pub(crate) struct NewProject {
    pub(crate) project_key: String,
    pub(crate) display_name: String,
    pub(crate) provider: String,
    pub(crate) repository_owner: String,
    pub(crate) repository_name: String,
    pub(crate) installation_id: i64,
    pub(crate) github_connection_id: Option<String>,
    pub(crate) allowed_base_branches: Vec<String>,
    pub(crate) sidecar_paths: Vec<String>,
    pub(crate) created_by_user_id: String,
}

#[derive(Debug)]
pub(crate) struct GithubAppConnectionRecord {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) config: GithubAppConfig,
}

#[derive(Debug, Clone)]
pub(crate) struct SearchRecord {
    pub(crate) record_id: String,
    pub(crate) kind: String,
    pub(crate) comment_kind: Option<String>,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) owner: Option<String>,
    pub(crate) anchor: Option<String>,
    pub(crate) parent: Option<String>,
    pub(crate) expectation_id: Option<String>,
    pub(crate) path: String,
    pub(crate) source_line: Option<i32>,
}

impl SearchRecord {
    pub(crate) fn new(
        path: &str,
        record_id: String,
        kind: &str,
        status: String,
        title: String,
        detail: String,
    ) -> Self {
        Self {
            record_id,
            kind: kind.to_owned(),
            comment_kind: None,
            status,
            title,
            detail,
            owner: None,
            anchor: None,
            parent: None,
            expectation_id: None,
            path: path.to_owned(),
            source_line: None,
        }
    }

    pub(crate) fn with_expectation(mut self, expectation_id: Option<String>) -> Self {
        self.expectation_id = expectation_id;
        self
    }

    pub(crate) fn with_owner(mut self, owner: Option<String>) -> Self {
        self.owner = owner;
        self
    }

    pub(crate) fn with_anchor(mut self, anchor: Option<String>) -> Self {
        self.anchor = anchor;
        self
    }

    pub(crate) fn with_parent(mut self, parent: Option<String>) -> Self {
        self.parent = parent;
        self
    }

    pub(crate) fn with_kind(mut self, kind: crate::model::ReviewCommentKind) -> Self {
        self.comment_kind = Some(kind.to_string());
        self
    }

    fn key(&self) -> String {
        format!("{}::{}::{}", self.path, self.kind, self.record_id)
    }
}

#[derive(Debug)]
pub(crate) struct SearchRequest {
    pub(crate) project_key: String,
    pub(crate) base_branch: String,
    pub(crate) query: String,
    pub(crate) kind: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) owner: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct SearchRow {
    record_id: String,
    kind: String,
    comment_kind: Option<String>,
    status: String,
    title: String,
    detail: String,
    owner: Option<String>,
    anchor: Option<String>,
    parent: Option<String>,
    expectation_id: Option<String>,
    path: String,
    source_line: Option<i32>,
    total_count: i64,
}

impl From<SearchRow> for SearchRecord {
    fn from(row: SearchRow) -> Self {
        Self {
            record_id: row.record_id,
            kind: row.kind,
            comment_kind: row.comment_kind,
            status: row.status,
            title: row.title,
            detail: row.detail,
            owner: row.owner,
            anchor: row.anchor,
            parent: row.parent,
            expectation_id: row.expectation_id,
            path: row.path,
            source_line: row.source_line,
        }
    }
}

pub(crate) fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| anyhow::anyhow!(error))?
        .to_string())
}

pub(crate) fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(password_hash).map_err(|error| anyhow::anyhow!(error))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
