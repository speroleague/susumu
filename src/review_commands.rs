#![allow(clippy::wildcard_imports)]

use super::diff_commands::{
    ChangeSummary, ReviewDiffReport, change_summary_json, diff_by, print_change_section,
    print_freshness_section,
};
use super::*;

pub(super) fn create_review(args: &ReviewCreateArgs) -> Result<()> {
    let analysis = load_analysis(
        &args.target,
        args.expectations.as_ref(),
        args.verifications.as_ref(),
        args.decisions.as_ref(),
        args.work.as_ref(),
        false,
    )?;
    let check = check_report(&analysis, args.strict);
    let handoff = handoff_report(&analysis, &check);
    let packet = review_packet(
        args.target.display().to_string(),
        current_unix_seconds(),
        &analysis,
        &check,
        &handoff,
    );
    let packet_json =
        serde_json::to_string_pretty(&packet).context("could not serialize review packet")?;

    if let Some(output) = &args.output {
        fs::write(output, &packet_json)
            .with_context(|| format!("could not write {}", output.display()))?;
        eprintln!("wrote review packet {}", output.display());
    }

    if args.json || args.output.is_none() {
        println!("{packet_json}");
    } else {
        println!("Susumu review packet: {}", analysis.project_name);
        println!("Root: {}", analysis.root);
        println!(
            "Review: {} critical, {} warning, {} attention ({})",
            check.critical,
            check.warning,
            check.attention,
            check_result_reason(&check)
        );
        println!("Top workflows: {}", handoff.top_workflows.len());
        println!("Suggested next actions: {}", handoff.next_actions.len());
    }

    Ok(())
}

pub(super) fn build_review(args: &ReviewBuildArgs) -> Result<()> {
    let state = write_review_build_outputs(args)?;
    print_review_build_summary(args, &state)?;

    if args.serve {
        serve_review(&ReviewServeArgs {
            packet: args.output.clone(),
            host: args.host.clone(),
            port: args.port,
        })?;
    }

    if args.fail_on_check && state.check.failed {
        process::exit(1);
    }
    Ok(())
}

fn write_review_build_outputs(args: &ReviewBuildArgs) -> Result<ReviewBuildState> {
    let analysis = load_analysis(
        &args.target,
        args.expectations.as_ref(),
        args.verifications.as_ref(),
        args.decisions.as_ref(),
        args.work.as_ref(),
        true,
    )?;
    write_text_file(&args.artifact_output, &write_susu(&analysis, false)?)?;

    let check = check_report(&analysis, args.strict);
    write_review_build_check_json(args, &analysis, &check)?;
    write_review_build_packet(args, &analysis, &check)?;

    Ok(ReviewBuildState {
        project_name: analysis.project_name,
        check,
    })
}

fn write_review_build_check_json(
    args: &ReviewBuildArgs,
    analysis: &ProjectAnalysis,
    check: &CheckReport,
) -> Result<()> {
    let Some(check_output) = &args.check_json else {
        return Ok(());
    };
    let check_json = check_json(analysis, check);
    write_text_file(
        check_output,
        &serde_json::to_string_pretty(&check_json).context("could not serialize check report")?,
    )
}

fn write_review_build_packet(
    args: &ReviewBuildArgs,
    analysis: &ProjectAnalysis,
    check: &CheckReport,
) -> Result<()> {
    let handoff = handoff_report(analysis, check);
    let packet = review_packet(
        args.target.display().to_string(),
        current_unix_seconds(),
        analysis,
        check,
        &handoff,
    );
    let packet_json =
        serde_json::to_string_pretty(&packet).context("could not serialize review packet")?;
    write_text_file(&args.output, &packet_json)?;
    write_review_build_html(args, &packet_json)
}

fn write_review_build_html(args: &ReviewBuildArgs, packet_json: &str) -> Result<()> {
    let Some(html_output) = &args.html else {
        return Ok(());
    };
    let stored_packet: ReviewPacketStored =
        serde_json::from_str(packet_json).context("could not read built review packet")?;
    let config = load_portal_config_for_target(&args.target)?;
    write_text_file(
        html_output,
        &review_portal_html_with_config(&stored_packet, &config)?,
    )
}

