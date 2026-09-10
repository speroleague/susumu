//! Commit attribution for dirty evidence findings.
//!
//! `src/analysis/basis.rs` emits SUS023 / SUS033 when a verification or decision
//! no longer matches the review basis it was recorded against. Those findings
//! only say *that* something changed. This module walks the Git history since the
//! record's stored `revision` and rewrites the finding detail to name the
//! commit(s) that touched the record's target.
//!
//! The core is pure over `&[GitCommit]` so it is unit-testable without a repo
//! (mirroring `src/git/connect.rs`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use susumu::model::ProjectAnalysis;

use crate::git::history::git_commits_for;
use crate::git::types::GitCommit;
use crate::normalize_git_path;

/// Most commits to walk back per recorded revision when attributing staleness.
pub(crate) const DEFAULT_HISTORY_LIMIT: usize = 500;

/// Most commits to name in an enriched finding detail before summarizing the rest.
const MAX_ATTRIBUTED_COMMITS: usize = 5;

const SHORT_SHA: usize = 7;

/// Enriches every dirty finding in `analysis` with the commit(s) that changed its
/// target since the record's stored revision. Best-effort: a no-op off Git, and
/// findings whose record carries no `revision` keep their generic detail.
pub(crate) fn enrich_stale_findings(analysis: &mut ProjectAnalysis, repo: &Path, limit: usize) {
    if analysis.source_revision.is_none() {
        return;
    }
    let source_paths = project_source_paths(analysis);
    let mut by_revision: BTreeMap<String, Vec<StaleTarget>> = BTreeMap::new();
    for target in stale_targets(analysis) {
        if let Some(revision) = target.revision.clone() {
            by_revision.entry(revision).or_default().push(target);
        }
    }
    for (revision, targets) in by_revision {
        // `git_commits_for` turns `Some(revision)` into the range `revision..HEAD`.
        let Ok(commits) = git_commits_for(repo, Some(&revision), None, Some(limit)) else {
            continue;
        };
        for target in targets {
            apply_commit_attribution(analysis, &target, &commits, &source_paths);
        }
    }
}

/// A dirty finding ready for commit attribution.
#[derive(Debug, Clone)]
pub(crate) struct StaleTarget {
    /// Index into `analysis.findings`.
    pub(crate) finding_index: usize,
    /// The verification or decision id the finding is about.
    pub(crate) record_id: String,
    /// Normalized repo-relative path of the target file, or `None` for a
    /// project-wide target (which is attributed against any scanned source file).
    pub(crate) path: Option<String>,
    /// The revision the record was recorded against, if it carries one.
    pub(crate) revision: Option<String>,
}

/// Collects the dirty findings (SUS023 / SUS033), pairing each with the recording
/// revision of its record. Findings with a resolvable file target carry that
/// path; findings on a project target carry `None`.
pub(crate) fn stale_targets(analysis: &ProjectAnalysis) -> Vec<StaleTarget> {
    analysis
        .findings
        .iter()
        .enumerate()
        .filter(|(_, finding)| finding.is_dirty_evidence())
        .filter_map(|(index, finding)| {
            let record_id = finding.subject.clone()?;
            let path = finding.file_id.as_deref().and_then(|file_id| {
                analysis
                    .files
                    .iter()
                    .find(|file| file.id == file_id)
                    .map(|file| normalize_git_path(&file.path))
            });
            Some(StaleTarget {
                finding_index: index,
                revision: record_revision(analysis, &record_id),
                record_id,
                path,
            })
        })
        .collect()
}

fn record_revision(analysis: &ProjectAnalysis, record_id: &str) -> Option<String> {
    if let Some(verification) = analysis
        .verifications
        .iter()
        .find(|verification| verification.id == record_id)
    {
        return verification.revision.clone();
    }
    analysis
        .decisions
        .iter()
        .find(|decision| decision.id == record_id)
        .and_then(|decision| decision.revision.clone())
}

/// The normalized repo-relative paths of every scanned source file.
pub(crate) fn project_source_paths(analysis: &ProjectAnalysis) -> BTreeSet<String> {
    analysis
        .files
        .iter()
        .map(|file| normalize_git_path(&file.path))
        .collect()
}

/// The commits in `commits` (newest-first) that changed `path`.
pub(crate) fn commits_touching<'a>(path: &str, commits: &'a [GitCommit]) -> Vec<&'a GitCommit> {
    commits
        .iter()
        .filter(|commit| {
            commit
                .changed_files
                .iter()
                .any(|changed| normalize_git_path(changed) == path)
        })
        .collect()
}

/// The commits in `commits` that changed any scanned source file.
pub(crate) fn commits_touching_project<'a>(
    source_paths: &BTreeSet<String>,
    commits: &'a [GitCommit],
) -> Vec<&'a GitCommit> {
    commits
        .iter()
        .filter(|commit| {
            commit
                .changed_files
                .iter()
                .any(|changed| source_paths.contains(&normalize_git_path(changed)))
        })
        .collect()
}

/// Rewrites the detail of the target's finding to name the attributed commits.
/// A no-op when nothing in `commits` touched the target.
pub(crate) fn apply_commit_attribution(
    analysis: &mut ProjectAnalysis,
    target: &StaleTarget,
    commits: &[GitCommit],
    source_paths: &BTreeSet<String>,
) {
    let (attributed, subject) = match target.path.as_deref() {
        Some(path) => (commits_touching(path, commits), path.to_owned()),
        None => (
            commits_touching_project(source_paths, commits),
            "the project".to_owned(),
        ),
    };
    if attributed.is_empty() {
        return;
    }
    let Some(finding) = analysis.findings.get_mut(target.finding_index) else {
        return;
    };
    let recorded = target.revision.as_deref().map_or_else(
        || "an earlier revision".to_owned(),
        |sha| format!("`{}`", short_sha(sha)),
    );
    finding.detail = format!(
        "{} was recorded against {recorded}. {subject} has changed in {} commit(s) since: {}. \
         Re-run `susumu verify <expectation> --supersedes {}` or record a decision that accepts the change.",
        target.record_id,
        attributed.len(),
        commit_list(&attributed),
        target.record_id,
    );
}

fn commit_list(commits: &[&GitCommit]) -> String {
    let named = commits
        .iter()
        .take(MAX_ATTRIBUTED_COMMITS)
        .map(|commit| format!("{} \"{}\"", short_sha(&commit.hash), commit.subject))
        .collect::<Vec<_>>()
        .join(", ");
    let extra = commits.len().saturating_sub(MAX_ATTRIBUTED_COMMITS);
    if extra > 0 {
        format!("{named}, and {extra} more")
    } else {
        named
    }
}

fn short_sha(sha: &str) -> &str {
    sha.get(..SHORT_SHA).unwrap_or(sha)
}

#[cfg(test)]
mod tests;
