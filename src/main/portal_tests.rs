use super::review_packet_tests::stored_review_packet;
use super::test_support::*;
use super::*;
use susumu::model::{Location, ReviewCommentKind, ReviewStatus, ReviewThread, Severity};

#[test]
fn ci_workflow_uploads_and_publishes_susumu_review_artifacts() {
    let workflow = include_str!("../../.github/workflows/ci.yml");

    assert!(workflow.contains("pull_request:"));
    assert!(workflow.contains("Susumu self-review packet"));
    assert!(workflow.contains("cargo run --locked -- review build ."));
    assert!(workflow.contains("--artifact-output \"$SUSUMU_ARTIFACT_DIR/project.susu\""));
    assert!(workflow.contains("--check-json \"$SUSUMU_ARTIFACT_DIR/check.json\""));
    assert!(workflow.contains("--output \"$SUSUMU_ARTIFACT_DIR/review.susu\""));
    assert!(workflow.contains("--html \"$SUSUMU_ARTIFACT_DIR/review.html\""));
    assert!(workflow.contains("Verify Susumu artifacts exist"));
    assert!(workflow.contains("actions/upload-artifact@v4"));
    assert!(workflow.contains("if-no-files-found: error"));
    assert!(workflow.contains("retention-days: 14"));
    assert!(workflow.contains("target/susumu-review/project.susu"));
    assert!(workflow.contains("target/susumu-review/check.json"));
    assert!(workflow.contains("target/susumu-review/review.susu"));
    assert!(workflow.contains("target/susumu-review/review.html"));
    assert!(workflow.contains("Publish Susumu portal"));
    assert!(workflow.contains("github.event_name == 'push' && github.ref == 'refs/heads/main'"));
    assert!(workflow.contains("pages: write"));
    assert!(workflow.contains("id-token: write"));
    assert!(workflow.contains("actions/download-artifact@v4"));
    assert!(workflow.contains("cp susumu-pages/review.html susumu-pages/index.html"));
    assert!(workflow.contains("actions/configure-pages@v5"));
    assert!(workflow.contains("actions/upload-pages-artifact@v3"));
    assert!(workflow.contains("actions/deploy-pages@v4"));
}

#[test]
fn readme_links_to_live_susumu_pages_portal() {
    let readme = include_str!("../../README.md");

    assert!(readme.contains("https://speroleague.github.io/susumu/"));
    assert!(readme.contains("View the live Susumu review portal"));
}

#[test]
fn portal_config_parses_branding_section() {
    let config = parse_portal_config(
        r##"
        [ignored]
        accent = "#ffffff"

        [portal]
        title = "Acme Project Memory"
        background = "#101820"
        accent = "#abc"
        ok = "#a1b2c3" # muted success
        "##,
    )
    .expect("parse portal config");

    assert_eq!(config.title.as_deref(), Some("Acme Project Memory"));
    assert_eq!(
        config.css_vars.get("--bg").map(String::as_str),
        Some("#101820")
    );
    assert_eq!(
        config.css_vars.get("--accent").map(String::as_str),
        Some("#abc")
    );
    assert_eq!(
        config.css_vars.get("--ok").map(String::as_str),
        Some("#a1b2c3")
    );
    assert!(parse_portal_config("[portal]\naccent = \"red\"\n").is_err());
}

#[test]
fn exported_review_html_loads_portal_config_from_project_root() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join(PORTAL_CONFIG_FILE),
        "[portal]\ntitle = \"Acme Memory\"\naccent = \"#445566\"\nbackground = \"#101820\"\n",
    )
    .expect("write portal config");
    let mut artifact = test_artifact();
    artifact.root = temp.path().display().to_string();
    refresh_derived_analysis(&mut artifact);
    let packet = stored_review_packet("fixture.review.susu", 1, &artifact);
    let packet_json = serde_json::to_string_pretty(&packet).expect("packet serializes");
    let packet_path = temp.path().join("fixture.review.susu");
    let html_path = temp.path().join("fixture-review.html");
    fs::write(&packet_path, packet_json).expect("write packet");

    export_review_html(&ReviewExportHtmlArgs {
        packet: packet_path,
        output: html_path.clone(),
    })
    .expect("export succeeds");

    let html = fs::read_to_string(html_path).expect("read html");
    assert!(html.contains("Acme Memory &middot; fixture"));
    assert!(html.contains("<div class=\"eyebrow\">Acme Memory</div>"));
    assert!(html.contains(":root{--accent:#445566;--bg:#101820}"));
}

