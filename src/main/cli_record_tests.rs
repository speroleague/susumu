use super::*;

#[test]
fn verify_command_parses_passed_status() {
    let cli = Cli::try_parse_from([
        "susumu",
        "verify",
        "e_checkout_sequence",
        "--passed",
        "--method",
        "cargo test checkout",
        "--evidence",
        "run:123",
    ])
    .expect("parse verify shortcut");

    match cli.command.expect("command") {
        Command::Verify(args) => {
            assert_eq!(args.expectation, "e_checkout_sequence");
            assert!(args.passed);
            assert_eq!(args.method, "cargo test checkout");
            assert_eq!(args.evidence.as_deref(), Some("run:123"));
        }
        other => panic!("expected verify shortcut, got {other:?}"),
    }
}

#[test]
fn verify_shortcut_writes_verification_sidecar() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(temp.path().join("main.rs"), "fn main() {}\n").expect("write source");
    fs::write(
        temp.path().join("expectations.susu"),
        "expectation e_verify target=project subject=- status=accepted source=\"human:test\" title=\"Verify shortcut\" detail=\"The verify shortcut should write verification records.\";\n",
    )
    .expect("write expectations sidecar");
    let output = temp.path().join("verifications.susu");

    verify_shortcut(VerifyArgs {
        expectation: "e_verify".to_owned(),
        target: temp.path().to_path_buf(),
        file: output.clone(),
        id: None,
        supersedes: None,
        execution_file: None,
        passed: true,
        failed: false,
        inconclusive: false,
        method: "cargo test".to_owned(),
        source: "human:test".to_owned(),
        evidence: Some("run:123".to_owned()),
        evidence_file: None,
        basis: None,
        detail: None,
        minify: false,
        json: false,
    })
    .expect("verify succeeds");

    let verifications = read_verification_sidecar(&output).expect("read verification sidecar");
    assert_eq!(verifications.len(), 1);
    assert_eq!(verifications[0].expectation_id, "e_verify");
    assert_eq!(verifications[0].status, VerificationStatus::Passed);
    assert_eq!(verifications[0].method, "cargo test");
    assert_eq!(verifications[0].evidence.as_deref(), Some("run:123"));
    assert!(
        verifications[0]
            .detail
            .contains("Recorded by susumu verify.")
    );
}

#[test]
fn verify_requires_one_status_flag() {
    let result = verification_status_from_flags(&VerifyArgs {
        expectation: "e_verify".to_owned(),
        target: PathBuf::from("."),
        file: PathBuf::from("verifications.susu"),
        id: None,
        supersedes: None,
        execution_file: None,
        passed: false,
        failed: false,
        inconclusive: false,
        method: "manual review".to_owned(),
        source: "human:test".to_owned(),
        evidence: None,
        evidence_file: None,
        basis: None,
        detail: None,
        minify: false,
        json: false,
    });

    assert!(result.is_err());
}

#[test]
fn verification_remove_preserves_append_only_history() {
    let temp = tempfile::tempdir().expect("tempdir");
    let file = temp.path().join("verifications.susu");
    fs::write(
        &file,
        "verification v_old expectation=e_verify status=passed supersedes=- method=\"cargo test\" source=\"human:test\" evidence=- basis=- detail=\"Passed.\";\n",
    )
    .expect("write verification sidecar");

    let result = remove_verification(&RemoveVerification {
        file: file.clone(),
        id: "v_old".to_owned(),
    });

    assert!(result.is_err());
    assert!(fs::read_to_string(file).unwrap().contains("v_old"));
}

#[test]
fn verification_chain_detects_tampering_after_initialization() {
    let source = "verification v_one expectation=e_verify status=passed method=\"cargo test\" source=\"human:test\" evidence=- basis=- detail=\"First.\";\nverification v_two expectation=e_verify status=failed method=\"cargo test\" source=\"human:test\" evidence=- basis=- detail=\"Second.\";\n";
    let mut records = parse_verifications(source).expect("parse chain records");
    let mut previous = None;
    for record in &mut records {
        let chain = verification_chain_digest(previous.as_deref(), record);
        record.chain = Some(chain.clone());
        previous = Some(chain);
    }

    let report = verify_verification_chain(Path::new("verifications.susu"), &records);
    assert_eq!(report.status, "valid");

    records[0].detail = "Edited after initialization.".to_owned();
    let tampered = verify_verification_chain(Path::new("verifications.susu"), &records);
    assert_eq!(tampered.status, "broken");
    assert_eq!(tampered.broken_at.as_deref(), Some("v_one"));
}

