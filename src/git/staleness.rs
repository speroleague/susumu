//! Commit attribution for dirty evidence findings.
//!
//! `src/analysis/basis.rs` emits SUS023 / SUS033 when a verification or decision
//! no longer matches the review basis it was recorded against. Those findings
//! only say *that* something changed. This module walks the Git history since the
//! record's stored `revision` and rewrites the finding detail to name the
//! commit(s) that touched the record's target file.
//!
//! The core is pure over `&[GitCommit]` so it is unit-testable without a repo
//! (mirroring `src/git/connect.rs`).

use std::collections::BTreeMap;
use std::path::Path;

use susumu::model::ProjectAnalysis;

use crate::git::history::git_commits_for;
use crate::git::types::GitCommit;
use crate::normalize_git_path;

/// Finding rule ids that carry a review basis and can be commit-attributed.
pub(crate) const DIRTY_RULES: [&str; 2] = ["SUS023", "SUS033"];

/// Most commits to walk back per recorded revision when attributing staleness.
pub(crate) const DEFAULT_HISTORY_LIMIT: usize = 500;

const SHORT_SHA: usize = 7;

/// Enriches every dirty finding in `analysis` with the commit(s) that changed its
/// target since the record's stored revision. Best-effort: a no-op off Git, and
/// findings whose record carries no `revision` keep their generic detail.
pub(crate) fn enrich_stale_findings(analysis: &mut ProjectAnalysis, repo: &Path, limit: usize) {
    if analysis.source_revision.is_none() {
        return;
    }
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
            apply_commit_attribution(analysis, &target, &commits);
        }
    }
}

/// A dirty finding that points at a known artifact file, ready for attribution.
#[derive(Debug, Clone)]
pub(crate) struct StaleTarget {
    /// Index into `analysis.findings`.
    pub(crate) finding_index: usize,
    /// The verification or decision id the finding is about.
    pub(crate) record_id: String,
    /// Normalized repo-relative path of the target file.
    pub(crate) path: String,
    /// The revision the record was recorded against, if it carries one.
    pub(crate) revision: Option<String>,
}

/// Collects the dirty findings (SUS023 / SUS033) whose target resolves to a file
/// in the artifact, pairing each with the recording revision of its record.
pub(crate) fn stale_targets(analysis: &ProjectAnalysis) -> Vec<StaleTarget> {
    analysis
        .findings
        .iter()
        .enumerate()
        .filter(|(_, finding)| DIRTY_RULES.contains(&finding.rule_id.as_str()))
        .filter_map(|(index, finding)| {
            let file_id = finding.file_id.as_deref()?;
            let record_id = finding.subject.clone()?;
            let path = analysis
                .files
                .iter()
                .find(|file| file.id == file_id)
                .map(|file| normalize_git_path(&file.path))?;
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

/// The commits in `commits` (newest-first, already scoped to the range after the
/// record's revision) that changed `path`.
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

/// Rewrites the detail of the target's finding to name the attributed commits.
/// A no-op when nothing in `commits` touched the target file.
pub(crate) fn apply_commit_attribution(
    analysis: &mut ProjectAnalysis,
    target: &StaleTarget,
    commits: &[GitCommit],
) {
    let attributed = commits_touching(&target.path, commits);
    if attributed.is_empty() {
        return;
    }
    let Some(finding) = analysis.findings.get_mut(target.finding_index) else {
        return;
    };
    let recorded = target.revision.as_deref().map(short_sha).map_or_else(
        || "an earlier revision".to_owned(),
        |sha| format!("`{sha}`"),
    );
    let commit_list = attributed
        .iter()
        .map(|commit| format!("{} \"{}\"", short_sha(&commit.hash), commit.subject))
        .collect::<Vec<_>>()
        .join(", ");
    finding.detail = format!(
        "{} was recorded against {recorded}. {} has changed in {} commit(s) since: {commit_list}. Re-run `susumu verify <expectation> --supersedes {}` or record a decision that accepts the change.",
        target.record_id,
        target.path,
        attributed.len(),
        target.record_id,
    );
}

fn short_sha(sha: &str) -> &str {
    sha.get(..SHORT_SHA).unwrap_or(sha)
}

#[cfg(test)]
mod tests;