#[test]
fn review_portal_html_embeds_packet_safely() {
    let mut artifact = test_artifact();
    artifact.project_name = "fixture </script>".to_owned();
    artifact.review_threads.push(ReviewThread {
        id: "r_open".to_owned(),
        target: susumu::model::ExpectationTarget::Project,
        subject: None,
        anchor: None,
        parent: None,
        kind: ReviewCommentKind::Comment,
        status: ReviewStatus::Open,
        owner: Some("team-platform".to_owned()),
        source: "human:reviewer".to_owned(),
        title: "Clarify deployment ownership".to_owned(),
        detail: "Who owns the deployment decision?".to_owned(),
    });
    refresh_derived_analysis(&mut artifact);
    let packet = stored_review_packet("fixture.review.susu", 1, &artifact);

    let html = review_portal_html(&packet).expect("portal renders");

    assert!(html.contains("Susumu Review"));
    assert!(html.contains("fixture &lt;/script&gt;"));
    assert!(html.contains("<\\/script>"));
    assert!(html.contains("Support summary"));
    assert!(html.contains("Evidence ladder"));
    assert!(html.contains("Expectation readiness board"));
    assert!(html.contains("Open review workload"));
    assert!(html.contains("team-platform"));
    assert!(html.contains("Search review threads"));
    assert!(html.contains("id=\"threadOwner\""));
    assert!(html.contains("id=\"threadStatus\""));
    assert!(html.contains("renderReviewThreads"));
    assert!(html.contains("This static portal is read-only"));
    assert!(!html.contains("Reply, assign, or resolve this discussion"));
    assert!(html.contains("expectation_readiness"));
    assert!(html.contains("Dirty and stale evidence"));
    assert!(html.contains("data-evidence-ladder"));
    assert!(html.contains("traceability-layout"));
    assert!(html.contains("traceability-list"));
    assert!(html.contains("traceability-detail"));
    assert!(html.contains("overscroll-behavior:contain"));
    assert!(html.contains("padding:8px 6px 0 0"));
    assert!(html.contains("Support reasons"));
    assert!(html.contains(".workflow-layout>*{min-width:0}"));
    assert!(html.contains(".detail-pane{position:sticky;top:98px;align-self:start;min-width:0;max-width:100%;overflow:hidden}"));
    assert!(html.contains("--accent:#9eb7a0"));
    assert!(html.contains("Record verification with susumu verify"));
    assert!(html.contains("POST /checkout"));
    assert!(html.contains("const packet = "));
}