fn print_review_build_summary(args: &ReviewBuildArgs, state: &ReviewBuildState) -> Result<()> {
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "project": state.project_name,
                "artifact": args.artifact_output,
                "review_packet": args.output,
                "check_json": args.check_json,
                "html": args.html,
                "result": {
                    "status": if state.check.failed { "failed" } else { "passed" },
                    "failed": state.check.failed,
                    "strict": state.check.strict,
                    "reason": check_result_reason(&state.check),
                },
                "review": {
                    "critical": state.check.critical,
                    "warning": state.check.warning,
                    "attention": state.check.attention,
                }
            }))
            .context("could not serialize review build summary")?
        );
    } else {
        println!("Susumu review build: {}", state.project_name);
        println!("Artifact: {}", args.artifact_output.display());
        println!("Review packet: {}", args.output.display());
        if let Some(check_output) = &args.check_json {
            println!("Check JSON: {}", check_output.display());
        }
        if let Some(html_output) = &args.html {
            println!("HTML portal: {}", html_output.display());
        }
        println!(
            "Review: {} critical, {} warning, {} attention ({})",
            state.check.critical,
            state.check.warning,
            state.check.attention,
            check_result_reason(&state.check)
        );
    }

    Ok(())
}

pub(super) fn open_review(args: &ReviewOpenArgs) -> Result<()> {
    let packet = read_review_packet(&args.packet)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&packet).context("could not serialize review packet")?
        );
        return Ok(());
    }
    if args.tui {
        return tui::run(packet.artifact, None);
    }
    print_review_packet(&packet, args.max_items);
    Ok(())
}

pub(super) fn diff_reviews(args: &ReviewDiffArgs) -> Result<()> {
    let old = read_review_packet(&args.old)?;
    let new = read_review_packet(&args.new)?;
    let report = review_diff_report(&old, &new);
    if args.json {
        print_review_diff_json(args, &old, &new, &report)?;
    } else {
        print_review_diff(&old, &new, &report, args.max_items);
    }
    if args.fail_on_regression && review_diff_regressed(&old, &new) {
        process::exit(1);
    }
    Ok(())
}

pub(super) fn serve_review(args: &ReviewServeArgs) -> Result<()> {
    let packet = read_review_packet(&args.packet)?;
    let config = load_portal_config_for_packet(&packet, &args.packet)?;
    let html = review_portal_html_with_config(&packet, &config)?;
    let packet_json =
        serde_json::to_string_pretty(&packet).context("could not serialize review packet")?;
    let listener =
        TcpListener::bind(format!("{}:{}", args.host, args.port)).with_context(|| {
            format!(
                "could not bind review server to {}:{}",
                args.host, args.port
            )
        })?;
    let address = listener
        .local_addr()
        .context("could not read review server address")?;
    println!("Susumu review portal: http://{address}/");
    println!("Serving {}", args.packet.display());
    println!("Press Ctrl+C to stop.");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_review_request(stream, &html, &packet_json) {
                    eprintln!("warning: could not serve review request: {error}");
                }
            }
            Err(error) => eprintln!("warning: review server connection failed: {error}"),
        }
    }
    Ok(())
}

pub(super) fn export_review_html(args: &ReviewExportHtmlArgs) -> Result<()> {
    let packet = read_review_packet(&args.packet)?;
    let config = load_portal_config_for_packet(&packet, &args.packet)?;
    let html = review_portal_html_with_config(&packet, &config)?;
    fs::write(&args.output, html)
        .with_context(|| format!("could not write {}", args.output.display()))?;
    println!("wrote review portal {}", args.output.display());
    Ok(())
}

pub(super) fn read_review_packet(path: &PathBuf) -> Result<ReviewPacketStored> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let mut packet = serde_json::from_str::<ReviewPacketStored>(&source)
        .with_context(|| format!("could not parse review packet {}", path.display()))?;
    if packet.schema_version != "susumu.review.v1" {
        bail!(
            "{} uses unsupported review schema `{}`",
            path.display(),
            packet.schema_version
        );
    }
    refresh_derived_analysis(&mut packet.artifact);
    Ok(packet)
}

