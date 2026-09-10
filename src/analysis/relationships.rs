use std::collections::BTreeSet;

use crate::model::{
    ExpectationTarget, Finding, ProjectAnalysis, ReviewAnchor, ReviewThread, Severity,
};

use super::basis::{decision_basis_finding, verification_basis_finding};
use super::findings::{
    expectation_subject_exists, missing_subject_finding, project_subject_finding,
    stale_subject_finding,
};

pub fn refresh_relationship_findings(analysis: &mut ProjectAnalysis) {
    analysis.findings.retain(|finding| {
        !matches!(
            finding.rule_id.as_str(),
            "SUS010"
                | "SUS011"
                | "SUS012"
                | "SUS020"
                | "SUS023"
                | "SUS030"
                | "SUS031"
                | "SUS032"
                | "SUS033"
                | "SUS040"
                | "SUS041"
                | "SUS042"
                | "SUS043"
                | "SUS050"
                | "SUS051"
                | "SUS052"
                | "SUS053"
                | "SUS054"
        )
    });
    add_expectation_relationship_findings(analysis);
    add_verification_relationship_findings(analysis);
    add_decision_relationship_findings(analysis);
    add_work_relationship_findings(analysis);
    add_review_thread_relationship_findings(analysis);
}

fn add_expectation_relationship_findings(analysis: &mut ProjectAnalysis) {
    for expectation in &analysis.expectations {
        match expectation.target {
            ExpectationTarget::Project => {
                if expectation.subject.is_some() {
                    analysis.findings.push(project_subject_finding(
                        "SUS012",
                        "expectation",
                        expectation,
                    ));
                }
            }
            ExpectationTarget::File | ExpectationTarget::Symbol | ExpectationTarget::Workflow => {
                let Some(subject) = expectation.subject.as_deref() else {
                    analysis.findings.push(missing_subject_finding(
                        "SUS010",
                        "Expectation",
                        expectation,
                    ));
                    continue;
                };

                if !expectation_subject_exists(analysis, expectation.target, subject) {
                    analysis.findings.push(stale_subject_finding(
                        "SUS011",
                        "Expectation",
                        expectation,
                        subject,
                    ));
                }
            }
        }
    }
}

fn add_verification_relationship_findings(analysis: &mut ProjectAnalysis) {
    for verification in &analysis.verifications {
        let Some(expectation) = analysis
            .expectations
            .iter()
            .find(|expectation| expectation.id == verification.expectation_id)
        else {
            analysis.findings.push(Finding {
                rule_id: "SUS020".to_owned(),
                source: "susumu:derived".to_owned(),
                severity: Severity::Warning,
                title: "Verification expectation was not found".to_owned(),
                detail: format!(
                    "{} references expectation `{}`, but that expectation is not present in this artifact.",
                    verification.id, verification.expectation_id
                ),
                file_id: None,
                subject: Some(verification.id.clone()),
                location: None,
            });
            continue;
        };

        // A verification that a later record supersedes is no longer relied on,
        // so its changed basis does not need renewed review.
        let superseded = analysis
            .verifications
            .iter()
            .any(|other| other.supersedes.as_deref() == Some(verification.id.as_str()));
        if !superseded
            && let Some(finding) = verification_basis_finding(analysis, verification, expectation)
        {
            analysis.findings.push(finding);
        }
    }
}

fn add_decision_relationship_findings(analysis: &mut ProjectAnalysis) {
    for decision in &analysis.decisions {
        match decision.target {
            ExpectationTarget::Project => {
                if decision.subject.is_some() {
                    analysis
                        .findings
                        .push(project_subject_finding("SUS032", "decision", decision));
                }
            }
            ExpectationTarget::File | ExpectationTarget::Symbol | ExpectationTarget::Workflow => {
                let Some(subject) = decision.subject.as_deref() else {
                    analysis
                        .findings
                        .push(missing_subject_finding("SUS030", "Decision", decision));
                    continue;
                };

                if !expectation_subject_exists(analysis, decision.target, subject) {
                    analysis.findings.push(stale_subject_finding(
                        "SUS031", "Decision", decision, subject,
                    ));
                } else if let Some(finding) = decision_basis_finding(analysis, decision) {
                    analysis.findings.push(finding);
                }
            }
        }
    }
}

fn add_work_relationship_findings(analysis: &mut ProjectAnalysis) {
    for work in &analysis.works {
        match work.target {
            ExpectationTarget::Project => {
                if work.subject.is_some() {
                    analysis
                        .findings
                        .push(project_subject_finding("SUS042", "work record", work));
                }
            }
            ExpectationTarget::File | ExpectationTarget::Symbol | ExpectationTarget::Workflow => {
                let Some(subject) = work.subject.as_deref() else {
                    analysis
                        .findings
                        .push(missing_subject_finding("SUS040", "Work record", work));
                    continue;
                };

                if !expectation_subject_exists(analysis, work.target, subject) {
                    analysis.findings.push(stale_subject_finding(
                        "SUS041",
                        "Work record",
                        work,
                        subject,
                    ));
                }
            }
        }

        if let Some(expectation_id) = work.expectation_id.as_deref()
            && !analysis
                .expectations
                .iter()
                .any(|expectation| expectation.id == expectation_id)
        {
            analysis.findings.push(Finding {
                rule_id: "SUS043".to_owned(),
                source: "susumu:derived".to_owned(),
                severity: Severity::Warning,
                title: "Work expectation was not found".to_owned(),
                detail: format!(
                    "{} references expectation `{expectation_id}`, but that expectation is not present in this artifact.",
                    work.id
                ),
                file_id: None,
                subject: Some(work.id.clone()),
                location: None,
            });
        }
    }
}