#[test]
fn evidence_file_hash_is_content_only() {
    let temp = tempfile::tempdir().expect("tempdir");
    let file = temp.path().join("junit.xml");
    let other_file = temp.path().join("renamed.xml");
    fs::write(&file, b"<testsuite tests=\"1\" failures=\"0\"/>").expect("write evidence");
    fs::write(&other_file, b"<testsuite tests=\"1\" failures=\"0\"/>")
        .expect("write second evidence");

    let hash = hash_evidence_file(&file).expect("hash evidence");
    assert!(hash.starts_with("sha256:"));
    assert_eq!(hash.len(), "sha256:".len() + 64);
    assert_eq!(
        hash,
        hash_evidence_file(&other_file).expect("hash second evidence")
    );
}

#[test]
fn review_shortcut_writes_convention_based_outputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(temp.path().join("main.rs"), "fn main() {}\n").expect("write source");
    fs::write(
        temp.path().join("expectations.susu"),
        "expectation e_review target=project subject=- status=accepted source=\"human:test\" title=\"Review stays easy\" detail=\"Daily review should use convention-based outputs.\";\n",
    )
    .expect("write expectations sidecar");
    fs::write(
        temp.path().join("verifications.susu"),
        "verification v_review expectation=e_review status=passed method=\"manual smoke test\" source=\"human:test\" evidence=\"local:review\" detail=\"The daily review command wrote convention-based outputs.\";\n",
    )
    .expect("write verifications sidecar");
    fs::write(
        temp.path().join(PORTAL_CONFIG_FILE),
        "[portal]\ntitle = \"Daily Memory\"\naccent = \"#778899\"\n",
    )
    .expect("write portal config");

    review_shortcut(&ReviewShortcutArgs {
        target: temp.path().to_path_buf(),
        output_dir: PathBuf::from(".susumu"),
        work: None,
        strict: false,
        fail_on_check: false,
        no_html: false,
        serve: false,
        host: "127.0.0.1".to_owned(),
        port: 0,
        json: false,
    })
    .expect("run review shortcut");

    let artifact_path = temp.path().join(".susumu").join("project.susu");
    let packet_path = temp.path().join(".susumu").join("review.susu");
    let check_path = temp.path().join(".susumu").join("check.json");
    let html_path = temp.path().join(".susumu").join("review.html");
    assert!(artifact_path.exists());
    assert!(packet_path.exists());
    assert!(check_path.exists());
    assert!(html_path.exists());

    let artifact = read_analysis_artifact(&artifact_path).expect("read project artifact");
    assert!(
        artifact
            .expectations
            .iter()
            .any(|expectation| expectation.id == "e_review")
    );
    assert!(
        artifact
            .verifications
            .iter()
            .any(|verification| verification.id == "v_review")
    );

    let packet = read_review_packet(&packet_path).expect("read review packet");
    assert_eq!(packet.artifact.expectations.len(), 1);
    assert_eq!(packet.artifact.verifications.len(), 1);
    assert!(
        packet
            .expectation_readiness
            .iter()
            .any(|item| item.expectation_id == "e_review" && item.bucket == "verified")
    );

    let check_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&check_path).expect("read check json"))
            .expect("check json should parse");
    assert_eq!(check_json["project"]["name"], artifact.project_name);
    let html = fs::read_to_string(&html_path).expect("read html portal");
    assert!(html.contains("Daily Memory &middot;"));
    assert!(html.contains(":root{--accent:#778899}"));
}

#[test]
fn open_static_review_requires_review_export() {
    let temp = tempfile::tempdir().expect("tempdir");
    let packet = temp.path().join("review.susu");

    let error = open_static_review(&packet).expect_err("missing HTML should fail");

    assert!(error.to_string().contains("run `susumu review` first"));
    assert!(error.to_string().contains("review.html"));
}

#[test]
fn sidecar_record_guard_ignores_semicolons_inside_strings() {
    let work = "work wk_one target=project subject=- expectation=- kind=implementation status=completed source=\"test\" evidence=\"commit:abc\" title=\"One\" detail=\"Generated by git connect. Reasons: 1 expectation link(s); no work record references this commit.\";\n";

    assert!(!has_records_other_than(work, "work"));
    assert!(has_records_other_than(work, "expectation"));
}
