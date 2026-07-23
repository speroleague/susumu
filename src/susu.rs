use anyhow::{Context, Result, anyhow, bail};

use crate::model::{
    Decision, Dependency, Expectation, Finding, FlowEdge, ProjectAnalysis, SourceFile, Symbol,
    Verification, Work, Workflow, WorkflowPriority,
};

use crate::susu_parse::{
    Token, atom_at, fields, optional_id, parse_location, parse_number, parse_work_statement,
    required, statements, tokenize,
};
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
pub fn parse_susu(source: &str) -> Result<ProjectAnalysis> {
    let mut parsed = ParsedAnalysis::default();
    for statement in statements(&tokenize(source)?) {
        parse_statement(&statement, &mut parsed)?;
    }
    parsed.into_analysis()
}

#[derive(Default)]
struct ParsedAnalysis {
    schema_version: Option<u32>,
    project_name: Option<String>,
    root: Option<String>,
    generated: Option<u64>,
    files: Vec<SourceFile>,
    symbols: Vec<Symbol>,
    dependencies: Vec<Dependency>,
    workflows: Vec<Workflow>,
    workflow_priorities: Vec<WorkflowPriority>,
    flows: Vec<FlowEdge>,
    expectations: Vec<Expectation>,
    verifications: Vec<Verification>,
    decisions: Vec<Decision>,
    works: Vec<Work>,
    findings: Vec<Finding>,
}

impl ParsedAnalysis {
    fn into_analysis(self) -> Result<ProjectAnalysis> {
        Ok(ProjectAnalysis {
            schema_version: self.schema_version.context("missing susu statement")?,
            project_name: self.project_name.context("missing project name")?,
            root: self.root.context("missing project root")?,
            generated_unix_seconds: self.generated.context("missing project timestamp")?,
            files: self.files,
            symbols: self.symbols,
            dependencies: self.dependencies,
            workflows: self.workflows,
            workflow_priorities: self.workflow_priorities,
            flows: self.flows,
            expectations: self.expectations,
            verifications: self.verifications,
            decisions: self.decisions,
            works: self.works,
            findings: self.findings,
        })
    }
}

fn parse_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let Some(Token::Atom(kind)) = statement.first() else {
        return Ok(());
    };
    match kind.as_str() {
        "susu" | "project" => parse_metadata_statement(kind, statement, parsed),
        "file" | "symbol" | "dependency" | "workflow" | "attention" | "priority" | "flow" => {
            parse_evidence_statement(kind, statement, parsed)
        }
        "expectation" | "verification" | "decision" | "work" | "finding" => {
            parse_record_statement(kind, statement, parsed)
        }
        other => bail!("unknown .susu statement: {other}"),
    }
}

fn parse_metadata_statement(
    kind: &str,
    statement: &[Token],
    parsed: &mut ParsedAnalysis,
) -> Result<()> {
    let values = fields(statement, 1)?;
    match kind {
        "susu" => {
            parsed.schema_version = Some(parse_number(required(&values, "version")?, "version")?);
        }
        "project" => {
            parsed.project_name = Some(required(&values, "name")?.to_owned());
            parsed.root = Some(required(&values, "root")?.to_owned());
            parsed.generated = Some(parse_number(required(&values, "generated")?, "generated")?);
        }
        _ => unreachable!("non-metadata statement routed to metadata parser"),
    }
    Ok(())
}

fn parse_evidence_statement(
    kind: &str,
    statement: &[Token],
    parsed: &mut ParsedAnalysis,
) -> Result<()> {
    match kind {
        "file" => parse_file_statement(statement, parsed),
        "symbol" => parse_symbol_statement(statement, parsed),
        "dependency" => parse_dependency_statement(statement, parsed),
        "workflow" => parse_workflow_statement(statement, parsed),
        "attention" | "priority" => parse_priority_statement(statement, parsed),
        "flow" => parse_flow_statement(statement, parsed),
        _ => unreachable!("non-evidence statement routed to evidence parser"),
    }
}

fn parse_file_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let id = atom_at(statement, 1, "file id")?;
    let values = fields(statement, 2)?;
    parsed.files.push(SourceFile {
        id,
        path: required(&values, "path")?.to_owned(),
        language: required(&values, "language")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        lines: parse_number(required(&values, "lines")?, "lines")?,
        bytes: parse_number(required(&values, "bytes")?, "bytes")?,
        content_hash: optional_id(values.get("hash").map_or("-", String::as_str)),
    });
    Ok(())
}

