use super::test_support::*;
use super::*;

#[test]
fn handoff_flags_expectations_and_work_without_verification() {
    let mut artifact = test_artifact();
    refresh_derived_analysis(&mut artifact);
    artifact.works.push(Work {
        id: "wk_checkout".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        expectation_id: Some("e_checkout_sequence".to_owned()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "agent:test".to_owned(),
        evidence: Some("commit:abc123".to_owned()),
        title: "Implement checkout sequence".to_owned(),
        detail: "Checkout implementation touched the workflow and now needs verification."
            .to_owned(),
    });

    let check = check_report(&artifact, false);
    let report = handoff_report(&artifact, &check);

    assert!(!report.top_workflows.is_empty());
    assert_eq!(report.top_workflows[0].id, "w_checkout");
    assert_eq!(
        report.expectations_without_verification[0].id,
        "e_checkout_sequence"
    );
    assert_eq!(report.work_needing_verification[0].id, "wk_checkout");
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| action.contains("wk_checkout"))
    );
}

fn review_packet_fixture() -> (tempfile::TempDir, ProjectAnalysis) {
    let mut artifact = test_artifact();
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("create source dir");
    fs::write(
        temp.path().join("src").join("api.ts"),
        "export function checkout() { return 'ok'; }\n",
    )
    .expect("write api source");
    fs::write(
        temp.path().join("src").join("routes.php"),
        "<?php function php_checkout() { return 'ok'; }\n",
    )
    .expect("write php source");
    artifact.root = temp.path().display().to_string();
    refresh_derived_analysis(&mut artifact);
    artifact.works.push(Work {
        id: "wk_checkout".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        expectation_id: Some("e_checkout_sequence".to_owned()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "agent:test".to_owned(),
        evidence: Some("commit:abc123".to_owned()),
        title: "Implement checkout sequence".to_owned(),
        detail: "Checkout implementation touched the workflow and now needs verification."
            .to_owned(),
    });
    (temp, artifact)
}

#[test]
fn review_packet_embeds_artifact_and_handoff_state() {
    let (_temp, artifact) = review_packet_fixture();
    let check = check_report(&artifact, false);
    let handoff = handoff_report(&artifact, &check);

    let packet = review_packet("fixture.susu".to_owned(), 123, &artifact, &check, &handoff);
    let json = serde_json::to_value(packet).expect("packet serializes");

    assert_eq!(json["schema_version"], "susumu.review.v1");
    assert_eq!(json["created_unix_seconds"], 123);
    assert_eq!(json["source"]["input"], "fixture.susu");
    assert_eq!(json["project"]["name"], "fixture");
    assert_eq!(json["artifact"]["project_name"], "fixture");
    assert_eq!(json["artifact"]["workflows"].as_array().unwrap().len(), 2);
    let previews = json["source_previews"].as_array().expect("source previews");
    assert!(previews.iter().any(|preview| {
        preview["file_id"] == "f_api"
            && preview["path"] == "src/api.ts"
            && preview["lines"][0]["tokens"]
                .as_array()
                .is_some_and(|tokens| !tokens.is_empty())
    }));
    assert_eq!(json["top_workflows"][0]["id"], "w_checkout");
    assert_eq!(
        json["expectations_without_verification"][0]["id"],
        "e_checkout_sequence"
    );
    assert_eq!(
        json["expectation_support"][0]["expectation_id"],
        "e_checkout_sequence"
    );
    assert_eq!(
        json["expectation_support"][0]["support_status"],
        "partially_supported"
    );
    assert_eq!(json["expectation_support"][0]["target_observed"], true);
    assert_eq!(json["expectation_support"][0]["work"], 1);
    assert_eq!(
        json["expectation_readiness"][0]["expectation_id"],
        "e_checkout_sequence"
    );
    assert_eq!(
        json["expectation_readiness"][0]["bucket"],
        "needs_verification"
    );
    assert_eq!(
        json["expectation_readiness"][0]["label"],
        "Has work, needs verification"
    );
    assert!(
        json["expectation_readiness"][0]["next_action"]
            .as_str()
            .expect("next action")
            .contains("susumu verify e_checkout_sequence")
    );
    assert_eq!(json["work_needing_verification"][0]["id"], "wk_checkout");
}

fn write_portable_memory_sources(source_root: &Path) {
    fs::create_dir_all(source_root.join("src")).expect("create source dir");
    fs::write(
        source_root.join("src").join("api.ts"),
        "export function checkout() { return reserveInventory(); }\n",
    )
    .expect("write api source");
    fs::write(
        source_root.join("src").join("routes.php"),
        "<?php function php_checkout() { return 'ok'; }\n",
    )
    .expect("write php source");
}

fn portable_memory_artifact(source_root: &Path) -> ProjectAnalysis {
    let mut artifact = test_artifact();
    artifact.root = source_root.display().to_string();
    artifact.verifications.push(Verification {
        id: "v_checkout".to_owned(),
        expectation_id: "e_checkout_sequence".to_owned(),
        status: VerificationStatus::Passed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "cargo test checkout".to_owned(),
        source: "ci:test".to_owned(),
        evidence: Some("run:checkout".to_owned()),
        basis: None,
        detail: "Checkout behavior was verified.".to_owned(),
    });
    artifact.decisions.push(Decision {
        id: "d_checkout".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        status: DecisionStatus::Accepted,
        source: "human:product".to_owned(),
        basis: None,
        title: "Keep checkout reservation first".to_owned(),
        detail: "The business accepted reserve-before-charge as durable intent.".to_owned(),
    });
    artifact.works.push(Work {
        id: "wk_checkout".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        expectation_id: Some("e_checkout_sequence".to_owned()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "agent:test".to_owned(),
        evidence: Some("commit:abc123".to_owned()),
        title: "Implement checkout sequence".to_owned(),
        detail: "Checkout implementation touched the workflow.".to_owned(),
    });
    refresh_derived_analysis(&mut artifact);
    artifact
}

fn archive_project_memory(artifact: &ProjectAnalysis, archive_root: &Path) -> (PathBuf, PathBuf) {
    fs::create_dir_all(archive_root).expect("create archive dir");
    let check = check_report(artifact, false);
    let handoff = handoff_report(artifact, &check);
    let packet = review_packet(
        "source/project.susu".to_owned(),
        123,
        artifact,
        &check,
        &handoff,
    );
    let packet_json =
        serde_json::to_string_pretty(&packet).expect("packet should serialize to JSON");
    let archived_packet = archive_root.join("review.susu");
    let archived_artifact = archive_root.join("project.susu");
    fs::write(&archived_packet, packet_json).expect("write archived review packet");
    fs::write(
        &archived_artifact,
        write_susu(artifact, false).expect("write artifact"),
    )
    .expect("write archived artifact");
    (archived_packet, archived_artifact)
}

fn assert_portable_memory_records(
    loaded_packet: &ReviewPacketStored,
    loaded_artifact: &ProjectAnalysis,
) {
    assert_eq!(loaded_packet.artifact.files.len(), 2);
    assert_eq!(loaded_packet.artifact.workflows.len(), 2);
    assert_eq!(loaded_packet.artifact.expectations.len(), 1);
    assert_eq!(loaded_packet.artifact.verifications.len(), 1);
    assert_eq!(loaded_packet.artifact.decisions.len(), 1);
    assert_eq!(loaded_packet.artifact.works.len(), 1);
    assert!(
        loaded_packet
            .source_previews
            .iter()
            .any(|preview| preview.file_id == "f_api"
                && preview.path == "src/api.ts"
                && preview.lines.iter().any(|line| !line.tokens.is_empty()))
    );
    assert!(
        loaded_packet
            .expectation_support
            .iter()
            .any(|support| support.expectation_id == "e_checkout_sequence"
                && support.support_status == "verified")
    );
    assert_eq!(
        loaded_artifact.expectations,
        loaded_packet.artifact.expectations
    );
    assert_eq!(
        loaded_artifact.verifications,
        loaded_packet.artifact.verifications
    );
    assert_eq!(loaded_artifact.decisions, loaded_packet.artifact.decisions);
    assert_eq!(loaded_artifact.works, loaded_packet.artifact.works);
    assert!(
        review_portal_html(loaded_packet)
            .expect("render archived portal")
            .contains("Checkout reserves inventory before charging")
    );
}

#[test]
fn review_packet_and_artifact_are_portable_project_memory() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source_root = temp.path().join("source");
    let archive_root = temp.path().join("archive");
    write_portable_memory_sources(&source_root);
    let artifact = portable_memory_artifact(&source_root);
    let (archived_packet, archived_artifact) = archive_project_memory(&artifact, &archive_root);

    fs::remove_dir_all(source_root.join("src")).expect("remove original source files");

    let loaded_packet = read_review_packet(&archived_packet).expect("read archived packet");
    let loaded_artifact =
        read_analysis_artifact(&archived_artifact).expect("read archived artifact");
    assert_portable_memory_records(&loaded_packet, &loaded_artifact);
}

pub(super) fn stored_review_packet(
    input: &str,
    created: u64,
    artifact: &ProjectAnalysis,
) -> ReviewPacketStored {
    let check = check_report(artifact, false);
    let handoff = handoff_report(artifact, &check);
    let packet = review_packet(input.to_owned(), created, artifact, &check, &handoff);
    serde_json::from_value(serde_json::to_value(packet).expect("packet serializes"))
        .expect("packet deserializes")
}

#[test]
fn readiness_json_summarizes_packet_queue() {
    let mut artifact = test_artifact();
    refresh_derived_analysis(&mut artifact);
    artifact.works.push(Work {
        id: "wk_checkout".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        expectation_id: Some("e_checkout_sequence".to_owned()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "agent:test".to_owned(),
        evidence: Some("commit:abc123".to_owned()),
        title: "Implement checkout sequence".to_owned(),
        detail: "Checkout implementation touched the workflow and now needs verification."
            .to_owned(),
    });
    let packet = stored_review_packet("fixture.review.susu", 1, &artifact);

    let items = readiness_command::filtered_items(
        &packet.expectation_readiness,
        Some("needs_verification"),
        Some("checkout"),
    );
    let json = readiness_command::readiness_json(
        Path::new("fixture.review.susu"),
        &packet,
        &items,
        Some("needs_verification"),
        Some("checkout"),
    );

    assert_eq!(json["packet"], "fixture.review.susu");
    assert_eq!(json["total"], 1);
    assert_eq!(json["shown"], 1);
    assert_eq!(json["filters"]["bucket"], "needs_verification");
    assert_eq!(json["filters"]["search"], "checkout");
    assert_eq!(json["items"][0]["bucket"], "needs_verification");
    assert_eq!(
        json["counts"]
            .as_array()
            .expect("counts")
            .iter()
            .find(|item| item["bucket"] == "needs_verification")
            .expect("needs verification count")["count"],
        1
    );
}

#[test]
fn readiness_filters_by_bucket_label_and_search() {
    let mut artifact = test_artifact();
    refresh_derived_analysis(&mut artifact);
    artifact.works.push(Work {
        id: "wk_checkout".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        expectation_id: Some("e_checkout_sequence".to_owned()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "agent:test".to_owned(),
        evidence: Some("commit:abc123".to_owned()),
        title: "Implement checkout sequence".to_owned(),
        detail: "Checkout implementation touched the workflow and now needs verification."
            .to_owned(),
    });
    let packet = stored_review_packet("fixture.review.susu", 1, &artifact);
    let bucket =
        readiness_command::canonical_bucket(Some("Has work, needs verification")).expect("bucket");

    let filtered =
        readiness_command::filtered_items(&packet.expectation_readiness, bucket, Some("checkout"));

    assert_eq!(bucket, Some("needs_verification"));
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].expectation_id, "e_checkout_sequence");
    assert!(readiness_command::canonical_bucket(Some("unknown")).is_err());
}
