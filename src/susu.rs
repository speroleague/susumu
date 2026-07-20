use std::collections::HashMap;

use anyhow::{Context, Result, anyhow, bail};

use crate::model::{
    Decision, Dependency, Expectation, Finding, FlowEdge, ProjectAnalysis, SourceFile, Symbol,
    Verification, Work, Workflow, WorkflowPriority,
};

use crate::susu_parse::{optional_id, parse_location, parse_number, required};
pub use crate::susu_write::{
    write_decisions, write_expectations, write_susu, write_verifications, write_works,
};

#[cfg(test)]
use crate::model::{
    Confidence, DecisionStatus, ExpectationStatus, ExpectationTarget, Language, Location, Severity,
    SymbolKind, VerificationStatus, WorkKind, WorkStatus, WorkflowKind,
};

/// Parses readable or minified `.susu` syntax into an analysis model.
///
/// # Errors
///
/// Returns an error for malformed syntax, missing required fields, unknown
/// record kinds, or values that do not match the declared schema.
#[allow(clippy::too_many_lines)]
pub fn parse_susu(source: &str) -> Result<ProjectAnalysis> {
    let statements = statements(&tokenize(source)?);
    let mut schema_version = None;
    let mut project_name = None;
    let mut root = None;
    let mut generated = None;
    let mut files = Vec::new();
    let mut symbols = Vec::new();
    let mut dependencies = Vec::new();
    let mut workflows = Vec::new();
    let mut workflow_priorities = Vec::new();
    let mut flows = Vec::new();
    let mut expectations = Vec::new();
    let mut verifications = Vec::new();
    let mut decisions = Vec::new();
    let mut works = Vec::new();
    let mut findings = Vec::new();

    for statement in statements {
        let Some(Token::Atom(kind)) = statement.first() else {
            continue;
        };
        match kind.as_str() {
            "susu" => {
                let fields = fields(&statement, 1)?;
                schema_version = Some(parse_number(required(&fields, "version")?, "version")?);
            }
            "project" => {
                let values = fields(&statement, 1)?;
                project_name = Some(required(&values, "name")?.to_owned());
                root = Some(required(&values, "root")?.to_owned());
                generated = Some(parse_number(required(&values, "generated")?, "generated")?);
            }
            "file" => {
                let id = atom_at(&statement, 1, "file id")?;
                let values = fields(&statement, 2)?;
                files.push(SourceFile {
                    id,
                    path: required(&values, "path")?.to_owned(),
                    language: required(&values, "language")?
                        .parse()
                        .map_err(|error: String| anyhow!(error))?,
                    lines: parse_number(required(&values, "lines")?, "lines")?,
                    bytes: parse_number(required(&values, "bytes")?, "bytes")?,
                    content_hash: optional_id(values.get("hash").map_or("-", String::as_str)),
                });
            }
            "symbol" => {
                let id = atom_at(&statement, 1, "symbol id")?;
                let values = fields(&statement, 2)?;
                symbols.push(Symbol {
                    id,
                    name: required(&values, "name")?.to_owned(),
                    kind: required(&values, "kind")?
                        .parse()
                        .map_err(|error: String| anyhow!(error))?,
                    file_id: required(&values, "file")?.to_owned(),
                    content_hash: optional_id(values.get("hash").map_or("-", String::as_str)),
                    location: parse_location(&values)?,
                    entrypoint: required(&values, "entry")?
                        .parse()
                        .context("entry must be true or false")?,
                });
            }
            "dependency" => {
                let values = fields(&statement, 1)?;
                dependencies.push(Dependency {
                    file_id: required(&values, "file")?.to_owned(),
                    name: required(&values, "name")?.to_owned(),
                    location: parse_location(&values)?,
                });
            }
            "workflow" => {
                let id = atom_at(&statement, 1, "workflow id")?;
                let values = fields(&statement, 2)?;
                workflows.push(Workflow {
                    id,
                    kind: required(&values, "kind")?
                        .parse()
                        .map_err(|error: String| anyhow!(error))?,
                    framework: required(&values, "framework")?.to_owned(),
                    trigger: required(&values, "trigger")?.to_owned(),
                    handler: optional_id(required(&values, "handler")?),
                    entry_symbol: optional_id(required(&values, "entry")?),
                    file_id: required(&values, "file")?.to_owned(),
                    confidence: required(&values, "confidence")?
                        .parse()
                        .map_err(|error: String| anyhow!(error))?,
                    location: parse_location(&values)?,
                });
            }
            "attention" | "priority" => {
                let values = fields(&statement, 1)?;
                workflow_priorities.push(WorkflowPriority {
                    workflow_id: required(&values, "workflow")?.to_owned(),
                    source: values
                        .get("source")
                        .cloned()
                        .unwrap_or_else(|| "susumu:derived".to_owned()),
                    score: parse_number(required(&values, "score")?, "score")?,
                    detail: required(&values, "detail")?.to_owned(),
                });
            }
            "flow" => {
                let from = atom_at(&statement, 1, "flow source")?;
                if statement.get(2) != Some(&Token::Arrow) {
                    bail!("flow is missing -> after {from}");
                }
                let target = atom_at(&statement, 3, "flow target")?;
                let values = fields(&statement, 4)?;
                flows.push(FlowEdge {
                    from,
                    to: (target != "?").then_some(target),
                    call: required(&values, "call")?.to_owned(),
                    confidence: required(&values, "confidence")?
                        .parse()
                        .map_err(|error: String| anyhow!(error))?,
                    location: parse_location(&values)?,
                });
            }
            "expectation" => {
                expectations.push(parse_expectation_statement(&statement)?);
            }
            "verification" => {
                verifications.push(parse_verification_statement(&statement)?);
            }
            "decision" => {
                decisions.push(parse_decision_statement(&statement)?);
            }
            "work" => {
                works.push(parse_work_statement(&statement)?);
            }
            "finding" => {
                let rule_id = atom_at(&statement, 1, "finding id")?;
                let values = fields(&statement, 2)?;
                findings.push(Finding {
                    rule_id,
                    source: values
                        .get("source")
                        .cloned()
                        .unwrap_or_else(|| "susumu:scanner".to_owned()),
                    severity: required(&values, "severity")?
                        .parse()
                        .map_err(|error: String| anyhow!(error))?,
                    title: required(&values, "title")?.to_owned(),
                    detail: required(&values, "detail")?.to_owned(),
                    file_id: optional_id(required(&values, "file")?),
                    subject: optional_id(required(&values, "subject")?),
                    location: if values.contains_key("start") {
                        Some(parse_location(&values)?)
                    } else {
                        None
                    },
                });
            }
            other => bail!("unknown .susu statement: {other}"),
        }
    }

    Ok(ProjectAnalysis {
        schema_version: schema_version.context("missing susu statement")?,
        project_name: project_name.context("missing project name")?,
        root: root.context("missing project root")?,
        generated_unix_seconds: generated.context("missing project timestamp")?,
        files,
        symbols,
        dependencies,
        workflows,
        workflow_priorities,
        flows,
        expectations,
        verifications,
        decisions,
        works,
        findings,
    })
}