#[allow(clippy::too_many_lines)]
pub(super) fn review_diff_report(
    old: &ReviewPacketStored,
    new: &ReviewPacketStored,
) -> ReviewDiffReport {
    ReviewDiffReport {
        artifact: diff_report(&old.artifact, &new.artifact),
        review_items: diff_by(
            &old.review_items,
            &new.review_items,
            review_item_key,
            review_item_label,
        ),
        next_actions: diff_strings(&old.next_actions, &new.next_actions),
        top_workflows: diff_by(
            &old.top_workflows,
            &new.top_workflows,
            |workflow| workflow.id.clone(),
            |workflow| {
                format!(
                    "{} - {} ({})",
                    workflow.id, workflow.trigger, workflow.framework
                )
            },
        ),
    }
}

fn diff_strings(old: &[String], new: &[String]) -> ChangeSummary {
    diff_by(old, new, Clone::clone, Clone::clone)
}

fn review_item_key(item: &ReviewItemStored) -> String {
    format!("{}|{}|{}", item.severity, item.title, item.source)
}

fn review_item_label(item: &ReviewItemStored) -> String {
    format!("[{}] {} ({})", item.severity, item.title, item.source)
}

pub(super) fn review_diff_regressed(old: &ReviewPacketStored, new: &ReviewPacketStored) -> bool {
    (!old.result.failed && new.result.failed) || new.review.critical > old.review.critical
}

fn print_review_packet(packet: &ReviewPacketStored, max_items: usize) {
    println!("Susumu review packet: {}", packet.project.name);
    println!("Schema: {}", packet.schema_version);
    println!("Created: {}", packet.created_unix_seconds);
    println!("Source: {}", packet.source.input);
    println!("Root: {}", packet.project.root);
    println!();
    println!(
        "Evidence: {} files, {} workflows, {} flows, {} findings",
        packet.evidence.files,
        packet.evidence.workflows,
        packet.evidence.flows,
        packet.evidence.findings
    );
    println!(
        "Records: {} expectations, {} verifications, {} decisions, {} work",
        packet.records.expectations,
        packet.records.verifications,
        packet.records.decisions,
        packet.records.work
    );
    println!(
        "Review: {} critical, {} warning, {} attention",
        packet.review.critical, packet.review.warning, packet.review.attention
    );
    println!(
        "Result: {} ({})",
        packet.result.status, packet.result.reason
    );
    println!();
    print_handoff_workflows(&packet.top_workflows, max_items);
    print_stored_review_items(&packet.review_items, max_items);
    readiness_command::print_items(&packet.expectation_readiness, max_items);
    print_expectation_support(&packet.expectation_support, max_items);
    print_handoff_records(
        "Expectations without verification",
        &packet.expectations_without_verification,
        max_items,
    );
    print_handoff_records(
        "Work needing verification",
        &packet.work_needing_verification,
        max_items,
    );
    print_string_section("Caveats", &packet.caveats, max_items);
    print_string_section("Suggested next actions", &packet.next_actions, max_items);
}

