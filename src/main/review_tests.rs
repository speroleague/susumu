use super::test_support::*;
use super::*;
use susumu::model::{Finding, Severity};

fn project_expectations() -> Vec<Expectation> {
    [
        (
            "e_project_one",
            "First project expectation",
            "First expectation.",
        ),
        (
            "e_project_two",
            "Second project expectation",
            "Second expectation.",
        ),
    ]
    .into_iter()
    .map(|(id, title, detail)| Expectation {
        id: id.to_owned(),
        target: ExpectationTarget::Project,
        subject: None,
        status: ExpectationStatus::Accepted,
        source: "human:test".to_owned(),
        title: title.to_owned(),
        detail: detail.to_owned(),
    })
    .collect()
}

fn expectation_support_work() -> Work {
    Work {
        id: "wk_one".to_owned(),
        target: ExpectationTarget::Project,
        subject: None,
        expectation_id: Some("e_project_one".to_owned()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "import:test".to_owned(),
        evidence: Some("commit:abc123".to_owned()),
        title: "Support first expectation".to_owned(),
        detail: "Only the explicitly linked expectation should count this work.".to_owned(),
    }
}

fn expectation_support_fixture() -> ProjectAnalysis {
    let mut artifact = test_artifact();
    artifact.expectations = project_expectations();
    artifact.works.push(expectation_support_work());
    artifact.verifications.push(Verification {
        id: "v_one_passed".to_owned(),
        expectation_id: "e_project_one".to_owned(),
        status: VerificationStatus::Passed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "cargo test".to_owned(),
        source: "ci:test".to_owned(),
        evidence: Some("run:1".to_owned()),
        basis: None,
        detail: "Passed.".to_owned(),
    });
    artifact.verifications.push(Verification {
        id: "v_one_failed".to_owned(),
        expectation_id: "e_project_one".to_owned(),
        status: VerificationStatus::Failed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "manual review".to_owned(),
        source: "human:test".to_owned(),
        evidence: Some("review:1".to_owned()),
        basis: None,
        detail: "Failed.".to_owned(),
    });
    artifact.verifications.push(Verification {
        id: "v_one_inconclusive".to_owned(),
        expectation_id: "e_project_one".to_owned(),
        status: VerificationStatus::Inconclusive,
        supersedes: None,
        execution: None,
        chain: None,
        method: "log review".to_owned(),
        source: "human:test".to_owned(),
        evidence: None,
        basis: None,
        detail: "Inconclusive.".to_owned(),
    });
    artifact.decisions.push(Decision {
        id: "d_project".to_owned(),
        target: ExpectationTarget::Project,
        subject: None,
        status: DecisionStatus::Accepted,
        source: "human:test".to_owned(),
        basis: None,
        title: "Accept project direction".to_owned(),
        detail: "Project-wide decision context should count for project expectations.".to_owned(),
    });
    artifact.expectations.push(Expectation {
        id: "e_workflow_gap".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        status: ExpectationStatus::Accepted,
        source: "human:test".to_owned(),
        title: "Workflow support includes findings".to_owned(),
        detail: "Findings tied to the workflow should appear in expectation support.".to_owned(),
    });
    artifact.findings.push(Finding {
        rule_id: "SUS999".to_owned(),
        source: "test".to_owned(),
        severity: Severity::Warning,
        title: "Workflow finding".to_owned(),
        detail: "A workflow finding should be counted.".to_owned(),
        file_id: Some("f_api".to_owned()),
        subject: Some("w_checkout".to_owned()),
        location: Some(test_location()),
    });

    artifact
}

#[test]
fn expectation_support_counts_expectation_specific_work_only_once() {
    let artifact = expectation_support_fixture();

    let support = expectation_support(&artifact);
    let first = support
        .iter()
        .find(|item| item.expectation_id == "e_project_one")
        .expect("first support");
    let second = support
        .iter()
        .find(|item| item.expectation_id == "e_project_two")
        .expect("second support");
    let workflow = support
        .iter()
        .find(|item| item.expectation_id == "e_workflow_gap")
        .expect("workflow support");

    assert!(first.target_observed);
    assert_eq!(first.verification.passed, 1);
    assert_eq!(first.verification.failed, 1);
    assert_eq!(first.verification.inconclusive, 1);
    assert_eq!(first.work, 1);
    assert_eq!(first.decisions, 1);
    assert_eq!(first.findings, 0);
    assert_eq!(first.support_status, "failed_verification");
    assert!(
        first
            .reasons
            .iter()
            .any(|reason| reason == "target observed")
    );
    assert!(
        first
            .reasons
            .iter()
            .any(|reason| reason == "1 failed verification record(s)")
    );
    assert!(
        first
            .reasons
            .iter()
            .any(|reason| reason == "1 linked work record(s)")
    );
    assert!(
        first
            .reasons
            .iter()
            .any(|reason| reason == "1 linked decision record(s)")
    );
    assert_eq!(second.work, 0);
    assert_eq!(second.decisions, 1);
    assert_eq!(second.support_status, "partially_supported");
    assert_eq!(workflow.findings, 1);
    assert_eq!(workflow.support_status, "partially_supported");
    assert!(
        workflow
            .reasons
            .iter()
            .any(|reason| reason == "1 linked finding(s)")
    );
}

#[test]
fn scanner_support_does_not_verify_expectations_without_passed_verification() {
    let mut artifact = test_artifact();
    refresh_derived_analysis(&mut artifact);
    artifact.works.push(Work {
        id: "wk_checkout".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        expectation_id: Some("e_checkout_sequence".to_owned()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "agent:test".to_owned(),
        evidence: Some("commit:abc123".to_owned()),
        title: "Implement checkout sequence".to_owned(),
        detail: "Work supports the expectation but does not prove it.".to_owned(),
    });
    artifact.findings.push(Finding {
        rule_id: "SUS998".to_owned(),
        source: "scanner:test".to_owned(),
        severity: Severity::Info,
        title: "Observed checkout workflow".to_owned(),
        detail: "Scanner-observed evidence is support, not verification.".to_owned(),
        file_id: Some("f_api".to_owned()),
        subject: Some("w_checkout".to_owned()),
        location: Some(test_location()),
    });

    let support = expectation_support(&artifact);
    let readiness = crate::expectation_readiness::expectation_readiness(&artifact, &support);
    let checkout_support = support
        .iter()
        .find(|item| item.expectation_id == "e_checkout_sequence")
        .expect("checkout support");
    let checkout_readiness = readiness
        .iter()
        .find(|item| item.expectation_id == "e_checkout_sequence")
        .expect("checkout readiness");

    assert!(checkout_support.target_observed);
    assert_eq!(checkout_support.work, 1);
    assert_eq!(checkout_support.findings, 1);
    assert_eq!(checkout_support.verification.passed, 0);
    assert_eq!(checkout_support.support_status, "partially_supported");
    assert_eq!(checkout_readiness.bucket, "needs_verification");
    assert!(
        checkout_readiness
            .next_action
            .contains("susumu verify e_checkout_sequence")
    );

    artifact.verifications.push(Verification {
        id: "v_checkout".to_owned(),
        expectation_id: "e_checkout_sequence".to_owned(),
        status: VerificationStatus::Passed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "cargo test checkout".to_owned(),
        source: "ci:test".to_owned(),
        evidence: Some("run:checkout".to_owned()),
        basis: None,
        detail: "Passed verification promotes support to verified.".to_owned(),
    });

    let verified_support = expectation_support(&artifact);
    let verified_readiness =
        crate::expectation_readiness::expectation_readiness(&artifact, &verified_support);
    let checkout_support = verified_support
        .iter()
        .find(|item| item.expectation_id == "e_checkout_sequence")
        .expect("verified support");
    let checkout_readiness = verified_readiness
        .iter()
        .find(|item| item.expectation_id == "e_checkout_sequence")
        .expect("verified readiness");

    assert_eq!(checkout_support.verification.passed, 1);
    assert_eq!(checkout_support.support_status, "verified");
    assert_eq!(checkout_readiness.bucket, "verified");
}

#[test]
fn review_build_writes_artifact_packet_check_and_html() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("expectations.susu"),
        "expectation e_build target=project subject=- status=accepted source=\"human:test\" title=\"Build review\" detail=\"Review build should load this sidecar.\";\n",
    )
    .expect("write expectations sidecar");
    let artifact = temp.path().join("target").join("project.susu");
    let packet = temp.path().join("target").join("project.review.susu");
    let check = temp.path().join("target").join("project.check.json");
    let html = temp.path().join("target").join("project.review.html");

    build_review(&ReviewBuildArgs {
        target: temp.path().to_path_buf(),
        expectations: None,
        verifications: None,
        decisions: None,
        work: None,
        artifact_output: artifact.clone(),
        output: packet.clone(),
        check_json: Some(check.clone()),
        html: Some(html.clone()),
        strict: false,
        fail_on_check: false,
        json: false,
        serve: false,
        host: "127.0.0.1".to_owned(),
        port: 0,
    })
    .expect("review build should write outputs");

    assert!(artifact.exists());
    assert!(packet.exists());
    assert!(check.exists());
    assert!(html.exists());
    let built = read_analysis_artifact(&artifact).expect("read built artifact");
    assert!(
        built
            .expectations
            .iter()
            .any(|expectation| expectation.id == "e_build")
    );
}

