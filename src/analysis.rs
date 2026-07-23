use sha2::{Digest, Sha256};

use crate::model::{
    Decision, Expectation, ExpectationTarget, Finding, ProjectAnalysis, Severity, Verification,
    Work,
};

mod findings;
mod priorities;

use findings::{
    expectation_subject_exists, missing_subject_finding, project_subject_finding,
    stale_subject_finding,
};

pub(crate) fn add_findings(analysis: &mut ProjectAnalysis) {
    crate::derived_findings::add_static_findings(analysis);
    refresh_relationship_findings(analysis);
    refresh_workflow_priorities(analysis);
}

pub fn refresh_expectation_findings(analysis: &mut ProjectAnalysis) {
    refresh_relationship_findings(analysis);
}

pub fn refresh_derived_analysis(analysis: &mut ProjectAnalysis) {
    refresh_relationship_findings(analysis);
    refresh_workflow_priorities(analysis);
}

pub use priorities::refresh_workflow_priorities;

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
        )
    });
    add_expectation_relationship_findings(analysis);
    add_verification_relationship_findings(analysis);
    add_decision_relationship_findings(analysis);
    add_work_relationship_findings(analysis);
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

        if let Some(finding) = verification_basis_finding(analysis, verification, expectation) {
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

/// Records the current review basis on decisions that do not yet carry one.
/// Existing bases are preserved so later scans can detect changed review
/// evidence.
pub fn anchor_decision_bases(analysis: &mut ProjectAnalysis) {
    let fingerprints = analysis
        .decisions
        .iter()
        .map(|decision| {
            decision
                .basis
                .is_none()
                .then(|| current_decision_basis(analysis, decision))
        })
        .collect::<Vec<_>>();

    for (decision, fingerprint) in analysis.decisions.iter_mut().zip(fingerprints) {
        if decision.basis.is_none()
            && let Some(Some(fingerprint)) = fingerprint
        {
            decision.basis = Some(fingerprint);
        }
    }
}

/// Records the current review basis on verifications that do not yet carry one.
/// Existing bases are preserved so later scans can detect when a check result
/// may need to be rerun or reviewed.
pub fn anchor_verification_bases(analysis: &mut ProjectAnalysis) {
    let fingerprints = analysis
        .verifications
        .iter()
        .map(|verification| {
            verification.basis.is_none().then(|| {
                verification_expectation(analysis, verification).and_then(|expectation| {
                    current_verification_basis(analysis, verification, expectation)
                })
            })
        })
        .collect::<Vec<_>>();

    for (verification, fingerprint) in analysis.verifications.iter_mut().zip(fingerprints) {
        if verification.basis.is_none()
            && let Some(Some(fingerprint)) = fingerprint
        {
            verification.basis = Some(fingerprint);
        }
    }
}

fn decision_basis_finding(analysis: &ProjectAnalysis, decision: &Decision) -> Option<Finding> {
    let basis = decision.basis.as_deref()?;
    let current = current_decision_basis(analysis, decision)?;
    (!basis_matches_current(
        basis,
        current.as_str(),
        analysis,
        decision.target,
        decision.subject.as_deref(),
    ))
    .then(|| Finding {
        rule_id: "SUS033".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Decision review evidence changed".to_owned(),
        detail: format!(
            "{} targets {} with basis `{basis}`, but the current review basis is `{current}`. Target code, expectations, or linked work may have changed.",
            decision.id, decision.target
        ),
        file_id: decision_file_id(analysis, decision),
        subject: Some(decision.id.clone()),
        location: decision_location(analysis, decision),
    })
}

fn verification_basis_finding(
    analysis: &ProjectAnalysis,
    verification: &Verification,
    expectation: &Expectation,
) -> Option<Finding> {
    let basis = verification.basis.as_deref()?;
    let current = current_verification_basis(analysis, verification, expectation)?;
    (!basis_matches_current(
        basis,
        current.as_str(),
        analysis,
        expectation.target,
        expectation.subject.as_deref(),
    ))
    .then(|| Finding {
        rule_id: "SUS023".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Verification evidence changed".to_owned(),
        detail: format!(
            "{} checks expectation `{}` with basis `{basis}`, but the current review basis is `{current}`. Target code, expectation details, or linked work may have changed.",
            verification.id, verification.expectation_id
        ),
        file_id: target_file_id(analysis, expectation.target, expectation.subject.as_deref()),
        subject: Some(verification.id.clone()),
        location: target_location(analysis, expectation.target, expectation.subject.as_deref()),
    })
}

const REVIEW_BASIS_PREFIX: &str = "review-v2:";

fn current_verification_basis(
    analysis: &ProjectAnalysis,
    verification: &Verification,
    expectation: &Expectation,
) -> Option<String> {
    review_basis(
        analysis,
        expectation.target,
        expectation.subject.as_deref(),
        std::iter::once(expectation),
        analysis.works.iter().filter(|work| {
            work.expectation_id.as_deref() == Some(expectation.id.as_str())
                || same_target(
                    work.target,
                    work.subject.as_deref(),
                    expectation.target,
                    expectation.subject.as_deref(),
                )
        }),
        Some(verification.id.as_str()),
    )
}

fn current_decision_basis(analysis: &ProjectAnalysis, decision: &Decision) -> Option<String> {
    let expectations = analysis.expectations.iter().filter(|expectation| {
        same_target(
            expectation.target,
            expectation.subject.as_deref(),
            decision.target,
            decision.subject.as_deref(),
        )
    });
    let expectation_ids = analysis
        .expectations
        .iter()
        .filter(|expectation| {
            same_target(
                expectation.target,
                expectation.subject.as_deref(),
                decision.target,
                decision.subject.as_deref(),
            )
        })
        .map(|expectation| expectation.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let works = analysis.works.iter().filter(|work| {
        expectation_ids.contains(work.expectation_id.as_deref().unwrap_or_default())
            || same_target(
                work.target,
                work.subject.as_deref(),
                decision.target,
                decision.subject.as_deref(),
            )
    });
    review_basis(
        analysis,
        decision.target,
        decision.subject.as_deref(),
        expectations,
        works,
        Some(decision.id.as_str()),
    )
}

fn review_basis<'a>(
    analysis: &ProjectAnalysis,
    target: ExpectationTarget,
    subject: Option<&str>,
    expectations: impl Iterator<Item = &'a Expectation>,
    works: impl Iterator<Item = &'a Work>,
    record_id: Option<&str>,
) -> Option<String> {
    let target_hash = target_fingerprint(analysis, target, subject)?;
    let mut parts = vec![format!("target={target_hash}")];
    let mut expectation_parts = expectations
        .map(|expectation| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}",
                expectation.id,
                expectation.target,
                expectation.subject.as_deref().unwrap_or("-"),
                expectation.status,
                expectation.source,
                expectation.title,
                expectation.detail
            )
        })
        .collect::<Vec<_>>();
    expectation_parts.sort_unstable();
    parts.extend(expectation_parts);

    let mut work_parts = works
        .map(|work| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                work.id,
                work.target,
                work.subject.as_deref().unwrap_or("-"),
                work.expectation_id.as_deref().unwrap_or("-"),
                work.kind,
                work.status,
                work.source,
                work.evidence.as_deref().unwrap_or("-"),
                work.title,
                work.detail
            )
        })
        .collect::<Vec<_>>();
    work_parts.sort_unstable();
    parts.extend(work_parts);
    if let Some(record_id) = record_id {
        parts.push(format!("record={record_id}"));
    }

    let references = parts.iter().map(String::as_str).collect::<Vec<_>>();
    Some(format!("{REVIEW_BASIS_PREFIX}{}", hash_parts(&references)))
}

