use crate::model::{
    Confidence, Decision, DecisionStatus, Expectation, ExpectationStatus, ExpectationTarget,
    Language, Location, ProjectAnalysis, SourceFile, Symbol, SymbolKind, Verification,
    VerificationStatus, Work, WorkKind, WorkStatus, Workflow, WorkflowKind,
};

use super::*;

fn analysis_with_expectations(expectations: Vec<Expectation>) -> ProjectAnalysis {
    ProjectAnalysis {
        schema_version: 1,
        project_name: "demo".to_owned(),
        root: ".".to_owned(),
        generated_unix_seconds: 0,
        files: vec![SourceFile {
            id: "f_main".to_owned(),
            path: "src/main.rs".to_owned(),
            language: Language::Rust,
            lines: 10,
            bytes: 100,
            content_hash: Some("hash0".to_owned()),
        }],
        symbols: vec![Symbol {
            id: "s_checkout".to_owned(),
            name: "checkout".to_owned(),
            kind: SymbolKind::Function,
            file_id: "f_main".to_owned(),
            content_hash: Some("symbol_hash0".to_owned()),
            location: Location {
                start_line: 1,
                start_column: 1,
                end_line: 3,
                end_column: 2,
            },
            entrypoint: false,
        }],
        dependencies: Vec::new(),
        workflows: vec![Workflow {
            id: "w_checkout".to_owned(),
            kind: WorkflowKind::Http,
            framework: "axum-compatible".to_owned(),
            trigger: "POST /checkout".to_owned(),
            handler: Some("checkout".to_owned()),
            entry_symbol: Some("s_checkout".to_owned()),
            file_id: "f_main".to_owned(),
            confidence: Confidence::Exact,
            location: Location {
                start_line: 5,
                start_column: 1,
                end_line: 5,
                end_column: 40,
            },
        }],
        workflow_priorities: Vec::new(),
        flows: Vec::new(),
        expectations,
        verifications: Vec::new(),
        decisions: Vec::new(),
        works: Vec::new(),
        findings: Vec::new(),
    }
}

fn expectation(id: &str, target: ExpectationTarget, subject: Option<&str>) -> Expectation {
    Expectation {
        id: id.to_owned(),
        target,
        subject: subject.map(ToOwned::to_owned),
        status: ExpectationStatus::Accepted,
        source: "human:test".to_owned(),
        title: "Test expectation".to_owned(),
        detail: "Test detail.".to_owned(),
    }
}

fn work(id: &str, target: ExpectationTarget, subject: Option<&str>) -> Work {
    Work {
        id: id.to_owned(),
        target,
        subject: subject.map(ToOwned::to_owned),
        expectation_id: None,
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "agent:test".to_owned(),
        evidence: Some("commit:test".to_owned()),
        title: "Test work".to_owned(),
        detail: "Test work detail.".to_owned(),
    }
}

#[test]
fn expectation_findings_flag_missing_and_stale_targets() {
    let mut analysis = analysis_with_expectations(vec![
        expectation("e_valid", ExpectationTarget::Workflow, Some("w_checkout")),
        expectation("e_missing", ExpectationTarget::Symbol, None),
        expectation("e_stale", ExpectationTarget::File, Some("f_missing")),
    ]);

    refresh_expectation_findings(&mut analysis);
    refresh_expectation_findings(&mut analysis);

    assert_eq!(
        analysis
            .findings
            .iter()
            .filter(|finding| finding.rule_id == "SUS010")
            .count(),
        1
    );
    assert_eq!(
        analysis
            .findings
            .iter()
            .filter(|finding| finding.rule_id == "SUS011")
            .count(),
        1
    );
}

#[test]
fn project_expectations_should_not_carry_subject_ids() {
    let mut analysis = analysis_with_expectations(vec![expectation(
        "e_project",
        ExpectationTarget::Project,
        Some("f_main"),
    )]);

    refresh_expectation_findings(&mut analysis);

    assert!(analysis.findings.iter().any(|finding| {
        finding.rule_id == "SUS012" && finding.subject.as_deref() == Some("e_project")
    }));
}