/// Parses authored `expectation` statements from a fragment or complete artifact.
///
/// # Errors
///
/// Returns an error when an expectation statement is malformed. Complete `.susu`
/// artifacts are accepted; non-expectation records are ignored.
pub fn parse_expectations(source: &str) -> Result<Vec<Expectation>> {
    let statements = statements(&tokenize(source)?);
    let mut expectations = Vec::new();
    for statement in statements {
        let Some(Token::Atom(kind)) = statement.first() else {
            continue;
        };
        if kind == "expectation" {
            expectations.push(parse_expectation_statement(&statement)?);
        }
    }
    Ok(expectations)
}

/// Parses authored `verification` statements from a fragment or complete artifact.
///
/// # Errors
///
/// Returns an error when a verification statement is malformed. Complete
/// `.susu` artifacts are accepted; non-verification records are ignored.
pub fn parse_verifications(source: &str) -> Result<Vec<Verification>> {
    let statements = statements(&tokenize(source)?);
    let mut verifications = Vec::new();
    for statement in statements {
        let Some(Token::Atom(kind)) = statement.first() else {
            continue;
        };
        if kind == "verification" {
            verifications.push(parse_verification_statement(&statement)?);
        }
    }
    Ok(verifications)
}

/// Parses authored `decision` statements from a fragment or complete artifact.
///
/// # Errors
///
/// Returns an error when a decision statement is malformed. Complete `.susu`
/// artifacts are accepted; non-decision records are ignored.
pub fn parse_decisions(source: &str) -> Result<Vec<Decision>> {
    let statements = statements(&tokenize(source)?);
    let mut decisions = Vec::new();
    for statement in statements {
        let Some(Token::Atom(kind)) = statement.first() else {
            continue;
        };
        if kind == "decision" {
            decisions.push(parse_decision_statement(&statement)?);
        }
    }
    Ok(decisions)
}