fn same_target(
    left_target: ExpectationTarget,
    left_subject: Option<&str>,
    right_target: ExpectationTarget,
    right_subject: Option<&str>,
) -> bool {
    left_target == right_target && left_subject == right_subject
}

fn basis_matches_current(
    stored: &str,
    current: &str,
    analysis: &ProjectAnalysis,
    target: ExpectationTarget,
    subject: Option<&str>,
) -> bool {
    stored == current
        || (!stored.starts_with(REVIEW_BASIS_PREFIX)
            && target_fingerprint(analysis, target, subject).as_deref() == Some(stored))
}

fn verification_expectation<'a>(
    analysis: &'a ProjectAnalysis,
    verification: &Verification,
) -> Option<&'a Expectation> {
    analysis
        .expectations
        .iter()
        .find(|expectation| expectation.id == verification.expectation_id)
}

fn target_fingerprint(
    analysis: &ProjectAnalysis,
    target: ExpectationTarget,
    subject: Option<&str>,
) -> Option<String> {
    match target {
        ExpectationTarget::Project => project_fingerprint(analysis),
        ExpectationTarget::File => subject
            .and_then(|id| analysis.files.iter().find(|file| file.id == id))
            .and_then(|file| file.content_hash.clone()),
        ExpectationTarget::Symbol => subject
            .and_then(|id| analysis.symbols.iter().find(|symbol| symbol.id == id))
            .and_then(|symbol| {
                symbol
                    .content_hash
                    .clone()
                    .or_else(|| located_fingerprint(analysis, &symbol.file_id, &symbol.location))
            }),
        ExpectationTarget::Workflow => subject
            .and_then(|id| analysis.workflows.iter().find(|workflow| workflow.id == id))
            .and_then(|workflow| {
                workflow
                    .entry_symbol
                    .as_deref()
                    .and_then(|symbol_id| {
                        analysis
                            .symbols
                            .iter()
                            .find(|symbol| symbol.id == symbol_id)
                            .and_then(|symbol| symbol.content_hash.clone())
                    })
                    .or_else(|| {
                        located_fingerprint(analysis, &workflow.file_id, &workflow.location)
                    })
            }),
    }
}