fn print_expectation_support(items: &[ExpectationSupport], max_items: usize) {
    println!();
    println!("Expectation support");
    if items.is_empty() {
        println!("  none");
        return;
    }
    for item in items.iter().take(max_items) {
        println!(
            "  - {} [{}] target={}{}",
            item.title,
            item.support_status,
            item.target,
            item.subject
                .as_ref()
                .map_or_else(String::new, |subject| format!(":{subject}"))
        );
        println!(
            "    observed={} verifications={}/{}/{} posture={} work={} decisions={} findings={}",
            item.target_observed,
            item.verification.passed,
            item.verification.failed,
            item.verification.inconclusive,
            item.evidence_posture,
            item.work,
            item.decisions,
            item.findings
        );
        for reason in item.reasons.iter().take(3) {
            println!("    reason: {reason}");
        }
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
}

fn print_stored_review_items(items: &[ReviewItemStored], max_items: usize) {
    println!("Needs review:");
    if items.is_empty() {
        println!("  none derived");
    }
    for item in items.iter().take(max_items) {
        println!("  [{}] {}", item.severity, item.title);
        println!("      source={}", item.source);
        println!("      {}", item.detail);
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
    println!();
}

fn print_review_diff(
    old: &ReviewPacketStored,
    new: &ReviewPacketStored,
    report: &ReviewDiffReport,
    max_items: usize,
) {
    println!(
        "Susumu review diff: {} -> {}",
        old.project.name, new.project.name
    );
    println!("Old created: {}", old.created_unix_seconds);
    println!("New created: {}", new.created_unix_seconds);
    println!("Result: {} -> {}", old.result.status, new.result.status);
    println!(
        "Review: critical {}->{}, warning {}->{}, attention {}->{}",
        old.review.critical,
        new.review.critical,
        old.review.warning,
        new.review.warning,
        old.review.attention,
        new.review.attention
    );
    if review_diff_regressed(old, new) {
        println!("Regression: yes");
    } else {
        println!("Regression: no");
    }
    println!();
    print_change_section("Review items", &report.review_items, max_items);
    print_change_section("Next actions", &report.next_actions, max_items);
    print_change_section("Top workflows", &report.top_workflows, max_items);
    println!("Embedded artifact changes:");
    print_change_section("Files", &report.artifact.files, max_items);
    print_change_section("Workflows", &report.artifact.workflows, max_items);
    print_change_section("Expectations", &report.artifact.expectations, max_items);
    print_change_section("Verifications", &report.artifact.verifications, max_items);
    print_change_section("Decisions", &report.artifact.decisions, max_items);
    print_change_section("Work", &report.artifact.works, max_items);
    print_freshness_section(&report.artifact.stale_items, max_items);
}

fn print_review_diff_json(
    args: &ReviewDiffArgs,
    old: &ReviewPacketStored,
    new: &ReviewPacketStored,
    report: &ReviewDiffReport,
) -> Result<()> {
    let regressed = review_diff_regressed(old, new);
    let output = serde_json::json!({
        "old": review_packet_summary_json(old),
        "new": review_packet_summary_json(new),
        "changes": {
            "review_items": change_summary_json(&report.review_items),
            "next_actions": change_summary_json(&report.next_actions),
            "top_workflows": change_summary_json(&report.top_workflows),
            "artifact": {
                "files": change_summary_json(&report.artifact.files),
                "workflows": change_summary_json(&report.artifact.workflows),
                "expectations": change_summary_json(&report.artifact.expectations),
                "verifications": change_summary_json(&report.artifact.verifications),
                "decisions": change_summary_json(&report.artifact.decisions),
                "work": change_summary_json(&report.artifact.works),
            },
        },
        "freshness": {
            "stale": report.artifact.stale_items.len(),
            "items": check_item_jsons(&report.artifact.stale_items),
        },
        "result": {
            "status": if args.fail_on_regression && regressed { "failed" } else { "passed" },
            "failed": args.fail_on_regression && regressed,
            "fail_on_regression": args.fail_on_regression,
            "regressed": regressed,
            "reason": review_diff_result_reason(args.fail_on_regression, regressed),
        },
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize review diff")?
    );
    Ok(())
}

fn review_packet_summary_json(packet: &ReviewPacketStored) -> serde_json::Value {
    serde_json::json!({
        "schema_version": &packet.schema_version,
        "created_unix_seconds": packet.created_unix_seconds,
        "source": &packet.source,
        "project": &packet.project,
        "evidence": &packet.evidence,
        "records": &packet.records,
        "review": &packet.review,
        "result": &packet.result,
    })
}

const fn review_diff_result_reason(fail_on_regression: bool, regressed: bool) -> &'static str {
    if fail_on_regression && regressed {
        "review regression present"
    } else if regressed {
        "passed with review regression"
    } else {
        "passed"
    }
}