fn parse_symbol_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let id = atom_at(statement, 1, "symbol id")?;
    let values = fields(statement, 2)?;
    parsed.symbols.push(Symbol {
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
    Ok(())
}

fn parse_dependency_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let values = fields(statement, 1)?;
    parsed.dependencies.push(Dependency {
        file_id: required(&values, "file")?.to_owned(),
        name: required(&values, "name")?.to_owned(),
        location: parse_location(&values)?,
    });
    Ok(())
}

fn parse_workflow_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let id = atom_at(statement, 1, "workflow id")?;
    let values = fields(statement, 2)?;
    parsed.workflows.push(Workflow {
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
    Ok(())
}

fn parse_priority_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let values = fields(statement, 1)?;
    parsed.workflow_priorities.push(WorkflowPriority {
        workflow_id: required(&values, "workflow")?.to_owned(),
        source: values
            .get("source")
            .cloned()
            .unwrap_or_else(|| "susumu:derived".to_owned()),
        score: parse_number(required(&values, "score")?, "score")?,
        detail: required(&values, "detail")?.to_owned(),
    });
    Ok(())
}

fn parse_flow_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let from = atom_at(statement, 1, "flow source")?;
    if statement.get(2) != Some(&Token::Arrow) {
        bail!("flow is missing -> after {from}");
    }
    let target = atom_at(statement, 3, "flow target")?;
    let values = fields(statement, 4)?;
    parsed.flows.push(FlowEdge {
        from,
        to: (target != "?").then_some(target),
        call: required(&values, "call")?.to_owned(),
        confidence: required(&values, "confidence")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        location: parse_location(&values)?,
    });
    Ok(())
}

fn parse_record_statement(
    kind: &str,
    statement: &[Token],
    parsed: &mut ParsedAnalysis,
) -> Result<()> {
    match kind {
        "expectation" => parsed
            .expectations
            .push(parse_expectation_statement(statement)?),
        "verification" => parsed
            .verifications
            .push(parse_verification_statement(statement)?),
        "decision" => parsed.decisions.push(parse_decision_statement(statement)?),
        "work" => parsed.works.push(parse_work_statement(statement)?),
        "finding" => parsed.findings.push(parse_finding_statement(statement)?),
        _ => unreachable!("non-record statement routed to record parser"),
    }
    Ok(())
}

fn parse_finding_statement(statement: &[Token]) -> Result<Finding> {
    let rule_id = atom_at(statement, 1, "finding id")?;
    let values = fields(statement, 2)?;
    Ok(Finding {
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
        execution: values
            .get("execution")
            .filter(|value| value.as_str() != "-")
            .map(|value| serde_json::from_str(value))
            .transpose()
            .context("invalid verification execution metadata")?,
        chain: optional_id(values.get("chain").map_or("-", String::as_str)),
        method: required(&values, "method")?.to_owned(),
        source: required(&values, "source")?.to_owned(),
        evidence: optional_id(required(&values, "evidence")?),
        basis: optional_id(values.get("basis").map_or("-", String::as_str)),
        detail: required(&values, "detail")?.to_owned(),
    })
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
                execution: None,
                chain: None,
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
    fn parses_and_writes_verification_execution_metadata() {
        let source = r#"verification v_exec expectation=e0 status=passed supersedes=- execution="{\"result\":\"passed\",\"exit_code\":0,\"run_id\":\"run-1\",\"issued_at\":\"2026-07-20T00:00:00Z\",\"artifact_manifest\":\"sha256:manifest\"}" method="cargo test" source="runner:local" evidence="sha256:artifact" basis=- detail="Execution metadata was supplied separately.";"#;
        let parsed = parse_verifications(source).expect("parse execution metadata");
        let execution = parsed[0].execution.as_ref().expect("execution metadata");
        assert_eq!(execution.result, "passed");
        assert_eq!(execution.exit_code, Some(0));
        assert_eq!(execution.run_id.as_deref(), Some("run-1"));

        let encoded = write_verifications(&parsed, false).expect("write execution metadata");
        assert!(encoded.contains("execution="));
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