/// Parses authored `work` statements from a fragment or complete artifact.
///
/// # Errors
///
/// Returns an error when a work statement is malformed. Complete `.susu`
/// artifacts are accepted; non-work records are ignored.
pub fn parse_works(source: &str) -> Result<Vec<Work>> {
    let statements = statements(&tokenize(source)?);
    let mut works = Vec::new();
    for statement in statements {
        let Some(Token::Atom(kind)) = statement.first() else {
            continue;
        };
        if kind == "work" {
            works.push(parse_work_statement(&statement)?);
        }
    }
    Ok(works)
}

fn parse_expectation_statement(statement: &[Token]) -> Result<Expectation> {
    let id = atom_at(statement, 1, "expectation id")?;
    let values = fields(statement, 2)?;
    Ok(Expectation {
        id,
        target: required(&values, "target")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        subject: optional_id(required(&values, "subject")?),
        status: required(&values, "status")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        source: required(&values, "source")?.to_owned(),
        title: required(&values, "title")?.to_owned(),
        detail: required(&values, "detail")?.to_owned(),
    })
}

fn parse_decision_statement(statement: &[Token]) -> Result<Decision> {
    let id = atom_at(statement, 1, "decision id")?;
    let values = fields(statement, 2)?;
    Ok(Decision {
        id,
        target: required(&values, "target")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        subject: optional_id(required(&values, "subject")?),
        status: required(&values, "status")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        source: required(&values, "source")?.to_owned(),
        basis: optional_id(values.get("basis").map_or("-", String::as_str)),
        title: required(&values, "title")?.to_owned(),
        detail: required(&values, "detail")?.to_owned(),
    })
}

fn parse_verification_statement(statement: &[Token]) -> Result<Verification> {
    let id = atom_at(statement, 1, "verification id")?;
    let values = fields(statement, 2)?;
    Ok(Verification {
        id,
        expectation_id: required(&values, "expectation")?.to_owned(),
        status: required(&values, "status")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        supersedes: optional_id(values.get("supersedes").map_or("-", String::as_str)),
        method: required(&values, "method")?.to_owned(),
        source: required(&values, "source")?.to_owned(),
        evidence: optional_id(required(&values, "evidence")?),
        basis: optional_id(values.get("basis").map_or("-", String::as_str)),
        detail: required(&values, "detail")?.to_owned(),
    })
}

fn parse_work_statement(statement: &[Token]) -> Result<Work> {
    let id = atom_at(statement, 1, "work id")?;
    let values = fields(statement, 2)?;
    Ok(Work {
        id,
        target: required(&values, "target")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        subject: optional_id(required(&values, "subject")?),
        expectation_id: optional_id(required(&values, "expectation")?),
        kind: required(&values, "kind")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        status: required(&values, "status")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        source: required(&values, "source")?.to_owned(),
        evidence: optional_id(required(&values, "evidence")?),
        title: required(&values, "title")?.to_owned(),
        detail: required(&values, "detail")?.to_owned(),
    })
}

fn atom_at(tokens: &[Token], index: usize, label: &str) -> Result<String> {
    match tokens.get(index) {
        Some(Token::Atom(value) | Token::String(value)) => Ok(value.clone()),
        _ => bail!("missing or invalid {label}"),
    }
}

fn fields(tokens: &[Token], start: usize) -> Result<HashMap<String, String>> {
    let mut output = HashMap::new();
    let mut index = start;
    while index < tokens.len() {
        let key = match &tokens[index] {
            Token::Atom(value) => value.clone(),
            unexpected => bail!("expected field name, found {unexpected:?}"),
        };
        if tokens.get(index + 1) != Some(&Token::Equals) {
            bail!("expected = after field {key}");
        }
        let value = match tokens.get(index + 2) {
            Some(Token::Atom(value) | Token::String(value)) => value.clone(),
            unexpected => bail!("expected value after {key}=, found {unexpected:?}"),
        };
        output.insert(key, value);
        index += 3;
    }
    Ok(output)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Atom(String),
    String(String),
    Equals,
    Arrow,
    Semicolon,
}

fn tokenize(source: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            character if character.is_whitespace() => index += 1,
            '=' => {
                tokens.push(Token::Equals);
                index += 1;
            }
            ';' => {
                tokens.push(Token::Semicolon);
                index += 1;
            }
            '-' if chars.get(index + 1) == Some(&'>') => {
                tokens.push(Token::Arrow);
                index += 2;
            }
            '"' => {
                let start = index;
                index += 1;
                let mut escaped = false;
                while index < chars.len() {
                    let character = chars[index];
                    index += 1;
                    if escaped {
                        escaped = false;
                    } else if character == '\\' {
                        escaped = true;
                    } else if character == '"' {
                        break;
                    }
                }
                if chars.get(index.saturating_sub(1)) != Some(&'"') {
                    bail!("unterminated string in .susu file");
                }
                let raw: String = chars[start..index].iter().collect();
                tokens.push(Token::String(
                    serde_json::from_str(&raw).context("invalid string escape in .susu file")?,
                ));
            }
            _ => {
                let start = index;
                while index < chars.len() && !is_atom_boundary(&chars, index) {
                    index += 1;
                }
                let atom: String = chars[start..index].iter().collect();
                if atom.is_empty() {
                    bail!("unexpected character in .susu file");
                }
                tokens.push(Token::Atom(atom));
            }
        }
    }
    Ok(tokens)
}

