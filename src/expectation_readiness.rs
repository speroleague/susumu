use susumu::model::{
    Expectation, ExpectationStatus, ExpectationTarget, ProjectAnalysis, VerificationStatus,
};

use crate::review_types::{
    ExpectationReadiness, ExpectationSupport, ExpectationVerificationSupport,
    verification_evidence_posture,
};

pub(crate) fn expectation_support(analysis: &ProjectAnalysis) -> Vec<ExpectationSupport> {
    let mut support = analysis
        .expectations
        .iter()
        .map(|expectation| {
            let target_observed = expectation_target_observed(analysis, expectation);
            let verification = expectation_verification_support(analysis, &expectation.id);
            let evidence_posture = expectation_evidence_posture(analysis, &expectation.id);
            let work = expectation_work_support_count(analysis, expectation);
            let decisions = expectation_decision_support_count(analysis, expectation);
            let findings = expectation_finding_support_count(analysis, expectation);
            let (support_status, reasons) = expectation_support_status(
                expectation,
                target_observed,
                &verification,
                work,
                decisions,
                findings,
            );
            ExpectationSupport {
                expectation_id: expectation.id.clone(),
                title: expectation.title.clone(),
                target: expectation.target.to_string(),
                subject: expectation.subject.clone(),
                target_observed,
                verification,
                work,
                decisions,
                findings,
                support_status,
                evidence_posture,
                reasons,
            }
        })
        .collect::<Vec<_>>();
    support.sort_by(|left, right| left.expectation_id.cmp(&right.expectation_id));
    support
}

pub(crate) fn expectation_readiness(
    analysis: &ProjectAnalysis,
    support: &[ExpectationSupport],
) -> Vec<ExpectationReadiness> {
    let mut readiness = support
        .iter()
        .map(|support| {
            let (bucket, label) = expectation_readiness_bucket(support);
            ExpectationReadiness {
                expectation_id: support.expectation_id.clone(),
                title: support.title.clone(),
                target: support.target.clone(),
                subject: support.subject.clone(),
                bucket: bucket.to_owned(),
                label: label.to_owned(),
                support_status: support.support_status.clone(),
                evidence_posture: support.evidence_posture.clone(),
                next_action: expectation_readiness_next_action(analysis, support),
            }
        })
        .collect::<Vec<_>>();
    readiness.sort_by(|left, right| {
        readiness_bucket_rank(&left.bucket)
            .cmp(&readiness_bucket_rank(&right.bucket))
            .then_with(|| left.expectation_id.cmp(&right.expectation_id))
    });
    readiness
}

fn expectation_evidence_posture(analysis: &ProjectAnalysis, expectation_id: &str) -> String {
    let mut postures = analysis
        .verifications
        .iter()
        .filter(|verification| verification.expectation_id == expectation_id)
        .map(verification_evidence_posture)
        .collect::<Vec<_>>();
    postures.sort_unstable();
    postures.dedup();
    match postures.as_slice() {
        [] => "none".to_owned(),
        [posture] => (*posture).to_owned(),
        _ => "mixed".to_owned(),
    }
}

fn expectation_readiness_bucket(support: &ExpectationSupport) -> (&'static str, &'static str) {
    if support.verification.failed > 0 {
        ("failed_verification", "Failed verification")
    } else if !support.target_observed {
        ("missing_target", "Missing target")
    } else if support.verification.passed > 0 {
        ("verified", "Verified")
    } else if support.work > 0 {
        ("needs_verification", "Has work, needs verification")
    } else {
        ("needs_work", "No linked work yet")
    }
}

const fn readiness_bucket_rank(bucket: &str) -> u8 {
    match bucket.as_bytes() {
        b"failed_verification" => 0,
        b"missing_target" => 1,
        b"needs_verification" => 2,
        b"needs_work" => 3,
        b"verified" => 4,
        _ => 5,
    }
}

fn expectation_readiness_next_action(
    analysis: &ProjectAnalysis,
    support: &ExpectationSupport,
) -> String {
    if support.verification.failed > 0 {
        return "Review the failed verification before relying on this expectation.".to_owned();
    }
    if !support.target_observed {
        return "Find or reconnect the target this expectation is about.".to_owned();
    }
    if support.verification.passed > 0 {
        return "Verified: ready for review or business confidence.".to_owned();
    }
    if support.work == 0 {
        return format!(
            "Connect work with susumu git or susumu git link <commit> {}.",
            support.expectation_id
        );
    }
    let verification_count = support.verification.passed
        + support.verification.failed
        + support.verification.inconclusive;
    if verification_count == 0 {
        return format!(
            "Record verification with susumu verify {} --passed --method \"<check>\".",
            support.expectation_id
        );
    }
    if support.verification.inconclusive > 0 && support.verification.passed == 0 {
        return "Resolve the inconclusive verification evidence.".to_owned();
    }
    let title = analysis
        .expectations
        .iter()
        .find(|expectation| expectation.id == support.expectation_id)
        .map_or("this expectation", |expectation| expectation.title.as_str());
    format!("Review `{title}` and decide whether more verification is needed.")
}

