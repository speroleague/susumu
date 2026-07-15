use std::collections::{HashMap, HashSet};

use sha2::{Digest, Sha256};

use crate::model::{
    Confidence, Decision, Expectation, ExpectationStatus, ExpectationTarget, Finding,
    ProjectAnalysis, Severity, Verification, VerificationStatus, Work, WorkflowPriority,
};

const LARGE_FILE_LINES: usize = 600;
const LARGE_SYMBOL_LINES: usize = 80;
const HIGH_FAN_OUT: usize = 8;

pub(crate) fn add_findings(analysis: &mut ProjectAnalysis) {
    for file in &analysis.files {
        if file.lines > LARGE_FILE_LINES {
            analysis.findings.push(Finding {
                rule_id: "SUS001".to_owned(),
                source: "susumu:derived".to_owned(),
                severity: Severity::Warning,
                title: "Large source file".to_owned(),
                detail: format!(
                    "Observed {} with {} lines. Large files can reduce workflow and ownership clarity.",
                    file.path, file.lines
                ),
                file_id: Some(file.id.clone()),
                subject: None,
                location: None,
            });
        }
    }

    for symbol in &analysis.symbols {
        if symbol.name != "<module>" && symbol.location.line_span() > LARGE_SYMBOL_LINES {
            analysis.findings.push(Finding {
                rule_id: "SUS002".to_owned(),
                source: "susumu:derived".to_owned(),
                severity: Severity::Warning,
                title: "Long workflow unit".to_owned(),
                detail: format!(
                    "Observed {} spanning {} lines. Long units can reduce independent workflow reviewability.",
                    symbol.name,
                    symbol.location.line_span()
                ),
                file_id: Some(symbol.file_id.clone()),
                subject: Some(symbol.id.clone()),
                location: Some(symbol.location.clone()),
            });
        }
    }

    let mut fan_out: HashMap<&str, HashSet<&str>> = HashMap::new();
    for flow in &analysis.flows {
        if let Some(to) = flow.to.as_deref() {
            fan_out.entry(&flow.from).or_default().insert(to);
        }
    }
    for (symbol_id, targets) in fan_out {
        if targets.len() >= HIGH_FAN_OUT
            && let Some(symbol) = analysis
                .symbols
                .iter()
                .find(|symbol| symbol.id == symbol_id)
        {
            analysis.findings.push(Finding {
                rule_id: "SUS003".to_owned(),
                source: "susumu:derived".to_owned(),
                severity: Severity::Warning,
                title: "High fan-out".to_owned(),
                detail: format!(
                    "Observed {} coordinating {} internal units. High fan-out marks a code-change attention point.",
                    symbol.name,
                    targets.len()
                ),
                file_id: Some(symbol.file_id.clone()),
                subject: Some(symbol.id.clone()),
                location: Some(symbol.location.clone()),
            });
        }
    }

    let ambiguous = analysis
        .flows
        .iter()
        .filter(|flow| flow.confidence == Confidence::Ambiguous)
        .count();
    if ambiguous > 0 {
        analysis.findings.push(Finding {
            rule_id: "SUS004".to_owned(),
            source: "susumu:derived".to_owned(),
            severity: Severity::Info,
            title: "Ambiguous call targets".to_owned(),
            detail: format!(
                "{ambiguous} calls matched multiple symbols. Targets remain unresolved; no target was selected."
            ),
            file_id: None,
            subject: None,
            location: None,
        });
    }

    add_cycle_findings(analysis);
    refresh_relationship_findings(analysis);
    refresh_workflow_priorities(analysis);
}

fn add_cycle_findings(analysis: &mut ProjectAnalysis) {
    for cycle in find_cycles(analysis) {
        let names = cycle
            .iter()
            .filter_map(|id| analysis.symbols.iter().find(|symbol| &symbol.id == id))
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>();
        if let Some(first) = cycle
            .first()
            .and_then(|id| analysis.symbols.iter().find(|symbol| &symbol.id == id))
        {
            analysis.findings.push(Finding {
                rule_id: "SUS005".to_owned(),
                source: "susumu:derived".to_owned(),
                severity: Severity::Warning,
                title: "Call cycle".to_owned(),
                detail: format!("Observed call cycle: {}", names.join(" -> ")),
                file_id: Some(first.file_id.clone()),
                subject: Some(first.id.clone()),
                location: Some(first.location.clone()),
            });
        }
    }
}

pub fn refresh_expectation_findings(analysis: &mut ProjectAnalysis) {
    refresh_relationship_findings(analysis);
}

pub fn refresh_derived_analysis(analysis: &mut ProjectAnalysis) {
    refresh_relationship_findings(analysis);
    refresh_workflow_priorities(analysis);
}

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