#[test]
fn verification_findings_flag_missing_expectations() {
    let mut analysis = analysis_with_expectations(vec![expectation(
        "e_known",
        ExpectationTarget::Project,
        None,
    )]);
    analysis.verifications.push(Verification {
        id: "v_stale".to_owned(),
        expectation_id: "e_missing".to_owned(),
        status: VerificationStatus::Inconclusive,
        supersedes: None,
        execution: None,
        chain: None,
        method: "manual review".to_owned(),
        source: "human:test".to_owned(),
        evidence: None,
        basis: None,
        detail: "Could not find the linked expectation.".to_owned(),
    });

    refresh_relationship_findings(&mut analysis);

    assert!(analysis.findings.iter().any(|finding| {
        finding.rule_id == "SUS020" && finding.subject.as_deref() == Some("v_stale")
    }));
}

#[test]
fn work_findings_flag_missing_targets_and_expectations() {
    let mut analysis = analysis_with_expectations(vec![expectation(
        "e_known",
        ExpectationTarget::Workflow,
        Some("w_checkout"),
    )]);
    let mut stale_work = work("w_stale", ExpectationTarget::Workflow, Some("w_missing"));
    stale_work.expectation_id = Some("e_missing".to_owned());
    analysis.works.push(stale_work);

    refresh_relationship_findings(&mut analysis);

    assert!(analysis.findings.iter().any(|finding| {
        finding.rule_id == "SUS041" && finding.subject.as_deref() == Some("w_stale")
    }));
    assert!(analysis.findings.iter().any(|finding| {
        finding.rule_id == "SUS043" && finding.subject.as_deref() == Some("w_stale")
    }));
}

#[test]
fn workflow_priority_scores_explain_attention() {
    let mut analysis = analysis_with_expectations(vec![expectation(
        "e_checkout",
        ExpectationTarget::Workflow,
        Some("w_checkout"),
    )]);
    analysis.verifications.push(Verification {
        id: "v_checkout".to_owned(),
        expectation_id: "e_checkout".to_owned(),
        status: VerificationStatus::Failed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "manual review".to_owned(),
        source: "human:test".to_owned(),
        evidence: None,
        basis: None,
        detail: "Checkout behavior did not match the expectation.".to_owned(),
    });

    refresh_workflow_priorities(&mut analysis);

    let priority = analysis.workflow_priorities.first().unwrap();
    assert_eq!(priority.workflow_id, "w_checkout");
    assert!(priority.score > 50);
    assert!(priority.detail.contains("accepted expectation linked"));
    assert!(priority.detail.contains("failed verification linked"));
}

#[test]
fn decision_basis_marks_changed_evidence_for_review() {
    let mut analysis = analysis_with_expectations(Vec::new());
    analysis.decisions.push(Decision {
        id: "d_checkout".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        status: DecisionStatus::Accepted,
        source: "human:test".to_owned(),
        basis: None,
        title: "Accept checkout shape".to_owned(),
        detail: "Checkout shape accepted for this test.".to_owned(),
    });

    anchor_decision_bases(&mut analysis);
    assert!(analysis.decisions[0].basis.is_some());

    analysis.files[0].content_hash = Some("hash1".to_owned());
    analysis.symbols[0].content_hash = Some("symbol_hash1".to_owned());
    refresh_relationship_findings(&mut analysis);

    assert!(analysis.findings.iter().any(|finding| {
        finding.rule_id == "SUS033" && finding.subject.as_deref() == Some("d_checkout")
    }));
}

#[test]
fn verification_basis_marks_changed_evidence_for_review() {
    let mut analysis = analysis_with_expectations(vec![expectation(
        "e_checkout",
        ExpectationTarget::Workflow,
        Some("w_checkout"),
    )]);
    analysis.verifications.push(Verification {
        id: "v_checkout".to_owned(),
        expectation_id: "e_checkout".to_owned(),
        status: VerificationStatus::Passed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "manual review".to_owned(),
        source: "human:test".to_owned(),
        evidence: Some("review:test".to_owned()),
        basis: None,
        detail: "Checkout behavior matched the expectation.".to_owned(),
    });

    anchor_verification_bases(&mut analysis);
    assert!(analysis.verifications[0].basis.is_some());

    analysis.files[0].content_hash = Some("hash1".to_owned());
    analysis.symbols[0].content_hash = Some("symbol_hash1".to_owned());
    refresh_relationship_findings(&mut analysis);

    assert!(analysis.findings.iter().any(|finding| {
        finding.rule_id == "SUS023" && finding.subject.as_deref() == Some("v_checkout")
    }));
}

