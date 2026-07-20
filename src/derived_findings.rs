use std::collections::{HashMap, HashSet};

use crate::model::{Confidence, Finding, ProjectAnalysis, Severity};

const LARGE_FILE_LINES: usize = 600;
const LARGE_SYMBOL_LINES: usize = 80;
const HIGH_FAN_OUT: usize = 8;

pub(crate) fn add_static_findings(analysis: &mut ProjectAnalysis) {
    add_large_file_findings(analysis);
    add_long_symbol_findings(analysis);
    add_high_fan_out_findings(analysis);
    add_ambiguous_call_finding(analysis);
    add_cycle_findings(analysis);
}

fn add_large_file_findings(analysis: &mut ProjectAnalysis) {
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
}

fn add_long_symbol_findings(analysis: &mut ProjectAnalysis) {
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
}

fn add_high_fan_out_findings(analysis: &mut ProjectAnalysis) {
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
}

fn add_ambiguous_call_finding(analysis: &mut ProjectAnalysis) {
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