fn project_fingerprint(analysis: &ProjectAnalysis) -> Option<String> {
    let mut hashes = analysis
        .files
        .iter()
        .map(|file| file.content_hash.as_deref())
        .collect::<Option<Vec<_>>>()?;
    hashes.sort_unstable();
    Some(hash_parts(&hashes))
}

fn located_fingerprint(
    analysis: &ProjectAnalysis,
    file_id: &str,
    location: &crate::model::Location,
) -> Option<String> {
    let file = analysis.files.iter().find(|file| file.id == file_id)?;
    let hash = file.content_hash.as_deref()?;
    Some(hash_parts(&[
        file_id,
        hash,
        &location.start_token(),
        &location.end_token(),
    ]))
}

fn hash_parts(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part.as_bytes());
        digest.update([0]);
    }
    hex_prefix(&digest.finalize(), 16)
}

fn hex_prefix(bytes: &[u8], count: usize) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(count * 2);
    for byte in bytes.iter().take(count) {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn target_file_id(
    analysis: &ProjectAnalysis,
    target: ExpectationTarget,
    subject: Option<&str>,
) -> Option<String> {
    match target {
        ExpectationTarget::File => subject.map(ToOwned::to_owned),
        ExpectationTarget::Symbol => subject
            .and_then(|id| analysis.symbols.iter().find(|symbol| symbol.id == id))
            .map(|symbol| symbol.file_id.clone()),
        ExpectationTarget::Workflow => subject
            .and_then(|id| analysis.workflows.iter().find(|workflow| workflow.id == id))
            .map(|workflow| workflow.file_id.clone()),
        ExpectationTarget::Project => None,
    }
}

fn target_location(
    analysis: &ProjectAnalysis,
    target: ExpectationTarget,
    subject: Option<&str>,
) -> Option<crate::model::Location> {
    match target {
        ExpectationTarget::Symbol => subject
            .and_then(|id| analysis.symbols.iter().find(|symbol| symbol.id == id))
            .map(|symbol| symbol.location.clone()),
        ExpectationTarget::Workflow => subject
            .and_then(|id| analysis.workflows.iter().find(|workflow| workflow.id == id))
            .map(|workflow| workflow.location.clone()),
        ExpectationTarget::Project | ExpectationTarget::File => None,
    }
}

fn decision_file_id(analysis: &ProjectAnalysis, decision: &Decision) -> Option<String> {
    target_file_id(analysis, decision.target, decision.subject.as_deref())
}

fn decision_location(
    analysis: &ProjectAnalysis,
    decision: &Decision,
) -> Option<crate::model::Location> {
    target_location(analysis, decision.target, decision.subject.as_deref())
}

#[cfg(test)]
mod tests;
