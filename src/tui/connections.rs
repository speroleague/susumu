#![allow(clippy::wildcard_imports)]
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ConnectionCategory {
    BlockedReview,
    NeedsVerification,
    GitBacked,
    Unlinked,
    Recorded,
}

pub(super) struct ConnectionItem<'a> {
    pub(super) category: ConnectionCategory,
    pub(super) work: &'a Work,
}

pub(super) fn render_connections(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = connection_items(&app.analysis);
    let list_items = items
        .iter()
        .map(|item| {
            let color = connection_category_color(item.category);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:18}", connection_category_label(item.category)),
                    Style::default().fg(color),
                ),
                Span::styled(
                    format!(" {:11} ", item.work.status),
                    Style::default().fg(work_status_color(item.work.status)),
                ),
                Span::raw(&item.work.title),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(list_items)
            .block(
                Block::default()
                    .title(" Git and work connections ")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(28, 38, 34))),
        columns[0],
        &mut app.list_state,
    );

    let selected = app.list_state.selected().and_then(|index| items.get(index));
    let detail = selected.map_or_else(
        || {
            vec![
                Line::raw("No work connections in this artifact."),
                Line::raw(""),
                Line::styled(
                    "Run `susumu git connect --artifact project.susu --export-work work.susu`, then open the artifact with `--work work.susu`.",
                    Style::default().fg(Color::DarkGray),
                ),
            ]
        },
        |item| connection_detail_lines(&app.analysis, item),
    );
    let source = selected.and_then(|item| work_source_target(&app.analysis, item.work));
    render_detail_with_source(
        frame,
        &app.analysis,
        detail,
        source,
        columns[1],
        " Connection detail ",
    );
}

pub(super) fn connection_items(analysis: &ProjectAnalysis) -> Vec<ConnectionItem<'_>> {
    let mut items = analysis
        .works
        .iter()
        .map(|work| ConnectionItem {
            category: connection_category(analysis, work),
            work,
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then_with(|| left.work.title.cmp(&right.work.title))
    });
    items
}

pub(super) fn connection_category(analysis: &ProjectAnalysis, work: &Work) -> ConnectionCategory {
    if work.status == WorkStatus::Blocked {
        return ConnectionCategory::BlockedReview;
    }
    if let Some(expectation_id) = work.expectation_id.as_deref()
        && expectation_verifications(analysis, expectation_id).is_empty()
    {
        return ConnectionCategory::NeedsVerification;
    }
    if work
        .evidence
        .as_deref()
        .is_some_and(|evidence| commit_evidence_hash(evidence).is_some())
    {
        return ConnectionCategory::GitBacked;
    }
    if work.expectation_id.is_none() {
        return ConnectionCategory::Unlinked;
    }
    ConnectionCategory::Recorded
}

pub(super) const fn connection_category_label(category: ConnectionCategory) -> &'static str {
    match category {
        ConnectionCategory::BlockedReview => "blocked/review",
        ConnectionCategory::NeedsVerification => "needs verification",
        ConnectionCategory::GitBacked => "git connected",
        ConnectionCategory::Unlinked => "unlinked work",
        ConnectionCategory::Recorded => "recorded",
    }
}

const fn connection_category_color(category: ConnectionCategory) -> Color {
    match category {
        ConnectionCategory::BlockedReview => Color::LightRed,
        ConnectionCategory::NeedsVerification => Color::Yellow,
        ConnectionCategory::GitBacked => Color::LightGreen,
        ConnectionCategory::Unlinked => Color::LightCyan,
        ConnectionCategory::Recorded => Color::DarkGray,
    }
}

pub(super) fn connection_detail_lines(
    analysis: &ProjectAnalysis,
    item: &ConnectionItem<'_>,
) -> Vec<Line<'static>> {
    let work = item.work;
    let expectation = work
        .expectation_id
        .as_deref()
        .map_or_else(|| "-".to_owned(), |id| expectation_title(analysis, id));
    let mut lines = vec![
        Line::styled(work.title.clone(), Style::default().fg(Color::White).bold()),
        Line::raw(""),
        label("category", connection_category_label(item.category)),
        label("id", &work.id),
        label("kind", &work.kind.to_string()),
        label("status", &work.status.to_string()),
        label("target", &work.target.to_string()),
        label(
            "subject",
            &expectation_subject(analysis, work.target, work.subject.as_deref()),
        ),
        label("expectation", &expectation),
        label("source", &work.source),
        label("evidence", work.evidence.as_deref().unwrap_or("-")),
    ];
    if let Some(commit) = work.evidence.as_deref().and_then(commit_evidence_hash) {
        lines.push(label("commit", commit));
    }
    lines.extend([Line::raw(""), section_title("Activity detail")]);
    lines.push(Line::raw(work.detail.clone()));
    append_connection_verifications(analysis, work, &mut lines);
    lines.extend([
        Line::raw(""),
        Line::styled(
            "Enter: jump to work record   b: back",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    lines
}

pub(super) fn append_connection_verifications(
    analysis: &ProjectAnalysis,
    work: &Work,
    lines: &mut Vec<Line<'static>>,
) {
    let Some(expectation_id) = work.expectation_id.as_deref() else {
        return;
    };
    let verifications = expectation_verifications(analysis, expectation_id);
    lines.extend([Line::raw(""), section_title("Linked verifications")]);
    if verifications.is_empty() {
        lines.push(Line::styled(
            "No verification records linked to this expectation yet.",
            Style::default().fg(Color::Yellow),
        ));
        return;
    }
    for verification in verifications {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:12}", verification.status),
                Style::default().fg(verification_color(verification.status)),
            ),
            Span::raw(" "),
            Span::raw(verification.id.clone()),
        ]));
    }
}