/// Records the current target fingerprint on decisions that do not yet carry a
/// basis. Existing bases are preserved so later scans can detect changed review
/// evidence.
pub fn anchor_decision_bases(analysis: &mut ProjectAnalysis) {
    let fingerprints = analysis
        .decisions
        .iter()
        .map(|decision| {
            decision
                .basis
                .is_none()
                .then(|| target_fingerprint(analysis, decision.target, decision.subject.as_deref()))
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

/// Records the current expectation target fingerprint on verifications that do
/// not yet carry a basis. Existing bases are preserved so later scans can
/// detect when a check result may need to be rerun or reviewed.
pub fn anchor_verification_bases(analysis: &mut ProjectAnalysis) {
    let fingerprints = analysis
        .verifications
        .iter()
        .map(|verification| {
            verification.basis.is_none().then(|| {
                verification_expectation(analysis, verification).and_then(|expectation| {
                    target_fingerprint(analysis, expectation.target, expectation.subject.as_deref())
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
    let current = target_fingerprint(analysis, decision.target, decision.subject.as_deref())?;
    (basis != current).then(|| Finding {
        rule_id: "SUS033".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Decision review evidence changed".to_owned(),
        detail: format!(
            "{} targets {} with basis `{basis}`, but current evidence fingerprint is `{current}`.",
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
    let current = target_fingerprint(analysis, expectation.target, expectation.subject.as_deref())?;
    (basis != current).then(|| Finding {
        rule_id: "SUS023".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Verification evidence changed".to_owned(),
        detail: format!(
            "{} checks expectation `{}` with basis `{basis}`, but current target evidence fingerprint is `{current}`.",
            verification.id, verification.expectation_id
        ),
        file_id: target_file_id(analysis, expectation.target, expectation.subject.as_deref()),
        subject: Some(verification.id.clone()),
        location: target_location(analysis, expectation.target, expectation.subject.as_deref()),
    })
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
            .and_then(|symbol| located_fingerprint(analysis, &symbol.file_id, &symbol.location)),
        ExpectationTarget::Workflow => subject
            .and_then(|id| analysis.workflows.iter().find(|workflow| workflow.id == id))
            .and_then(|workflow| {
                located_fingerprint(analysis, &workflow.file_id, &workflow.location)
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

trait TargetedRecord {
    fn id(&self) -> &str;
    fn target(&self) -> ExpectationTarget;
}

impl TargetedRecord for Expectation {
    fn id(&self) -> &str {
        &self.id
    }

    fn target(&self) -> ExpectationTarget {
        self.target
    }
}

impl TargetedRecord for Decision {
    fn id(&self) -> &str {
        &self.id
    }

    fn target(&self) -> ExpectationTarget {
        self.target
    }
}

impl TargetedRecord for Work {
    fn id(&self) -> &str {
        &self.id
    }

    fn target(&self) -> ExpectationTarget {
        self.target
    }
}

fn project_subject_finding(rule_id: &str, noun: &str, record: &impl TargetedRecord) -> Finding {
    Finding {
        rule_id: rule_id.to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Info,
        title: format!("Project {noun} has a subject"),
        detail: format!(
            "{} is project-wide and has a subject id. Expected subject is `-`.",
            record.id()
        ),
        file_id: None,
        subject: Some(record.id().to_owned()),
        location: None,
    }
}

fn missing_subject_finding(rule_id: &str, noun: &str, record: &impl TargetedRecord) -> Finding {
    Finding {
        rule_id: rule_id.to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: format!("{noun} is missing a target id"),
        detail: format!(
            "{} targets {} without a subject id.",
            record.id(),
            record.target()
        ),
        file_id: None,
        subject: Some(record.id().to_owned()),
        location: None,
    }
}

fn stale_subject_finding(
    rule_id: &str,
    noun: &str,
    record: &impl TargetedRecord,
    subject: &str,
) -> Finding {
    Finding {
        rule_id: rule_id.to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: format!("{noun} target was not found"),
        detail: format!(
            "{} targets {} `{subject}`, but that id is not present in this artifact.",
            record.id(),
            record.target()
        ),
        file_id: None,
        subject: Some(record.id().to_owned()),
        location: None,
    }
}

pub fn refresh_workflow_priorities(analysis: &mut ProjectAnalysis) {
    let mut priorities = analysis
        .workflows
        .iter()
        .map(|workflow| {
            let mut score = 10_u32;
            let mut reasons = vec!["workflow trigger observed".to_owned()];

            if workflow.entry_symbol.is_some() {
                score += 25;
                reasons.push("handler symbol resolved".to_owned());
            }

            if workflow.kind.to_string() == "http" {
                score += 20;
                reasons.push("HTTP route observed".to_owned());
            }

            if let Some(entry_symbol) = workflow.entry_symbol.as_deref() {
                let outgoing = analysis
                    .flows
                    .iter()
                    .filter(|flow| flow.from == entry_symbol)
                    .count();
                if outgoing >= 5 {
                    score += 10;
                    reasons.push(format!("{outgoing} outgoing call edges observed"));
                }

                let gaps = analysis
                    .flows
                    .iter()
                    .filter(|flow| flow.from == entry_symbol && flow.to.is_none())
                    .count();
                if gaps > 0 {
                    score += 8;
                    reasons.push(unresolved_call_edge_reason(gaps));
                }
            }

            for expectation in analysis.expectations.iter().filter(|expectation| {
                expectation.target == ExpectationTarget::Workflow
                    && expectation.subject.as_deref() == Some(workflow.id.as_str())
            }) {
                match expectation.status {
                    ExpectationStatus::Accepted => {
                        score += 20;
                        reasons.push("accepted expectation linked".to_owned());
                    }
                    ExpectationStatus::Proposed => {
                        score += 10;
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
                            score += 25;
                            reasons.push("failed verification linked".to_owned());
                        }
                        VerificationStatus::Inconclusive => {
                            score += 12;
                            reasons.push("inconclusive verification linked".to_owned());
                        }
                        VerificationStatus::Passed => {
                            score += 4;
                            reasons.push("passed verification linked".to_owned());
                        }
                    }
                }
            }

            let related_findings = related_finding_count(analysis, workflow);
            if related_findings > 0 {
                score += 6 * u32::try_from(related_findings).unwrap_or(u32::MAX);
                reasons.push(format!("{related_findings} linked findings"));
            }

            WorkflowPriority {
                workflow_id: workflow.id.clone(),
                source: "susumu:derived".to_owned(),
                score,
                detail: reasons.join("; "),
            }
        })
        .collect::<Vec<_>>();

    priorities.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.workflow_id.cmp(&right.workflow_id))
    });
    analysis.workflow_priorities = priorities;
}

fn related_finding_count(analysis: &ProjectAnalysis, workflow: &crate::model::Workflow) -> usize {
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

fn find_cycles(analysis: &ProjectAnalysis) -> Vec<Vec<String>> {
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
    for flow in &analysis.flows {
        if let Some(to) = flow.to.as_deref() {
            graph.entry(&flow.from).or_default().push(to);
        }
    }

    let mut cycles = Vec::new();
    let mut completed = HashSet::new();
    let mut stack = Vec::new();
    let mut active = HashSet::new();
    for symbol in &analysis.symbols {
        visit(
            &symbol.id,
            &graph,
            &mut completed,
            &mut active,
            &mut stack,
            &mut cycles,
        );
    }
    cycles
}

fn visit<'a>(
    node: &'a str,
    graph: &HashMap<&'a str, Vec<&'a str>>,
    completed: &mut HashSet<&'a str>,
    active: &mut HashSet<&'a str>,
    stack: &mut Vec<&'a str>,
    cycles: &mut Vec<Vec<String>>,
) {
    if completed.contains(node) {
        return;
    }
    if active.contains(node) {
        if let Some(start) = stack.iter().position(|candidate| *candidate == node) {
            let mut cycle: Vec<String> = stack[start..]
                .iter()
                .map(|value| (*value).to_owned())
                .collect();
            cycle.push(node.to_owned());
            if !cycles.iter().any(|known| known == &cycle) {
                cycles.push(cycle);
            }
        }
        return;
    }

    active.insert(node);
    stack.push(node);
    if let Some(neighbors) = graph.get(node) {
        for neighbor in neighbors {
            visit(neighbor, graph, completed, active, stack, cycles);
        }
    }
    stack.pop();
    active.remove(node);
    completed.insert(node);
}

fn expectation_subject_exists(
    analysis: &ProjectAnalysis,
    target: ExpectationTarget,
    subject: &str,
) -> bool {
    match target {
        ExpectationTarget::Project => true,
        ExpectationTarget::File => analysis.files.iter().any(|file| file.id == subject),
        ExpectationTarget::Symbol => analysis.symbols.iter().any(|symbol| symbol.id == subject),
        ExpectationTarget::Workflow => analysis
            .workflows
            .iter()
            .any(|workflow| workflow.id == subject),
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{
        Decision, DecisionStatus, Expectation, ExpectationStatus, ExpectationTarget, Language,
        Location, ProjectAnalysis, SourceFile, Symbol, SymbolKind, Verification,
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
            method: "manual review".to_owned(),
            source: "human:test".to_owned(),
            evidence: Some("review:test".to_owned()),
            basis: None,
            detail: "Checkout behavior matched the expectation.".to_owned(),
        });

        anchor_verification_bases(&mut analysis);
        assert!(analysis.verifications[0].basis.is_some());

        analysis.files[0].content_hash = Some("hash1".to_owned());
        refresh_relationship_findings(&mut analysis);

        assert!(analysis.findings.iter().any(|finding| {
            finding.rule_id == "SUS023" && finding.subject.as_deref() == Some("v_checkout")
        }));
    }
}