fn is_atom_boundary(chars: &[char], index: usize) -> bool {
    chars[index].is_whitespace()
        || matches!(chars[index], '=' | ';' | '"')
        || (chars[index] == '-' && chars.get(index + 1) == Some(&'>'))
}

fn statements(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut output = Vec::new();
    let mut current = Vec::new();
    for token in tokens {
        if token == &Token::Semicolon {
            if !current.is_empty() {
                output.push(std::mem::take(&mut current));
            }
        } else {
            current.push(token.clone());
        }
    }
    if !current.is_empty() {
        output.push(current);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_lines)]
    fn fixture() -> ProjectAnalysis {
        ProjectAnalysis {
            schema_version: 1,
            project_name: "demo".to_owned(),
            root: "C:\\demo".to_owned(),
            generated_unix_seconds: 42,
            files: vec![SourceFile {
                id: "f0".to_owned(),
                path: "src/main.rs".to_owned(),
                language: Language::Rust,
                lines: 3,
                bytes: 24,
                content_hash: Some("hash0".to_owned()),
            }],
            symbols: vec![Symbol {
                id: "s0".to_owned(),
                name: "main".to_owned(),
                kind: SymbolKind::Function,
                file_id: "f0".to_owned(),
                content_hash: Some("symbol-hash0".to_owned()),
                location: Location {
                    start_line: 1,
                    start_column: 1,
                    end_line: 3,
                    end_column: 2,
                },
                entrypoint: true,
            }],
            dependencies: Vec::new(),
            workflows: vec![Workflow {
                id: "w0".to_owned(),
                kind: WorkflowKind::Http,
                framework: "axum-compatible".to_owned(),
                trigger: "GET /health".to_owned(),
                handler: Some("main".to_owned()),
                entry_symbol: Some("s0".to_owned()),
                file_id: "f0".to_owned(),
                confidence: Confidence::Exact,
                location: Location {
                    start_line: 2,
                    start_column: 5,
                    end_line: 2,
                    end_column: 18,
                },
            }],
            workflow_priorities: vec![WorkflowPriority {
                workflow_id: "w0".to_owned(),
                source: "susumu:derived".to_owned(),
                score: 55,
                detail: "workflow trigger observed; handler symbol resolved".to_owned(),
            }],
            flows: vec![FlowEdge {
                from: "s0".to_owned(),
                to: None,
                call: "println".to_owned(),
                confidence: Confidence::External,
                location: Location {
                    start_line: 2,
                    start_column: 5,
                    end_line: 2,
                    end_column: 18,
                },
            }],
            expectations: vec![Expectation {
                id: "e0".to_owned(),
                target: ExpectationTarget::Workflow,
                subject: Some("w0".to_owned()),
                status: ExpectationStatus::Accepted,
                source: "human:product".to_owned(),
                title: "Health checks remain cheap".to_owned(),
                detail: "The health route must not call external services.".to_owned(),
            }],
            verifications: vec![Verification {
                id: "v0".to_owned(),
                expectation_id: "e0".to_owned(),
                status: VerificationStatus::Passed,
                supersedes: None,
                method: "cargo test".to_owned(),
                source: "human:engineer".to_owned(),
                evidence: Some("ci:123".to_owned()),
                basis: Some("basis-v0".to_owned()),
                detail: "The health test checks local-only behavior.".to_owned(),
            }],
            decisions: vec![Decision {
                id: "d0".to_owned(),
                target: ExpectationTarget::Workflow,
                subject: Some("w0".to_owned()),
                status: DecisionStatus::Accepted,
                source: "human:lead".to_owned(),
                basis: Some("basis0".to_owned()),
                title: "Health route accepted".to_owned(),
                detail: "The team accepts the local-only health check evidence.".to_owned(),
            }],
            works: vec![Work {
                id: "wk0".to_owned(),
                target: ExpectationTarget::Workflow,
                subject: Some("w0".to_owned()),
                expectation_id: Some("e0".to_owned()),
                kind: WorkKind::Implementation,
                status: WorkStatus::Completed,
                source: "agent:codex".to_owned(),
                evidence: Some("commit:abc123".to_owned()),
                title: "Keep health check local".to_owned(),
                detail: "Updated the health workflow so it stays local-only.".to_owned(),
            }],
            findings: vec![Finding {
                rule_id: "SUS004".to_owned(),
                source: "susumu:derived".to_owned(),
                severity: Severity::Info,
                title: "A title".to_owned(),
                detail: "A detail with \"quotes\"".to_owned(),
                file_id: None,
                subject: None,
                location: None,
            }],
        }
    }

    #[test]
    fn readable_round_trip() {
        let expected = fixture();
        let encoded = write_susu(&expected, false).unwrap();
        assert!(encoded.contains("flow s0 -> ?"));
        assert!(encoded.contains("expectation e0 target=workflow"));
        assert!(encoded.contains("verification v0 expectation=e0"));
        assert!(encoded.contains("decision d0 target=workflow"));
        assert!(encoded.contains("work wk0 target=workflow"));
        assert!(encoded.contains("attention workflow=w0"));
        assert_eq!(parse_susu(&encoded).unwrap(), expected);
    }

    #[test]
    fn minified_round_trip() {
        let expected = fixture();
        let encoded = write_susu(&expected, true).unwrap();
        assert!(!encoded.contains('\n'));
        assert_eq!(parse_susu(&encoded).unwrap(), expected);
    }

    #[test]
    fn parses_legacy_priority_and_finding_sources() {
        let source = r#"susu version=1;
project name="demo" root="C:\\demo" generated=42;
file f0 path="src/main.rs" language=rust lines=3 bytes=24;
symbol s0 name="main" kind=function file=f0 start=1:1 end=3:2 entry=true;
workflow w0 kind=http framework="axum-compatible" trigger="GET /health" handler="main" entry=s0 file=f0 confidence=exact start=2:5 end=2:18;
priority workflow=w0 score=55 detail="workflow trigger observed";
finding SUS004 severity=info title="Ambiguous call targets" detail="Targets remain unresolved." file=- subject=-;
"#;

        let parsed = parse_susu(source).unwrap();

        assert_eq!(parsed.workflow_priorities[0].source, "susumu:derived");
        assert_eq!(parsed.findings[0].source, "susumu:scanner");
    }

    #[test]
    fn parses_expectation_fragments() {
        let source = r#"expectation e1 target=project subject=- status=proposed source="human:ops" title="Document backup expectations" detail="The project must document backup and restore expectations.";"#;

        let expectations = parse_expectations(source).unwrap();

        assert_eq!(expectations.len(), 1);
        assert_eq!(expectations[0].id, "e1");
        assert_eq!(expectations[0].subject, None);
    }

    #[test]
    fn writes_expectation_fragments() {
        let expected = fixture().expectations;

        let encoded = write_expectations(&expected, false).unwrap();

        assert!(encoded.starts_with("expectation e0"));
        assert_eq!(parse_expectations(&encoded).unwrap(), expected);
    }

    #[test]
    fn parses_and_writes_verification_fragments() {
        let expected = fixture().verifications;

        let encoded = write_verifications(&expected, false).unwrap();

        assert!(encoded.starts_with("verification v0"));
        assert_eq!(parse_verifications(&encoded).unwrap(), expected);
    }

    #[test]
    fn parses_and_writes_verification_supersession() {
        let source = "verification v_new expectation=e0 status=inconclusive supersedes=v_old method=\"recheck\" source=\"human:reviewer\" evidence=- basis=- detail=\"The prior result is no longer relied on.\";\n";
        let parsed = parse_verifications(source).expect("parse superseding verification");
        assert_eq!(parsed[0].supersedes.as_deref(), Some("v_old"));

        let encoded = write_verifications(&parsed, false).expect("write superseding verification");
        assert!(encoded.contains("supersedes=v_old"));
        assert_eq!(parse_verifications(&encoded).unwrap(), parsed);
    }

    #[test]
    fn parses_and_writes_decision_fragments() {
        let expected = fixture().decisions;

        let encoded = write_decisions(&expected, false).unwrap();

        assert!(encoded.starts_with("decision d0"));
        assert_eq!(parse_decisions(&encoded).unwrap(), expected);
    }

    #[test]
    fn parses_and_writes_work_fragments() {
        let expected = fixture().works;

        let encoded = write_works(&expected, false).unwrap();

        assert!(encoded.starts_with("work wk0"));
        assert_eq!(parse_works(&encoded).unwrap(), expected);
    }
}
