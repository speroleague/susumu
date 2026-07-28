use anyhow::{Context, Result, bail};
use rand::random;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{db::Database, github::GithubAppClient};

const PR_TITLE: &str = "Update Susumu project memory";

pub(crate) struct MaterializedChange {
    pub(crate) path: String,
    pub(crate) content: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SyncOutcome {
    NotQueued,
    Synchronized {
        active_branch: String,
        pull_request_number: i64,
    },
}

pub(crate) async fn synchronize(
    database: &Database,
    github: &GithubAppClient,
    project_key: &str,
    base_branch: &str,
    changes: &[MaterializedChange],
    actor_user_id: Option<&str>,
) -> Result<SyncOutcome> {
    if changes.is_empty() {
        bail!("at least one materialized change is required");
    }
    let Some(claim) = database.claim_sync(project_key, base_branch).await? else {
        return Ok(SyncOutcome::NotQueued);
    };
    let result = async {
        validate_changes(&claim, changes)?;
        database
            .record_audit_event(
                actor_user_id,
                "sync_started",
                &claim.project_key,
                &claim.base_branch,
                serde_json::json!({ "files": changes.len() }),
            )
            .await?;
        synchronize_claim(database, github, &claim, changes).await
    }
    .await;
    if let Err(error) = &result {
        if let Some(detail) = error.to_string().strip_prefix("sync conflict: ") {
            let observed = detail.split("; observed=").nth(1).unwrap_or_default();
            database
                .mark_sync_conflict(&claim.project_key, &claim.base_branch, observed, detail)
                .await
                .context("could not record synchronization conflict")?;
        } else {
            database
                .fail_sync(&claim.project_key, &claim.base_branch, &error.to_string())
                .await
                .context("could not record failed repository synchronization")?;
        }
        database
            .record_audit_event(
                actor_user_id,
                "sync_failed",
                &claim.project_key,
                &claim.base_branch,
                serde_json::json!({ "error": error.to_string() }),
            )
            .await?;
    }
    if let Ok((active_branch, pull_request_number)) = &result {
        database
            .record_audit_event(
                actor_user_id,
                "sync_completed",
                &claim.project_key,
                &claim.base_branch,
                serde_json::json!({
                    "active_branch": active_branch,
                    "pull_request_number": pull_request_number,
                }),
            )
            .await?;
    }
    result.map(
        |(active_branch, pull_request_number)| SyncOutcome::Synchronized {
            active_branch,
            pull_request_number,
        },
    )
}

async fn synchronize_claim(
    database: &Database,
    github: &GithubAppClient,
    claim: &super::db::SyncClaim,
    changes: &[MaterializedChange],
) -> Result<(String, i64)> {
    let token = github.installation_token(claim.installation_id).await?;
    let current_base_sha = ensure_base_is_current(github, &token.token, claim).await?;
    let (branch, pull_request_number, is_new_lifecycle) =
        match (claim.active_branch.as_deref(), claim.pull_request_number) {
            (Some(branch), Some(number)) => (branch.to_owned(), number, false),
            (None, None) => {
                let branch = new_branch_name(&claim.project_key, &claim.base_branch)?;
                github
                    .create_branch(
                        &token.token,
                        &claim.repository_owner,
                        &claim.repository_name,
                        &branch,
                        &current_base_sha,
                    )
                    .await?;
                (branch, 0, true)
            }
            _ => bail!("repository synchronization state has incomplete active PR data"),
        };

    for change in changes {
        let existing_sha = if is_new_lifecycle {
            None
        } else {
            github
                .file_sha(
                    &token.token,
                    &claim.repository_owner,
                    &claim.repository_name,
                    &change.path,
                    &branch,
                )
                .await?
        };
        github
            .upsert_file(
                &token.token,
                &claim.repository_owner,
                &claim.repository_name,
                &change.path,
                &branch,
                "Update Susumu project memory",
                &change.content,
                existing_sha.as_deref(),
            )
            .await?;
    }

    let pull_request = if is_new_lifecycle {
        github
            .create_pull_request(
                &token.token,
                &claim.repository_owner,
                &claim.repository_name,
                PR_TITLE,
                &branch,
                &claim.base_branch,
                "Materialized Susumu review changes are ready for human review.",
            )
            .await?
    } else {
        github
            .update_pull_request(
                &token.token,
                &claim.repository_owner,
                &claim.repository_name,
                pull_request_number,
                PR_TITLE,
                "Materialized Susumu review changes were updated and remain ready for human review.",
            )
            .await?
    };
    let head_sha = github
        .branch_sha(
            &token.token,
            &claim.repository_owner,
            &claim.repository_name,
            &branch,
        )
        .await?;
    database
        .complete_sync(
            &claim.project_key,
            &claim.base_branch,
            &branch,
            pull_request.number,
            &current_base_sha,
            &head_sha,
        )
        .await?;
    Ok((branch, pull_request.number))
}

async fn ensure_base_is_current(
    github: &GithubAppClient,
    installation_token: &str,
    claim: &super::db::SyncClaim,
) -> Result<String> {
    let current_base_sha = github
        .branch_sha(
            installation_token,
            &claim.repository_owner,
            &claim.repository_name,
            &claim.base_branch,
        )
        .await?;
    if let Some(previous_base_sha) = &claim.base_sha
        && previous_base_sha != &current_base_sha
    {
        bail!(
            "sync conflict: base branch advanced from {previous_base_sha} to {current_base_sha}; observed={current_base_sha}"
        );
    }
    Ok(current_base_sha)
}

fn new_branch_name(project_key: &str, base_branch: &str) -> Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_secs();
    let project = branch_slug(project_key);
    let base = branch_slug(base_branch);
    if project.is_empty() || base.is_empty() {
        bail!("project and base branch must produce a valid branch name");
    }
    Ok(format!(
        "susumu/{project}/{base}-{timestamp}-{:08x}",
        random::<u32>()
    ))
}

fn validate_changes(claim: &super::db::SyncClaim, changes: &[MaterializedChange]) -> Result<()> {
    for (index, change) in changes.iter().enumerate() {
        if !claim.sidecar_paths.iter().any(|path| path == &change.path) {
            bail!("change path is not an allowlisted Susumu sidecar");
        }
        if changes[..index]
            .iter()
            .any(|previous| previous.path == change.path)
        {
            bail!("materialized changes cannot contain duplicate paths");
        }
    }
    Ok(())
}

fn branch_slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{MaterializedChange, branch_slug, validate_changes};
    use crate::server::db::SyncClaim;

    #[test]
    fn branch_slug_keeps_names_safe_and_readable() {
        assert_eq!(branch_slug("release/2026 beta"), "release-2026-beta");
        assert_eq!(branch_slug("project_key"), "project_key");
    }

    #[test]
    fn materialization_accepts_only_configured_unique_sidecars() {
        let claim = SyncClaim {
            project_key: "demo".to_owned(),
            repository_owner: "owner".to_owned(),
            repository_name: "repo".to_owned(),
            installation_id: 7,
            sidecar_paths: vec!["work.susu".to_owned()],
            base_branch: "main".to_owned(),
            active_branch: None,
            pull_request_number: None,
            base_sha: None,
        };
        let valid = [MaterializedChange {
            path: "work.susu".to_owned(),
            content: b"work".to_vec(),
        }];
        assert!(validate_changes(&claim, &valid).is_ok());
        let invalid = [MaterializedChange {
            path: "README.md".to_owned(),
            content: b"no".to_vec(),
        }];
        assert!(validate_changes(&claim, &invalid).is_err());
    }
}