fn expectation_target_observed(analysis: &ProjectAnalysis, expectation: &Expectation) -> bool {
    match expectation.target {
        ExpectationTarget::Project => expectation.subject.is_none(),
        ExpectationTarget::File => expectation
            .subject
            .as_deref()
            .is_some_and(|id| analysis.files.iter().any(|file| file.id == id)),
        ExpectationTarget::Symbol => expectation
            .subject
            .as_deref()
            .is_some_and(|id| analysis.symbols.iter().any(|symbol| symbol.id == id)),
        ExpectationTarget::Workflow => expectation
            .subject
            .as_deref()
            .is_some_and(|id| analysis.workflows.iter().any(|workflow| workflow.id == id)),
    }
}

fn expectation_verification_support(
    analysis: &ProjectAnalysis,
    expectation_id: &str,
) -> ExpectationVerificationSupport {
    let mut support = ExpectationVerificationSupport {
        passed: 0,
        failed: 0,
        inconclusive: 0,
    };
    for verification in analysis
        .verifications
        .iter()
        .filter(|verification| verification.expectation_id == expectation_id)
    {
        match verification.status {
            VerificationStatus::Passed => support.passed += 1,
            VerificationStatus::Failed => support.failed += 1,
            VerificationStatus::Inconclusive => support.inconclusive += 1,
        }
    }
    support
}

fn expectation_work_support_count(analysis: &ProjectAnalysis, expectation: &Expectation) -> usize {
    analysis
        .works
        .iter()
        .filter(|work| {
            if let Some(expectation_id) = work.expectation_id.as_deref() {
                expectation_id == expectation.id
            } else {
                work.target == expectation.target && work.subject == expectation.subject
            }
        })
        .count()
}

fn expectation_decision_support_count(
    analysis: &ProjectAnalysis,
    expectation: &Expectation,
) -> usize {
    analysis
        .decisions
        .iter()
        .filter(|decision| {
            decision.target == expectation.target && decision.subject == expectation.subject
        })
        .count()
}

fn expectation_finding_support_count(
    analysis: &ProjectAnalysis,
    expectation: &Expectation,
) -> usize {
    analysis
        .findings
        .iter()
        .filter(|finding| {
            expectation
                .subject
                .as_deref()
                .is_some_and(|subject| finding.subject.as_deref() == Some(subject))
        })
        .count()
}

fn expectation_support_status(
    expectation: &Expectation,
    target_observed: bool,
    verification: &ExpectationVerificationSupport,
    work: usize,
    decisions: usize,
    findings: usize,
) -> (String, Vec<String>) {
    let mut reasons = Vec::new();
    if target_observed {
        reasons.push("target observed".to_owned());
    } else {
        reasons.push("target not observed".to_owned());
    }
    if verification.passed > 0 {
        reasons.push(format!(
            "{} passed verification record(s)",
            verification.passed
        ));
    }
    if verification.failed > 0 {
        reasons.push(format!(
            "{} failed verification record(s)",
            verification.failed
        ));
    }
    if verification.inconclusive > 0 {
        reasons.push(format!(
            "{} inconclusive verification record(s)",
            verification.inconclusive
        ));
    }
    if work > 0 {
        reasons.push(format!("{work} linked work record(s)"));
    }
    if decisions > 0 {
        reasons.push(format!("{decisions} linked decision record(s)"));
    }
    if findings > 0 {
        reasons.push(format!("{findings} linked finding(s)"));
    }
    if verification.passed + verification.failed + verification.inconclusive == 0 {
        reasons.push("no verification records linked".to_owned());
    }

    let status = if matches!(expectation.status, ExpectationStatus::Superseded) {
        "superseded"
    } else if !target_observed {
        "missing_target"
    } else if verification.failed > 0 {
        "failed_verification"
    } else if verification.passed > 0 {
        "verified"
    } else if verification.inconclusive > 0 {
        "inconclusive"
    } else if work + decisions + findings > 0 {
        "partially_supported"
    } else {
        "needs_support"
    };
    (status.to_owned(), reasons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use susumu::model::{Expectation, SCHEMA_VERSION, Work, WorkKind, WorkStatus};

    #[test]
    fn readiness_marks_work_without_verification_as_needing_verification() {
        let mut artifact = ProjectAnalysis {
            schema_version: SCHEMA_VERSION,
            project_name: "fixture".to_owned(),
            root: ".".to_owned(),
            generated_unix_seconds: 0,
            files: Vec::new(),
            symbols: Vec::new(),
            dependencies: Vec::new(),
            workflows: Vec::new(),
            workflow_priorities: Vec::new(),
            flows: Vec::new(),
            expectations: vec![Expectation {
                id: "e_project".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:test".to_owned(),
                title: "Project expectation".to_owned(),
                detail: "The project expectation should be supported.".to_owned(),
            }],
            verifications: Vec::new(),
            decisions: Vec::new(),
            works: Vec::new(),
            findings: Vec::new(),
        };
        artifact.works.push(Work {
            id: "wk_project".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            expectation_id: Some("e_project".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "agent:test".to_owned(),
            evidence: Some("commit:abc123".to_owned()),
            title: "Implement project expectation".to_owned(),
            detail: "Work exists but verification does not.".to_owned(),
        });

        let support = expectation_support(&artifact);
        let readiness = expectation_readiness(&artifact, &support);

        assert_eq!(support[0].support_status, "partially_supported");
        assert_eq!(support[0].work, 1);
        assert_eq!(readiness[0].bucket, "needs_verification");
        assert!(readiness[0].next_action.contains("susumu verify e_project"));
    }
}