#[test]
fn expectation_changes_dirty_verifications_and_decisions() {
    let mut analysis = analysis_with_expectations(vec![expectation(
        "e_checkout",
        ExpectationTarget::Workflow,
        Some("w_checkout"),
    )]);
    analysis.verifications.push(Verification {
        id: "v_checkout".to_owned(),
        expectation_id: "e_checkout".to_owned(),
        status: VerificationStatus::Passed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "manual review".to_owned(),
        source: "human:test".to_owned(),
        evidence: None,
        basis: None,
        detail: "Checked.".to_owned(),
    });
    analysis.decisions.push(Decision {
        id: "d_checkout".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        status: DecisionStatus::Accepted,
        source: "human:test".to_owned(),
        basis: None,
        title: "Accept checkout shape".to_owned(),
        detail: "Accepted.".to_owned(),
    });

    anchor_verification_bases(&mut analysis);
    anchor_decision_bases(&mut analysis);
    analysis.expectations[0].detail = "Changed expectation detail.".to_owned();
    refresh_relationship_findings(&mut analysis);

    assert!(analysis.findings.iter().any(|finding| {
        finding.rule_id == "SUS023" && finding.subject.as_deref() == Some("v_checkout")
    }));
    assert!(analysis.findings.iter().any(|finding| {
        finding.rule_id == "SUS033" && finding.subject.as_deref() == Some("d_checkout")
    }));
}

#[test]
fn linked_work_changes_dirty_verifications_and_decisions() {
    let mut analysis = analysis_with_expectations(vec![expectation(
        "e_checkout",
        ExpectationTarget::Workflow,
        Some("w_checkout"),
    )]);
    analysis.verifications.push(Verification {
        id: "v_checkout".to_owned(),
        expectation_id: "e_checkout".to_owned(),
        status: VerificationStatus::Passed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "manual review".to_owned(),
        source: "human:test".to_owned(),
        evidence: None,
        basis: None,
        detail: "Checked.".to_owned(),
    });
    analysis.decisions.push(Decision {
        id: "d_checkout".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        status: DecisionStatus::Accepted,
        source: "human:test".to_owned(),
        basis: None,
        title: "Accept checkout shape".to_owned(),
        detail: "Accepted.".to_owned(),
    });

    anchor_verification_bases(&mut analysis);
    anchor_decision_bases(&mut analysis);
    let mut linked_work = work(
        "work_checkout",
        ExpectationTarget::Workflow,
        Some("w_checkout"),
    );
    linked_work.expectation_id = Some("e_checkout".to_owned());
    analysis.works.push(linked_work);
    refresh_relationship_findings(&mut analysis);

    assert!(analysis.findings.iter().any(|finding| {
        finding.rule_id == "SUS023" && finding.subject.as_deref() == Some("v_checkout")
    }));
    assert!(analysis.findings.iter().any(|finding| {
        finding.rule_id == "SUS033" && finding.subject.as_deref() == Some("d_checkout")
    }));
}

#[test]
fn symbol_verification_ignores_unrelated_file_changes() {
    let mut analysis = analysis_with_expectations(vec![expectation(
        "e_checkout",
        ExpectationTarget::Symbol,
        Some("s_checkout"),
    )]);
    analysis.verifications.push(Verification {
        id: "v_checkout".to_owned(),
        expectation_id: "e_checkout".to_owned(),
        status: VerificationStatus::Passed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "manual review".to_owned(),
        source: "human:test".to_owned(),
        evidence: Some("review:test".to_owned()),
        basis: None,
        detail: "Checkout behavior matched the expectation.".to_owned(),
    });

    anchor_verification_bases(&mut analysis);
    analysis.files[0].content_hash = Some("unrelated_file_change".to_owned());
    refresh_relationship_findings(&mut analysis);

    assert!(
        !analysis
            .findings
            .iter()
            .any(|finding| finding.rule_id == "SUS023")
    );
}
