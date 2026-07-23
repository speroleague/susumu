use std::collections::BTreeSet;

use serde::Serialize;
use susumu::model::{ExpectationTarget, ProjectAnalysis};

use crate::{git::types::GitCommit, normalize_git_path};

#[derive(Debug)]
pub(crate) struct GitConnectReport {
    pub(crate) records: Vec<GitConnection>,
    pub(crate) connected: usize,
    pub(crate) needs_record: usize,
    pub(crate) unconnected: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct GitConnection {
    pub(crate) commit: String,
    pub(crate) short_commit: String,
    pub(crate) author: String,
    pub(crate) date: String,
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) reasons: Vec<String>,
    pub(crate) changed_files: Vec<String>,
    pub(crate) workflows: Vec<GitConnectedRecord>,
    pub(crate) expectations: Vec<GitConnectedRecord>,
    pub(crate) verifications: Vec<GitConnectedRecord>,
    pub(crate) decisions: Vec<GitConnectedRecord>,
    pub(crate) works: Vec<GitConnectedRecord>,
    pub(crate) suggestions: Vec<GitSuggestion>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GitConnectedRecord {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GitSuggestion {
    pub(crate) expectation_id: String,
    pub(crate) title: String,
    pub(crate) score: usize,
    pub(crate) command: String,
}

#[derive(Debug)]
pub(crate) struct SuggestedExpectation {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) score: usize,
}

#[derive(Debug)]
struct GitConnectionMatches {
    workflows: Vec<GitConnectedRecord>,
    expectations: Vec<GitConnectedRecord>,
    verifications: Vec<GitConnectedRecord>,
    decisions: Vec<GitConnectedRecord>,
    works: Vec<GitConnectedRecord>,
}

pub(crate) fn build_git_connect_report(
    artifact: &ProjectAnalysis,
    commits: &[GitCommit],
) -> GitConnectReport {
    let records = commits
        .iter()
        .map(|commit| git_connection(artifact, commit))
        .collect::<Vec<_>>();
    let connected = records
        .iter()
        .filter(|record| record.status == "connected")
        .count();
    let needs_record = records
        .iter()
        .filter(|record| record.status == "needs_record")
        .count();
    let unconnected = records
        .iter()
        .filter(|record| record.status == "unconnected")
        .count();
    GitConnectReport {
        records,
        connected,
        needs_record,
        unconnected,
    }
}

fn git_connection(artifact: &ProjectAnalysis, commit: &GitCommit) -> GitConnection {
    let matches = git_connection_matches(artifact, commit);
    let missing_expectation_work =
        missing_expectation_work_records(artifact, &commit.hash, &matches.expectations);
    let status = git_connection_status(&matches, &missing_expectation_work);
    let reasons = git_connection_reasons(&matches, &missing_expectation_work, &status);
    let suggestions = if status == "unconnected" {
        git_link_suggestions(artifact, commit)
    } else {
        Vec::new()
    };

    GitConnection {
        commit: commit.hash.clone(),
        short_commit: commit.hash.chars().take(8).collect(),
        author: format!("{} <{}>", commit.author_name, commit.author_email),
        date: commit.author_date.clone(),
        title: commit.subject.clone(),
        status,
        reasons,
        changed_files: commit.changed_files.clone(),
        workflows: matches.workflows,
        expectations: matches.expectations,
        verifications: matches.verifications,
        decisions: matches.decisions,
        works: matches.works,
        suggestions,
    }
}

fn git_connection_matches(artifact: &ProjectAnalysis, commit: &GitCommit) -> GitConnectionMatches {
    let text = format!("{}\n{}", commit.subject, commit.body);
    let matched_file_ids = matched_artifact_file_ids(artifact, &commit.changed_files);
    let workflows = connected_workflows(artifact, &text, &matched_file_ids);
    let workflow_ids = record_ids(&workflows);
    let expectations = connected_expectations(artifact, &text, &matched_file_ids, &workflow_ids);
    let expectation_ids = record_ids(&expectations);
    let verifications = connected_verifications(artifact, &text, &expectation_ids);
    let decisions = connected_decisions(artifact, &text, &matched_file_ids, &workflow_ids);
    let works = connected_works(artifact, &text, &commit.hash);

    GitConnectionMatches {
        workflows,
        expectations,
        verifications,
        decisions,
        works,
    }
}

fn git_connection_status(
    matches: &GitConnectionMatches,
    missing_expectation_work: &[GitConnectedRecord],
) -> String {
    let has_record = if matches.expectations.is_empty() {
        !matches.works.is_empty()
    } else {
        missing_expectation_work.is_empty()
    };
    let has_context = !matches.workflows.is_empty()
        || !matches.expectations.is_empty()
        || !matches.verifications.is_empty()
        || !matches.decisions.is_empty();
    if has_record {
        "connected"
    } else if has_context {
        "needs_record"
    } else {
        "unconnected"
    }
    .to_owned()
}

fn git_connection_reasons(
    matches: &GitConnectionMatches,
    missing_expectation_work: &[GitConnectedRecord],
    status: &str,
) -> Vec<String> {
    let mut reasons = git_connection_match_reasons(matches);
    if reasons.is_empty() {
        reasons.push("no Susumu records or workflow files matched".to_owned());
    } else if !missing_expectation_work.is_empty() {
        reasons.push(format!(
            "{} expectation work record(s) missing",
            missing_expectation_work.len()
        ));
    } else if status != "connected" {
        reasons.push("no work record references this commit".to_owned());
    }
    reasons
}

fn git_connection_match_reasons(matches: &GitConnectionMatches) -> Vec<String> {
    let mut reasons = Vec::new();
    push_count_reason(&mut reasons, matches.workflows.len(), "workflow link(s)");
    push_count_reason(
        &mut reasons,
        matches.expectations.len(),
        "expectation link(s)",
    );
    push_count_reason(
        &mut reasons,
        matches.verifications.len(),
        "verification link(s)",
    );
    push_count_reason(&mut reasons, matches.decisions.len(), "decision link(s)");
    push_count_reason(&mut reasons, matches.works.len(), "work record link(s)");
    reasons
}

fn push_count_reason(reasons: &mut Vec<String>, count: usize, label: &str) {
    if count > 0 {
        reasons.push(format!("{count} {label}"));
    }
}

fn record_ids(records: &[GitConnectedRecord]) -> BTreeSet<String> {
    records.iter().map(|record| record.id.clone()).collect()
}

fn git_link_suggestions(artifact: &ProjectAnalysis, commit: &GitCommit) -> Vec<GitSuggestion> {
    suggested_expectations(artifact, &format!("{}\n{}", commit.subject, commit.body))
        .into_iter()
        .map(|candidate| GitSuggestion {
            command: format!(
                "susumu git link {} {}",
                short_hash(&commit.hash),
                candidate.id
            ),
            expectation_id: candidate.id,
            title: candidate.title,
            score: candidate.score,
        })
        .collect()
}

pub(crate) fn suggested_expectations(
    artifact: &ProjectAnalysis,
    searchable: &str,
) -> Vec<SuggestedExpectation> {
    let searchable_tokens = expectation_language_tokens(searchable);
    if searchable_tokens.is_empty() {
        return Vec::new();
    }
    let mut matches = artifact
        .expectations
        .iter()
        .filter_map(|expectation| {
            let mut expectation_text = expectation.title.clone();
            expectation_text.push(' ');
            expectation_text.push_str(&expectation.detail);
            let expectation_tokens = expectation_language_tokens(&expectation_text);
            let overlap = expectation_tokens.intersection(&searchable_tokens).count();
            (overlap >= 2).then(|| SuggestedExpectation {
                id: expectation.id.clone(),
                title: expectation.title.clone(),
                score: overlap,
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });
    matches.truncate(3);
    matches
}

fn connected_workflows(
    artifact: &ProjectAnalysis,
    text: &str,
    file_ids: &[String],
) -> Vec<GitConnectedRecord> {
    let mut records = artifact
        .workflows
        .iter()
        .filter_map(|workflow| {
            let explicit = contains_token(text, &workflow.id);
            let file_match = file_ids.iter().any(|file_id| file_id == &workflow.file_id);
            (explicit || file_match).then(|| GitConnectedRecord {
                id: workflow.id.clone(),
                title: format!("{} ({})", workflow.trigger, workflow.framework),
                reason: if explicit {
                    "commit text mentions workflow id".to_owned()
                } else {
                    "commit changed workflow file".to_owned()
                },
            })
        })
        .collect::<Vec<_>>();
    sort_connected_records(&mut records);
    records
}

fn connected_expectations(
    artifact: &ProjectAnalysis,
    text: &str,
    file_ids: &[String],
    workflow_ids: &BTreeSet<String>,
) -> Vec<GitConnectedRecord> {
    let language_match = single_language_matched_expectation(artifact, text);
    let mut records = artifact
        .expectations
        .iter()
        .filter_map(|expectation| {
            let explicit = contains_token(text, &expectation.id);
            let reason = if explicit {
                Some("commit text mentions expectation id")
            } else if language_match.as_deref() == Some(expectation.id.as_str()) {
                Some("commit text matches expectation language")
            } else {
                match (expectation.target, expectation.subject.as_deref()) {
                    (ExpectationTarget::Workflow, Some(subject))
                        if workflow_ids.contains(subject) =>
                    {
                        Some("expectation targets matched workflow")
                    }
                    (ExpectationTarget::File, Some(subject))
                        if file_ids.iter().any(|file_id| file_id == subject) =>
                    {
                        Some("expectation targets changed file")
                    }
                    _ => None,
                }
            }?;
            Some(GitConnectedRecord {
                id: expectation.id.clone(),
                title: expectation.title.clone(),
                reason: reason.to_owned(),
            })
        })
        .collect::<Vec<_>>();
    sort_connected_records(&mut records);
    records
}

fn connected_verifications(
    artifact: &ProjectAnalysis,
    text: &str,
    expectation_ids: &BTreeSet<String>,
) -> Vec<GitConnectedRecord> {
    let mut records = artifact
        .verifications
        .iter()
        .filter_map(|verification| {
            let explicit = contains_token(text, &verification.id);
            let expectation_match = expectation_ids.contains(&verification.expectation_id);
            (explicit || expectation_match).then(|| GitConnectedRecord {
                id: verification.id.clone(),
                title: expectation_title(artifact, &verification.expectation_id),
                reason: if explicit {
                    "commit text mentions verification id".to_owned()
                } else {
                    "verification checks matched expectation".to_owned()
                },
            })
        })
        .collect::<Vec<_>>();
    sort_connected_records(&mut records);
    records
}

fn connected_decisions(
    artifact: &ProjectAnalysis,
    text: &str,
    file_ids: &[String],
    workflow_ids: &BTreeSet<String>,
) -> Vec<GitConnectedRecord> {
    let mut records = artifact
        .decisions
        .iter()
        .filter_map(|decision| {
            let explicit = contains_token(text, &decision.id);
            let reason = if explicit {
                Some("commit text mentions decision id")
            } else {
                match (decision.target, decision.subject.as_deref()) {
                    (ExpectationTarget::Workflow, Some(subject))
                        if workflow_ids.contains(subject) =>
                    {
                        Some("decision targets matched workflow")
                    }
                    (ExpectationTarget::File, Some(subject))
                        if file_ids.iter().any(|file_id| file_id == subject) =>
                    {
                        Some("decision targets changed file")
                    }
                    _ => None,
                }
            }?;
            Some(GitConnectedRecord {
                id: decision.id.clone(),
                title: decision.title.clone(),
                reason: reason.to_owned(),
            })
        })
        .collect::<Vec<_>>();
    sort_connected_records(&mut records);
    records
}

fn connected_works(
    artifact: &ProjectAnalysis,
    text: &str,
    commit_hash: &str,
) -> Vec<GitConnectedRecord> {
    let mut records = artifact
        .works
        .iter()
        .filter_map(|work| {
            let explicit = contains_token(text, &work.id);
            let evidence = work
                .evidence
                .as_deref()
                .and_then(commit_evidence_hash)
                .is_some_and(|hash| commit_hash_matches(commit_hash, hash));
            (explicit || evidence).then(|| GitConnectedRecord {
                id: work.id.clone(),
                title: work.title.clone(),
                reason: if explicit {
                    "commit text mentions work id".to_owned()
                } else {
                    "work evidence references commit".to_owned()
                },
            })
        })
        .collect::<Vec<_>>();
    sort_connected_records(&mut records);
    records
}

pub(crate) fn missing_expectation_work_records(
    artifact: &ProjectAnalysis,
    commit_hash: &str,
    expectations: &[GitConnectedRecord],
) -> Vec<GitConnectedRecord> {
    expectations
        .iter()
        .filter(|expectation| {
            !artifact.works.iter().any(|work| {
                work.expectation_id.as_deref() == Some(expectation.id.as_str())
                    && work
                        .evidence
                        .as_deref()
                        .and_then(commit_evidence_hash)
                        .is_some_and(|hash| commit_hash_matches(commit_hash, hash))
            })
        })
        .cloned()
        .collect()
}

fn commit_evidence_hash(evidence: &str) -> Option<&str> {
    evidence
        .strip_prefix("commit:")
        .filter(|hash| !hash.is_empty())
}

fn commit_hash_matches(commit_hash: &str, evidence_hash: &str) -> bool {
    commit_hash.starts_with(evidence_hash) || evidence_hash.starts_with(commit_hash)
}

fn sort_connected_records(records: &mut [GitConnectedRecord]) {
    records.sort_by(|left, right| left.id.cmp(&right.id));
}

pub(crate) fn matched_artifact_file_ids(
    artifact: &ProjectAnalysis,
    changed_files: &[String],
) -> Vec<String> {
    let mut matched = artifact
        .files
        .iter()
        .filter(|file| {
            let artifact_path = normalize_git_path(&file.path);
            changed_files.iter().any(|path| path == &artifact_path)
        })
        .map(|file| file.id.clone())
        .collect::<Vec<_>>();
    matched.sort();
    matched.dedup();
    matched
}

pub(crate) fn single_language_matched_expectation(
    artifact: &ProjectAnalysis,
    searchable: &str,
) -> Option<String> {
    let searchable_tokens = expectation_language_tokens(searchable);
    let mut matches = artifact
        .expectations
        .iter()
        .filter_map(|expectation| {
            let mut expectation_text = expectation.title.clone();
            expectation_text.push(' ');
            expectation_text.push_str(&expectation.detail);
            let expectation_tokens = expectation_language_tokens(&expectation_text);
            let overlap = expectation_tokens.intersection(&searchable_tokens).count();
            (overlap >= 2).then(|| (expectation.id.clone(), overlap))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let (id, score) = matches.first()?;
    if *score < 2 {
        return None;
    }
    if matches
        .get(1)
        .is_some_and(|(_, next_score)| next_score >= score)
    {
        return None;
    }
    Some(id.clone())
}

fn expectation_language_tokens(text: &str) -> BTreeSet<String> {
    text.split(is_token_boundary)
        .filter_map(normalize_expectation_language_token)
        .collect()
}

fn normalize_expectation_language_token(token: &str) -> Option<String> {
    let mut token = token.to_ascii_lowercase();
    if token.len() < 3 || expectation_language_stop_word(&token) {
        return None;
    }
    for suffix in ["ing", "ed", "s"] {
        if token.len() > suffix.len() + 2 && token.ends_with(suffix) {
            token.truncate(token.len() - suffix.len());
            break;
        }
    }
    if token.len() < 3 || expectation_language_stop_word(&token) {
        None
    } else {
        Some(token)
    }
}

fn expectation_language_stop_word(token: &str) -> bool {
    matches!(
        token,
        "about"
            | "able"
            | "after"
            | "before"
            | "code"
            | "current"
            | "detail"
            | "from"
            | "have"
            | "into"
            | "project"
            | "record"
            | "records"
            | "repository"
            | "should"
            | "show"
            | "susumu"
            | "that"
            | "their"
            | "when"
            | "which"
            | "with"
            | "workflow"
            | "workflows"
    )
}

pub(crate) fn contains_token(haystack: &str, needle: &str) -> bool {
    haystack
        .split(is_token_boundary)
        .any(|token| token == needle)
}

const fn is_token_boundary(character: char) -> bool {
    !(character.is_ascii_alphanumeric() || character == '_')
}

fn expectation_title(analysis: &ProjectAnalysis, id: &str) -> String {
    analysis
        .expectations
        .iter()
        .find(|expectation| expectation.id == id)
        .map_or_else(|| id.to_owned(), |expectation| expectation.title.clone())
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(8).collect()
}