#[test]
fn core_review_build_has_no_ai_dependency() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(temp.path().join("main.rs"), "fn main() {}\n").expect("write source");
    fs::write(
        temp.path().join("expectations.susu"),
        "expectation e_ai_optional target=project subject=- status=accepted source=\"human:test\" title=\"AI stays optional\" detail=\"Core scan, check, review packet, and portal output should work without AI configuration.\";\n",
    )
    .expect("write expectations sidecar");

    let artifact = temp.path().join("target").join("project.susu");
    let packet = temp.path().join("target").join("project.review.susu");
    let check = temp.path().join("target").join("project.check.json");
    let html = temp.path().join("target").join("project.review.html");

    build_review(&ReviewBuildArgs {
        target: temp.path().to_path_buf(),
        expectations: None,
        verifications: None,
        decisions: None,
        work: None,
        artifact_output: artifact.clone(),
        output: packet.clone(),
        check_json: Some(check.clone()),
        html: Some(html.clone()),
        strict: false,
        fail_on_check: false,
        json: false,
        serve: false,
        host: "127.0.0.1".to_owned(),
        port: 0,
    })
    .expect("core review build should not require AI configuration");

    let built = read_analysis_artifact(&artifact).expect("read built artifact");
    assert!(
        built
            .expectations
            .iter()
            .any(|expectation| expectation.id == "e_ai_optional")
    );
    assert!(check.exists());

    let packet = read_review_packet(&packet).expect("read review packet");
    assert!(
        packet
            .artifact
            .expectations
            .iter()
            .any(|expectation| expectation.id == "e_ai_optional")
    );
    assert!(
        fs::read_to_string(&html)
            .expect("read html portal")
            .contains("AI stays optional")
    );

    let readme = include_str!("../../README.md");
    let vision = include_str!("../../docs/vision.md");
    let vernacular = include_str!("../../docs/vernacular.md");
    assert!(readme.contains("runs without AI keys"));
    assert!(vision.contains("The core product does not require AI."));
    assert!(vision.contains("Optional bring-your-own-key AI"));
    assert!(vision.contains("labeled as generated"));
    assert!(vision.contains("cite underlying evidence"));
    assert!(vision.contains("human acceptance"));
    assert!(vernacular.contains("source=\"ai:draft\""));
    assert!(vernacular.contains("status=proposed"));
}
