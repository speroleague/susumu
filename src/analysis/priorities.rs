use crate::model::{
    Confidence, ExpectationStatus, ExpectationTarget, ProjectAnalysis, VerificationStatus,
    Workflow, WorkflowPriority,
};

pub fn refresh_workflow_priorities(analysis: &mut ProjectAnalysis) {
    let mut priorities = analysis
        .workflows
        .iter()
        .map(|workflow| priority_for_workflow(analysis, workflow))
        .collect::<Vec<_>>();

    priorities.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.workflow_id.cmp(&right.workflow_id))
    });
    analysis.workflow_priorities = priorities;
}

fn priority_for_workflow(analysis: &ProjectAnalysis, workflow: &Workflow) -> WorkflowPriority {
    let mut score = 10_u32;
    let mut reasons = vec!["workflow trigger observed".to_owned()];

    add_observation_priority(workflow, &mut score, &mut reasons);
    add_call_edge_priority(analysis, workflow, &mut score, &mut reasons);
    add_expectation_priority(analysis, workflow, &mut score, &mut reasons);
    add_finding_priority(analysis, workflow, &mut score, &mut reasons);

    WorkflowPriority {
        workflow_id: workflow.id.clone(),
        source: "susumu:derived".to_owned(),
        score,
        detail: reasons.join("; "),
    }
}

fn add_observation_priority(workflow: &Workflow, score: &mut u32, reasons: &mut Vec<String>) {
    if workflow.entry_symbol.is_some() {
        *score += 25;
        reasons.push("handler symbol resolved".to_owned());
    }

    if workflow.kind.to_string() == "http" {
        *score += 20;
        reasons.push("HTTP route observed".to_owned());
    }
}

fn add_call_edge_priority(
    analysis: &ProjectAnalysis,
    workflow: &Workflow,
    score: &mut u32,
    reasons: &mut Vec<String>,
) {
    let Some(entry_symbol) = workflow.entry_symbol.as_deref() else {
        return;
    };

    let outgoing = analysis
        .flows
        .iter()
        .filter(|flow| flow.from == entry_symbol)
        .count();
    if outgoing >= 5 {
        *score += 10;
        reasons.push(format!("{outgoing} outgoing call edges observed"));
    }

    let gaps = analysis
        .flows
        .iter()
        .filter(|flow| {
            flow.from == entry_symbol
                && flow.to.is_none()
                && flow.confidence != Confidence::External
        })
        .count();
    if gaps > 0 {
        *score += 8;
        reasons.push(unresolved_call_edge_reason(gaps));
    }
}

fn add_expectation_priority(
    analysis: &ProjectAnalysis,
    workflow: &Workflow,
    score: &mut u32,
    reasons: &mut Vec<String>,
) {
    for expectation in analysis.expectations.iter().filter(|expectation| {
        expectation.target == ExpectationTarget::Workflow
            && expectation.subject.as_deref() == Some(workflow.id.as_str())
    }) {
        match expectation.status {
            ExpectationStatus::Accepted => {
                *score += 20;
                reasons.push("accepted expectation linked".to_owned());
            }
            ExpectationStatus::Proposed => {
                *score += 10;
                reasons.push("proposed expectation linked".to_owned());
            }
            ExpectationStatus::Superseded => {}
        }

        for verification in analysis
            .verifications
            .iter()
            .filter(|verification| verification.expectation_id == expectation.id)
        {
            match verification.status {
                VerificationStatus::Failed => {
                    *score += 25;
                    reasons.push("failed verification linked".to_owned());
                }
                VerificationStatus::Inconclusive => {
                    *score += 12;
                    reasons.push("inconclusive verification linked".to_owned());
                }
                VerificationStatus::Passed => {
                    *score += 4;
                    reasons.push("passed verification linked".to_owned());
                }
            }
        }
    }
}

fn add_finding_priority(
    analysis: &ProjectAnalysis,
    workflow: &Workflow,
    score: &mut u32,
    reasons: &mut Vec<String>,
) {
    let related_findings = related_finding_count(analysis, workflow);
    if related_findings > 0 {
        *score += 6 * u32::try_from(related_findings).unwrap_or(u32::MAX);
        reasons.push(format!("{related_findings} linked findings"));
    }
}

fn related_finding_count(analysis: &ProjectAnalysis, workflow: &Workflow) -> usize {
    analysis
        .findings
        .iter()
        .filter(|finding| {
            finding.file_id.as_deref() == Some(workflow.file_id.as_str())
                || workflow
                    .entry_symbol
                    .as_deref()
                    .is_some_and(|entry| finding.subject.as_deref() == Some(entry))
                || finding.subject.as_deref() == Some(workflow.id.as_str())
        })
        .count()
}

fn unresolved_call_edge_reason(gaps: usize) -> String {
    let label = if gaps == 1 { "edge" } else { "edges" };
    format!("{gaps} unresolved outgoing call {label}")
}