#[test]
fn review_source_previews_embed_syntax_tokens() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source_dir = temp.path().join("src");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::write(
        source_dir.join("api.ts"),
        "export function checkout() {\n  reserveInventory();\n  capturePayment();\n}\n",
    )
    .expect("write source");
    let mut artifact = test_artifact();
    artifact.root = temp.path().display().to_string();
    refresh_derived_analysis(&mut artifact);
    artifact.findings.push(susumu::model::Finding {
        rule_id: "SUS023".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Verification evidence changed".to_owned(),
        detail: "Evidence changed near checkout.".to_owned(),
        file_id: Some("f_api".to_owned()),
        subject: Some("v_checkout".to_owned()),
        location: Some(Location {
            start_line: 2,
            start_column: 3,
            end_line: 2,
            end_column: 15,
        }),
    });

    let previews = crate::review::packet::review_source_previews(&artifact);

    assert!(previews.len() >= 2);
    assert!(previews.iter().any(|preview| preview.path == "src/api.ts"
        && preview.highlight_start == 2
        && preview.highlight_end == 2));
    assert!(
        previews
            .iter()
            .flat_map(|preview| &preview.lines)
            .any(|line| line.text.contains("checkout"))
    );
    assert!(
        previews
            .iter()
            .flat_map(|preview| &preview.lines)
            .any(|line| {
                line.tokens
                    .iter()
                    .any(|token| token.text.contains("checkout"))
            })
    );
    assert!(
        previews
            .iter()
            .flat_map(|preview| &preview.lines)
            .flat_map(|line| &line.tokens)
            .all(|token| token.color.starts_with('#'))
    );
}
#[test]
fn export_review_html_writes_standalone_portal() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut artifact = test_artifact();
    artifact.root = temp
        .path()
        .join("project-without-config")
        .display()
        .to_string();
    refresh_derived_analysis(&mut artifact);
    let packet = stored_review_packet("fixture.review.susu", 1, &artifact);
    let packet_json = serde_json::to_string_pretty(&packet).expect("packet serializes");
    let packet_path = temp.path().join("fixture.review.susu");
    let html_path = temp.path().join("fixture-review.html");
    fs::write(&packet_path, packet_json).expect("write packet");

    export_review_html(&ReviewExportHtmlArgs {
        packet: packet_path,
        output: html_path.clone(),
    })
    .expect("export succeeds");

    let html = fs::read_to_string(html_path).expect("read html");
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("Susumu Review &middot; fixture"));
    assert!(html.contains("const packet = "));
    assert!(html.contains("POST /checkout"));
    assert!(html.contains("Workflow evidence"));
    assert!(html.contains("data-workflow-id"));
    assert!(html.contains("Linked expectations"));
    assert!(html.contains("Expectation readiness board"));
    assert!(html.contains("Has work, needs verification"));
    assert!(html.contains("Expectation traceability"));
    assert!(html.contains("Evidence ladder"));
    assert!(html.contains("Suggested next action"));
    assert!(html.contains("data-expectation-id"));
    assert!(html.contains("data-evidence-ladder"));
    assert!(html.contains("Dirty and stale evidence"));
    assert!(html.contains("Decisions on same target"));
    assert!(html.contains("workflow-layout traceability-layout"));
    assert!(html.contains("class=\"traceability-list\""));
    assert!(html.contains("detail-pane traceability-detail"));
    assert!(html.contains("max-width:100%;min-height:0;overflow:auto"));
    assert!(html.contains("--bg:#11131a"));
}

#[test]
fn review_diff_detects_review_and_artifact_changes() {
    let mut old_artifact = test_artifact();
    refresh_derived_analysis(&mut old_artifact);
    let old = stored_review_packet("old.review.susu", 1, &old_artifact);

    let mut new_artifact = test_artifact();
    refresh_derived_analysis(&mut new_artifact);
    new_artifact.verifications.push(Verification {
        id: "v_checkout_failed".to_owned(),
        expectation_id: "e_checkout_sequence".to_owned(),
        status: VerificationStatus::Failed,
        supersedes: None,
        execution: None,
        chain: None,
        method: "manual review".to_owned(),
        source: "human:qa".to_owned(),
        evidence: Some("review:1".to_owned()),
        basis: None,
        detail: "Checkout order is not acceptable.".to_owned(),
    });
    refresh_derived_analysis(&mut new_artifact);
    let new = stored_review_packet("new.review.susu", 2, &new_artifact);

    let report = review_diff_report(&old, &new);

    assert!(review_diff_regressed(&old, &new));
    assert!(
        report
            .artifact
            .verifications
            .added
            .iter()
            .any(|item| item.contains("v_checkout_failed"))
    );
    assert!(
        report
            .review_items
            .added
            .iter()
            .any(|item| item.contains("failed verification"))
    );
    assert!(
        report
            .next_actions
            .added
            .iter()
            .any(|item| item.contains("failed verification"))
    );
    assert!(
        report
            .top_workflows
            .changed
            .iter()
            .any(|item| item.contains("w_checkout"))
    );
}
