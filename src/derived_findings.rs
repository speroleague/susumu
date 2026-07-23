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
            let recursive = cycle.len() == 2 && cycle.first() == cycle.last();
            let (title, detail) = if recursive {
                (
                    "Recursive call",
                    format!("Observed recursive call: {} calls itself.", names[0]),
                )
            } else {
                (
                    "Call cycle",
                    format!("Observed call cycle: {}", names.join(" -> ")),
                )
            };
            analysis.findings.push(Finding {
                rule_id: "SUS005".to_owned(),
                source: "susumu:derived".to_owned(),
                severity: Severity::Warning,
                title: title.to_owned(),
                detail,
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
    let mut active = HashSet::new();
    for symbol in &analysis.symbols {
        let node = symbol.id.as_str();
        if completed.contains(node) {
            continue;
        }

        let mut path = Vec::new();
        let mut frames = vec![(node, 0usize)];
        active.insert(node);
        path.push(node);
        while let Some((current, next_neighbor)) = frames.last_mut() {
            let neighbor = graph
                .get(current)
                .and_then(|neighbors| neighbors.get(*next_neighbor))
                .copied();
            let Some(neighbor) = neighbor else {
                let (finished, _) = frames.pop().expect("cycle traversal frame exists");
                path.pop();
                active.remove(finished);
                completed.insert(finished);
                continue;
            };
            *next_neighbor += 1;

            if completed.contains(neighbor) {
                continue;
            }
            if active.contains(neighbor) {
                if let Some(start) = path.iter().position(|candidate| *candidate == neighbor) {
                    let mut cycle: Vec<String> = path[start..]
                        .iter()
                        .map(|value| (*value).to_owned())
                        .collect();
                    cycle.push(neighbor.to_owned());
                    if !cycles.iter().any(|known| known == &cycle) {
                        cycles.push(cycle);
                    }
                }
                continue;
            }

            active.insert(neighbor);
            path.push(neighbor);
            frames.push((neighbor, 0));
        }
    }
    cycles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FlowEdge, Language, Location, SourceFile, Symbol, SymbolKind};

    fn symbol(id: &str, name: &str) -> Symbol {
        Symbol {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: SymbolKind::Function,
            file_id: "file".to_owned(),
            content_hash: None,
            location: Location {
                start_line: 1,
                start_column: 1,
                end_line: 2,
                end_column: 1,
            },
            entrypoint: false,
        }
    }

    fn analysis(symbols: Vec<Symbol>, flows: Vec<FlowEdge>) -> ProjectAnalysis {
        ProjectAnalysis {
            schema_version: 1,
            project_name: "test".to_owned(),
            root: ".".to_owned(),
            generated_unix_seconds: 0,
            files: vec![SourceFile {
                id: "file".to_owned(),
                path: "test.rs".to_owned(),
                language: Language::Rust,
                lines: 2,
                bytes: 0,
                content_hash: None,
            }],
            symbols,
            dependencies: Vec::new(),
            workflows: Vec::new(),
            workflow_priorities: Vec::new(),
            flows,
            expectations: Vec::new(),
            verifications: Vec::new(),
            decisions: Vec::new(),
            works: Vec::new(),
            findings: Vec::new(),
        }
    }

    #[test]
    fn classifies_self_cycles_as_recursive_calls() {
        let mut analysis = analysis(
            vec![symbol("a", "walk")],
            vec![FlowEdge {
                from: "a".to_owned(),
                to: Some("a".to_owned()),
                call: "walk".to_owned(),
                confidence: Confidence::Exact,
                location: Location {
                    start_line: 1,
                    start_column: 1,
                    end_line: 1,
                    end_column: 1,
                },
            }],
        );

        add_static_findings(&mut analysis);

        let finding = analysis
            .findings
            .iter()
            .find(|finding| finding.rule_id == "SUS005")
            .expect("recursive finding");
        assert_eq!(finding.title, "Recursive call");
        assert_eq!(
            finding.detail,
            "Observed recursive call: walk calls itself."
        );
    }
}
