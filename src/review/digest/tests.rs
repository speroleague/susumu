use susumu::model::{
    Finding, ReviewAnchor, ReviewCommentKind, ReviewStatus, ReviewThread, Severity, Verification,
    VerificationStatus,
};

use crate::review::checks::check_report;
use crate::test_support::test_artifact;

use super::{DIGEST_SCHEMA_VERSION, Digest};

fn passed_verification(id: &str, expectation_id: &str) -> Verification {
    Verification {
        id: id.to_owned(),
        expectation_id: expectation_id.to_owned(),
        status: VerificationStatus::Passed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "manual".to_owned(),
        source: "human:test".to_owned(),
        evidence: None,
        basis: Some("review-v2:x".to_owned()),
        revision: Some("1111111".to_owned()),
        detail: "checked".to_owned(),
    }
}

#[test]
fn digest_is_empty_when_nothing_needs_attention() {
    let mut analysis = test_artifact();
    analysis
        .verifications
        .push(passed_verification("v_ok", "e_checkout_sequence"));

    let check = check_report(&analysis, false);
    let digest = Digest::build(&analysis, &check);

    assert_eq!(digest.schema_version, DIGEST_SCHEMA_VERSION);
    assert_eq!(digest.review.critical, check.critical);
    assert_eq!(digest.review.warning, check.warning);
    assert_eq!(digest.review.attention, check.attention);
    assert_eq!(digest.result.status, "clear");
    assert!(!digest.integrations.middleman.detected);
    assert_eq!(digest.attention_count, 0);
    assert!(digest.needs_reverification.is_empty());
}

#[test]
fn digest_detects_middleman_only_from_its_conventional_marker() {
    let temporary = tempfile::tempdir().expect("create temporary project");
    std::fs::create_dir_all(temporary.path().join(".middleman")).expect("create marker parent");
    std::fs::write(
        temporary.path().join(".middleman/middleman.toml"),
        "schema_version = 1\n",
    )
    .expect("write marker");

    let mut analysis = test_artifact();
    analysis.root = temporary.path().display().to_string();
    let check = check_report(&analysis, false);
    let digest = Digest::build(&analysis, &check);

    assert!(digest.integrations.middleman.detected);
}

#[test]
fn digest_reports_reverification_open_threads_and_missing_verification() {
    let mut analysis = test_artifact();
    analysis
        .verifications
        .push(passed_verification("v_checkout", "e_checkout_sequence"));
    analysis.findings.push(Finding {
        rule_id: "SUS023".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Verification evidence changed".to_owned(),
        detail: "v_checkout was recorded against `abc1234`. src/api.ts has changed in 1 commit(s) since: def5678 \"fix\".".to_owned(),
        file_id: Some("f_api".to_owned()),
        subject: Some("v_checkout".to_owned()),
        location: None,
    });
    analysis.review_threads.push(ReviewThread {
        id: "r_1".to_owned(),
        target: "project".parse().unwrap(),
        subject: None,
        anchor: Some(ReviewAnchor::Expectation("e_checkout_sequence".to_owned())),
        parent: None,
        kind: ReviewCommentKind::Question,
        status: ReviewStatus::Open,
        owner: Some("team-platform".to_owned()),
        source: "human:reviewer".to_owned(),
        title: "Clarify reservation ordering".to_owned(),
        detail: "Which comes first?".to_owned(),
    });

    let check = check_report(&analysis, false);
    let digest = Digest::build(&analysis, &check);

    assert_eq!(digest.needs_reverification.len(), 1);
    assert_eq!(digest.needs_reverification[0].id, "e_checkout_sequence");
    assert!(digest.needs_reverification[0].detail.contains("def5678"));

    assert_eq!(digest.open_threads.len(), 1);
    assert_eq!(digest.open_threads[0].id, "r_1");
    assert!(digest.open_threads[0].detail.contains("team-platform"));

    // test_artifact has one expectation; it now has a (dirty) verification, so it
    // is not in "expectations without verification".
    assert!(digest.expectations_without_verification.is_empty());
    assert!(digest.attention_count >= 2);
}

#[test]
fn digest_excludes_resolved_review_threads() {
    let mut analysis = test_artifact();
    analysis.review_threads.push(ReviewThread {
        id: "r_done".to_owned(),
        target: "project".parse().unwrap(),
        subject: None,
        anchor: None,
        parent: None,
        kind: ReviewCommentKind::Comment,
        status: ReviewStatus::Resolved,
        owner: None,
        source: "human:reviewer".to_owned(),
        title: "Handled".to_owned(),
        detail: "done".to_owned(),
    });

    let check = check_report(&analysis, false);
    let digest = Digest::build(&analysis, &check);

    assert!(digest.open_threads.is_empty());
}
