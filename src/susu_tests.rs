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
