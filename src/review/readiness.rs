use std::{
    fs,
    path::{Path, PathBuf},
};

use super::types::{ExpectationReadiness, READINESS_BUCKETS, ReviewPacketStored};
use anyhow::{Context, Result};
use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct ReadinessArgs {
    /// Review packet to inspect.
    #[arg(
        short,
        long,
        default_value = ".susumu/review.susu",
        value_name = "FILE"
    )]
    pub(crate) packet: PathBuf,

    /// Maximum readiness items to print.
    #[arg(long, default_value_t = 20)]
    pub(crate) max_items: usize,

    /// Filter by readiness bucket.
    #[arg(long, value_name = "BUCKET")]
    pub(crate) bucket: Option<String>,

    /// Search expectation id, title, target, subject, label, or status.
    #[arg(short, long)]
    pub(crate) search: Option<String>,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

pub(crate) fn run(args: &ReadinessArgs) -> Result<()> {
    let packet = read_packet(&args.packet).with_context(|| {
        format!(
            "could not read readiness from {}; run `susumu review` first",
            args.packet.display()
        )
    })?;
    let bucket = canonical_bucket(args.bucket.as_deref())?;
    let items = filtered_items(
        &packet.expectation_readiness,
        bucket,
        args.search.as_deref(),
    );
    if args.json {
        print_json(
            &args.packet,
            &packet,
            &items,
            bucket,
            args.search.as_deref(),
        )?;
    } else {
        print_report(
            &args.packet,
            &packet,
            &items,
            bucket,
            args.search.as_deref(),
            args.max_items,
        );
    }
    Ok(())
}

fn read_packet(path: &Path) -> Result<ReviewPacketStored> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("could not read review packet {}", path.display()))?;
    serde_json::from_str(&source)
        .with_context(|| format!("could not parse review packet {}", path.display()))
}

fn print_report(
    packet_path: &Path,
    packet: &ReviewPacketStored,
    items: &[ExpectationReadiness],
    bucket: Option<&str>,
    search: Option<&str>,
    max_items: usize,
) {
    println!("Susumu readiness: {}", packet.project.name);
    println!("Packet: {}", packet_path.display());
    println!(
        "Result: {} ({})",
        packet.result.status, packet.result.reason
    );
    println!(
        "Showing: {} of {} expectations",
        items.len(),
        packet.expectation_readiness.len()
    );
    if bucket.is_some() || search.is_some() {
        println!(
            "Filters: bucket={} search={}",
            bucket.unwrap_or("any"),
            search.unwrap_or("any")
        );
    }
    println!();
    print_counts(items);
    println!();
    print_items(items, max_items);
}

fn print_counts(items: &[ExpectationReadiness]) {
    println!("Readiness counts");
    for (bucket, label) in READINESS_BUCKETS {
        let count = items.iter().filter(|item| item.bucket == bucket).count();
        println!("  - {label}: {count}");
    }
}

pub(crate) fn print_items(items: &[ExpectationReadiness], max_items: usize) {
    println!("Expectation readiness");
    if items.is_empty() {
        println!("  none");
        return;
    }
    for item in items.iter().take(max_items) {
        println!(
            "  - {} [{}] {}",
            item.title, item.label, item.expectation_id
        );
        println!("    next: {}", item.next_action);
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
}

fn print_json(
    packet_path: &Path,
    packet: &ReviewPacketStored,
    items: &[ExpectationReadiness],
    bucket: Option<&str>,
    search: Option<&str>,
) -> Result<()> {
    let output = readiness_json(packet_path, packet, items, bucket, search);
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize readiness report")?
    );
    Ok(())
}

pub(crate) fn readiness_json(
    packet_path: &Path,
    packet: &ReviewPacketStored,
    items: &[ExpectationReadiness],
    bucket: Option<&str>,
    search: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "packet": packet_path.display().to_string(),
        "project": &packet.project,
        "result": &packet.result,
        "total": packet.expectation_readiness.len(),
        "shown": items.len(),
        "filters": {"bucket": bucket, "search": search},
        "counts": READINESS_BUCKETS.iter().map(|(bucket, label)| serde_json::json!({
            "bucket": bucket,
            "label": label,
            "count": items.iter().filter(|item| item.bucket == *bucket).count(),
        })).collect::<Vec<_>>(),
        "items": items,
    })
}

pub(crate) fn canonical_bucket(bucket: Option<&str>) -> Result<Option<&'static str>> {
    let Some(bucket) = bucket else {
        return Ok(None);
    };
    let normalized = normalize_filter(bucket);
    let canonical = READINESS_BUCKETS
        .iter()
        .find(|(candidate, label)| {
            normalize_filter(candidate) == normalized || normalize_filter(label) == normalized
        })
        .map(|(candidate, _)| *candidate);
    canonical.map(Some).with_context(|| {
        format!(
            "unknown readiness bucket `{bucket}`; expected one of: {}",
            READINESS_BUCKETS
                .iter()
                .map(|(bucket, _)| *bucket)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

fn normalize_filter(value: &str) -> String {
    let mut normalized = String::new();
    let mut separator = false;
    for ch in value.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            separator = false;
        } else if !separator && !normalized.is_empty() {
            normalized.push('_');
            separator = true;
        }
    }
    if normalized.ends_with('_') {
        normalized.pop();
    }
    normalized
}

pub(crate) fn filtered_items(
    items: &[ExpectationReadiness],
    bucket: Option<&str>,
    search: Option<&str>,
) -> Vec<ExpectationReadiness> {
    let search = search.map(str::to_lowercase);
    items
        .iter()
        .filter(|item| bucket.is_none_or(|bucket| item.bucket == bucket))
        .filter(|item| {
            search
                .as_deref()
                .is_none_or(|search| matches_search(item, search))
        })
        .cloned()
        .collect()
}

fn matches_search(item: &ExpectationReadiness, search: &str) -> bool {
    [
        item.expectation_id.as_str(),
        item.title.as_str(),
        item.target.as_str(),
        item.subject.as_deref().unwrap_or_default(),
        item.bucket.as_str(),
        item.label.as_str(),
        item.support_status.as_str(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(search))
}
