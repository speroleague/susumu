use susumu::model::{Finding, Severity, Verification, VerificationStatus};

use crate::git::types::GitCommit;
use crate::test_support::test_artifact;

use super::{apply_commit_attribution, commits_touching, project_source_paths, stale_targets};

fn commit(hash: &str, subject: &str, changed: &[&str]) -> GitCommit {
    GitCommit {
        hash: hash.to_owned(),
        author_name: "Test".to_owned(),
        author_email: "test@example.test".to_owned(),
        author_date: "2026-01-01T00:00:00Z".to_owned(),
        subject: subject.to_owned(),
        body: String::new(),
        changed_files: changed.iter().map(|path| (*path).to_owned()).collect(),
    }
}

fn dirty_verification_artifact() -> susumu::model::ProjectAnalysis {
    let mut analysis = test_artifact();
    analysis.source_revision = Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned());
    analysis.verifications.push(Verification {
        id: "v_checkout".to_owned(),
        expectation_id: "e_checkout_sequence".to_owned(),
        status: VerificationStatus::Passed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "cargo test".to_owned(),
        source: "human:test".to_owned(),
        evidence: None,
        basis: Some("review-v2:old".to_owned()),
        revision: Some("1111111111111111111111111111111111111111".to_owned()),
        detail: "recorded".to_owned(),
    });
    analysis.findings.push(Finding {
        rule_id: "SUS023".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Verification evidence changed".to_owned(),
        detail: "generic detail".to_owned(),
        file_id: Some("f_api".to_owned()),
        subject: Some("v_checkout".to_owned()),
        location: None,
    });
    analysis
}

fn attribute(analysis: &mut susumu::model::ProjectAnalysis, commits: &[GitCommit]) {
    let target = stale_targets(analysis)[0].clone();
    let paths = project_source_paths(analysis);
    apply_commit_attribution(analysis, &target, commits, &paths);
}

#[test]
fn stale_targets_pairs_dirty_finding_with_record_revision() {
    let analysis = dirty_verification_artifact();

    let targets = stale_targets(&analysis);

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].record_id, "v_checkout");
    assert_eq!(targets[0].path.as_deref(), Some("src/api.ts"));
    assert_eq!(
        targets[0].revision.as_deref(),
        Some("1111111111111111111111111111111111111111")
    );
}

#[test]
fn stale_targets_ignores_non_dirty_findings() {
    let mut analysis = dirty_verification_artifact();
    analysis.findings[0].rule_id = "SUS001".to_owned();

    assert!(stale_targets(&analysis).is_empty());
}

#[test]
fn stale_targets_keeps_project_target_findings_without_a_file() {
    let mut analysis = dirty_verification_artifact();
    analysis.findings[0].file_id = None;

    let targets = stale_targets(&analysis);

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, None);
    assert_eq!(targets[0].record_id, "v_checkout");
}

#[test]
fn commits_touching_matches_normalized_paths() {
    let commits = [
        commit("c1", "touch api", &["./src/api.ts"]),
        commit("c2", "touch routes", &["src/routes.php"]),
    ];

    let touching = commits_touching("src/api.ts", &commits);

    assert_eq!(touching.len(), 1);
    assert_eq!(touching[0].hash, "c1");
}

#[test]
fn apply_commit_attribution_names_the_causing_commit() {
    let mut analysis = dirty_verification_artifact();
    let commits = [commit(
        "abc1234def5678",
        "reorder reservation",
        &["src/api.ts"],
    )];

    attribute(&mut analysis, &commits);

    let detail = &analysis.findings[0].detail;
    assert!(detail.contains("v_checkout"));
    assert!(detail.contains("src/api.ts"));
    assert!(detail.contains("abc1234"));
    assert!(detail.contains("reorder reservation"));
    assert!(detail.contains("1 commit"));
}

#[test]
fn apply_commit_attribution_is_noop_without_a_touching_commit() {
    let mut analysis = dirty_verification_artifact();
    let commits = [commit("zzz", "unrelated", &["docs/README.md"])];

    attribute(&mut analysis, &commits);

    assert_eq!(analysis.findings[0].detail, "generic detail");
}

#[test]
fn project_target_is_attributed_to_any_scanned_source_change() {
    let mut analysis = dirty_verification_artifact();
    analysis.findings[0].file_id = None;
    let commits = [
        commit("d1", "unrelated docs", &["docs/guide.md"]),
        commit("d2", "tweak routes", &["src/routes.php"]),
    ];

    attribute(&mut analysis, &commits);

    let detail = &analysis.findings[0].detail;
    assert!(detail.contains("the project has changed"));
    assert!(detail.contains("d2"));
    assert!(detail.contains("1 commit"));
    assert!(!detail.contains("d1"));
}

#[test]
fn commit_list_is_capped_and_summarized() {
    let mut analysis = dirty_verification_artifact();
    let commits: Vec<GitCommit> = (0..8)
        .map(|n| {
            commit(
                &format!("c{n}00000"),
                &format!("change {n}"),
                &["src/api.ts"],
            )
        })
        .collect();

    attribute(&mut analysis, &commits);

    let detail = &analysis.findings[0].detail;
    assert!(detail.contains("8 commit(s)"));
    assert!(detail.contains("and 3 more"));
}
