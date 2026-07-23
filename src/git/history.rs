use std::{path::Path, process::Command as ProcessCommand};

use anyhow::{Context, Result, bail};

use crate::{cli::git::GitImportArgs, git::types::GitCommit};

pub(crate) const GIT_LOG_FORMAT: &str = "%H%x1f%an%x1f%ae%x1f%aI%x1f%s%x1f%b%x1e";

pub(crate) fn git_commits(args: &GitImportArgs) -> Result<Vec<GitCommit>> {
    git_commits_for(
        &args.repo,
        args.since.as_deref(),
        args.until.as_deref(),
        args.limit,
    )
}

pub(crate) fn git_commits_for(
    repo: &Path,
    since: Option<&str>,
    until: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<GitCommit>> {
    let mut command = ProcessCommand::new("git");
    command
        .arg("-C")
        .arg(repo)
        .arg("log")
        .arg(format!("--format={GIT_LOG_FORMAT}"));
    if let Some(limit) = limit {
        command.arg("-n").arg(limit.to_string());
    }
    if let Some(revision) = git_revision_range(since, until) {
        command.arg(revision);
    }

    let output = command
        .output()
        .with_context(|| format!("could not run git in {}", repo.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("does not have any commits") {
            return Ok(Vec::new());
        }
        bail!("git log failed in {}: {}", repo.display(), stderr.trim());
    }
    let stdout = String::from_utf8(output.stdout).context("git log output was not UTF-8")?;
    let mut commits = parse_git_commits(&stdout);
    for commit in &mut commits {
        commit.changed_files = git_changed_files(repo, &commit.hash)?;
    }
    Ok(commits)
}

pub(crate) fn git_commit_for_ref(repo: &Path, revision: &str) -> Result<GitCommit> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .arg("log")
        .arg("-n")
        .arg("1")
        .arg(format!("--format={GIT_LOG_FORMAT}"))
        .arg(revision)
        .output()
        .with_context(|| format!("could not read commit {revision} in {}", repo.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git log failed for commit {revision} in {}: {}",
            repo.display(),
            stderr.trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).context("git log output was not UTF-8")?;
    let mut commits = parse_git_commits(&stdout);
    let mut commit = commits
        .pop()
        .with_context(|| format!("could not find commit {revision} in {}", repo.display()))?;
    commit.changed_files = git_changed_files(repo, &commit.hash)?;
    Ok(commit)
}

pub(crate) fn git_changed_files(repo: &Path, commit_hash: &str) -> Result<Vec<String>> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .arg("diff-tree")
        .arg("--no-commit-id")
        .arg("--name-only")
        .arg("-r")
        .arg("--root")
        .arg(commit_hash)
        .output()
        .with_context(|| format!("could not read changed files for commit {commit_hash}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git diff-tree failed for commit {commit_hash}: {}",
            stderr.trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).context("git diff-tree output was not UTF-8")?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(normalize_path)
        .collect())
}

pub(crate) fn git_revision_range(since: Option<&str>, until: Option<&str>) -> Option<String> {
    match (since, until) {
        (Some(since), Some(until)) => Some(format!("{since}..{until}")),
        (Some(since), None) => Some(format!("{since}..HEAD")),
        (None, Some(until)) => Some(until.to_owned()),
        (None, None) => None,
    }
}

pub(crate) fn parse_git_commits(source: &str) -> Vec<GitCommit> {
    source
        .split('\x1e')
        .filter_map(|record| {
            let record = record.trim_matches('\n');
            if record.is_empty() {
                return None;
            }
            let mut fields = record.splitn(6, '\x1f');
            Some(GitCommit {
                hash: fields.next()?.to_owned(),
                author_name: fields.next()?.to_owned(),
                author_email: fields.next()?.to_owned(),
                author_date: fields.next()?.to_owned(),
                subject: fields.next()?.to_owned(),
                body: fields.next().unwrap_or_default().trim().to_owned(),
                changed_files: Vec::new(),
            })
        })
        .collect()
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}