fn add_review_thread_relationship_findings(analysis: &mut ProjectAnalysis) {
    let mut findings = Vec::new();
    for review in &analysis.review_threads {
        findings.extend(review_anchor_finding(analysis, review));
        findings.extend(review_subject_finding(analysis, review));
        // A review with a located target but no subject cannot be checked for
        // parent or cycle relationships; the missing-subject finding stands alone.
        if review_missing_required_subject(review) {
            continue;
        }
        findings.extend(review_parent_finding(analysis, review));
        findings.extend(review_cycle_finding(analysis, review));
    }
    analysis.findings.extend(findings);
}

fn review_missing_required_subject(review: &ReviewThread) -> bool {
    matches!(
        review.target,
        ExpectationTarget::File | ExpectationTarget::Symbol | ExpectationTarget::Workflow
    ) && review.subject.is_none()
}

fn review_anchor_finding(analysis: &ProjectAnalysis, review: &ReviewThread) -> Option<Finding> {
    let anchor = review.anchor.as_ref()?;
    if review_anchor_exists(analysis, anchor) {
        return None;
    }
    Some(Finding {
        rule_id: "SUS055".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Review thread anchor was not found".to_owned(),
        detail: format!(
            "{} anchors to `{anchor}`, but that record or source path is not present in this artifact.",
            review.id
        ),
        file_id: None,
        subject: Some(review.id.clone()),
        location: None,
    })
}

fn review_subject_finding(analysis: &ProjectAnalysis, review: &ReviewThread) -> Option<Finding> {
    match review.target {
        ExpectationTarget::Project => review
            .subject
            .is_some()
            .then(|| project_subject_finding("SUS052", "review thread", review)),
        ExpectationTarget::File | ExpectationTarget::Symbol | ExpectationTarget::Workflow => {
            let Some(subject) = review.subject.as_deref() else {
                return Some(missing_subject_finding("SUS050", "Review thread", review));
            };
            (!expectation_subject_exists(analysis, review.target, subject))
                .then(|| stale_subject_finding("SUS051", "Review thread", review, subject))
        }
    }
}

fn review_parent_finding(analysis: &ProjectAnalysis, review: &ReviewThread) -> Option<Finding> {
    let parent = review.parent.as_deref()?;
    if analysis
        .review_threads
        .iter()
        .any(|candidate| candidate.id == parent)
    {
        return None;
    }
    Some(Finding {
        rule_id: "SUS053".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Review thread parent was not found".to_owned(),
        detail: format!(
            "{} replies to review thread `{parent}`, but that parent is not present in this artifact.",
            review.id
        ),
        file_id: None,
        subject: Some(review.id.clone()),
        location: None,
    })
}

fn review_cycle_finding(analysis: &ProjectAnalysis, review: &ReviewThread) -> Option<Finding> {
    if !review_thread_has_cycle(analysis, &review.id) {
        return None;
    }
    Some(Finding {
        rule_id: "SUS054".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Review thread parent cycle detected".to_owned(),
        detail: format!(
            "{} belongs to a review reply cycle. Thread parents must eventually terminate at a root record.",
            review.id
        ),
        file_id: None,
        subject: Some(review.id.clone()),
        location: None,
    })
}

fn review_anchor_exists(analysis: &ProjectAnalysis, anchor: &ReviewAnchor) -> bool {
    match anchor {
        ReviewAnchor::Expectation(id) => {
            analysis.expectations.iter().any(|record| record.id == *id)
        }
        ReviewAnchor::Verification(id) => {
            analysis.verifications.iter().any(|record| record.id == *id)
        }
        ReviewAnchor::Work(id) => analysis.works.iter().any(|record| record.id == *id),
        ReviewAnchor::Decision(id) => analysis.decisions.iter().any(|record| record.id == *id),
        ReviewAnchor::Finding(id) => analysis.findings.iter().any(|record| record.rule_id == *id),
        ReviewAnchor::Source { path, .. } => analysis.files.iter().any(|file| file.path == *path),
    }
}

fn review_thread_has_cycle(analysis: &ProjectAnalysis, start: &str) -> bool {
    let mut visited = BTreeSet::new();
    let mut current = Some(start);
    while let Some(id) = current {
        if !visited.insert(id) {
            return true;
        }
        current = analysis
            .review_threads
            .iter()
            .find(|review| review.id == id)
            .and_then(|review| review.parent.as_deref());
    }
    false
}
