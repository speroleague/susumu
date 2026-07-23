#![allow(clippy::wildcard_imports)]

use super::*;

pub(super) fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let title = Line::from(vec![
        Span::styled(
            " SUSUMU ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            &app.analysis.project_name,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", app.analysis.root),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(title).block(Block::default().borders(Borders::BOTTOM)),
        area,
    );
}

pub(super) fn render_tabs(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let titles = TABS
        .iter()
        .enumerate()
        .map(|(index, title)| {
            if index == TABS.len() - 1 {
                format!(" 0 {title} ")
            } else {
                format!(" {} {title} ", index + 1)
            }
        })
        .collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .select(app.tab)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider("|");
    frame.render_widget(tabs, area);
}

pub(super) fn render_overview(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(area);
    render_overview_metrics(frame, app, columns[0]);
    render_overview_languages(frame, app, columns[1]);
}

pub(super) fn render_overview_metrics(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let resolved = app.analysis.resolved_flow_count();
    let unresolved = app.analysis.flows.len().saturating_sub(resolved);
    let entrypoints = app
        .analysis
        .symbols
        .iter()
        .filter(|symbol| symbol.entrypoint)
        .count();
    let metrics = vec![
        Line::from(vec![
            "  Files       ".into(),
            app.analysis.files.len().to_string().cyan(),
        ]),
        Line::from(vec![
            "  Source      ".into(),
            source_availability_text(&app.analysis).light_cyan(),
        ]),
        Line::from(vec![
            "  Symbols     ".into(),
            app.analysis.symbols.len().to_string().cyan(),
        ]),
        Line::from(vec![
            "  Entrypoints ".into(),
            entrypoints.to_string().cyan(),
        ]),
        Line::from(vec![
            "  Workflows   ".into(),
            app.analysis.workflows.len().to_string().magenta(),
        ]),
        Line::from(vec![
            "  Expectations".into(),
            app.analysis.expectations.len().to_string().light_magenta(),
        ]),
        Line::from(vec![
            "  Verified    ".into(),
            app.analysis.verifications.len().to_string().light_green(),
        ]),
        Line::from(vec![
            "  Decisions   ".into(),
            app.analysis.decisions.len().to_string().light_blue(),
        ]),
        Line::from(vec![
            "  Work        ".into(),
            app.analysis.works.len().to_string().light_cyan(),
        ]),
        Line::from(vec!["  Flows       ".into(), resolved.to_string().green()]),
        Line::from(vec![
            "  Gaps        ".into(),
            unresolved.to_string().yellow(),
        ]),
        Line::from(vec![
            "  Findings    ".into(),
            app.analysis.findings.len().to_string().yellow(),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(metrics).block(
            Block::default()
                .title(" Evidence model ")
                .borders(Borders::ALL),
        ),
        area,
    );
}

pub(super) fn render_overview_languages(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let mut language_lines = Vec::new();
    for (language, count) in app.analysis.language_counts() {
        language_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{language:<12}"), Style::default().fg(Color::White)),
            Span::styled(count.to_string(), Style::default().fg(Color::LightCyan)),
        ]));
    }
    language_lines.push(Line::raw(""));
    language_lines.push(Line::styled(
        "  Exact evidence is connected. Ambiguity stays visible.",
        Style::default().fg(Color::DarkGray),
    ));
    language_lines.push(Line::raw(""));
    language_lines.push(Line::styled(
        "  Top workflows by attention score",
        Style::default().fg(Color::White).bold(),
    ));
    for priority in app.analysis.workflow_priorities.iter().take(3) {
        let workflow = app
            .analysis
            .workflows
            .iter()
            .find(|workflow| workflow.id == priority.workflow_id);
        let trigger = workflow.map_or(priority.workflow_id.as_str(), |workflow| {
            workflow.trigger.as_str()
        });
        language_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{:>3} ", priority.score),
                Style::default().fg(Color::LightMagenta),
            ),
            Span::raw(trigger.to_owned()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(language_lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().title(" Languages ").borders(Borders::ALL)),
        area,
    );
}

#[derive(Debug, Clone)]
pub(super) struct ReviewItem {
    pub(super) severity: ReviewSeverity,
    title: String,
    detail: String,
    pub(super) source: String,
    pub(super) jump: Option<ReviewJump>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ReviewSeverity {
    Attention,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub(super) enum ReviewJump {
    Finding(usize),
    Verification(String),
    Decision(String),
    Work(String),
    Workflow(String),
}

pub(super) fn render_review(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = review_items(&app.analysis);
    let list_items = items
        .iter()
        .map(|item| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:9}", review_severity_label(item.severity)),
                    Style::default().fg(review_severity_color(item.severity)),
                ),
                Span::raw(" "),
                Span::raw(&item.title),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(list_items)
            .block(
                Block::default()
                    .title(" Review queue ")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(42, 33, 25))),
        columns[0],
        &mut app.list_state,
    );

    let detail = app
        .list_state
        .selected()
        .and_then(|index| items.get(index))
        .map_or_else(
            || {
                vec![
                    Line::raw("No review items."),
                    Line::raw(""),
                    Line::styled(
                        "This does not prove the project is correct; it means no current review signals were derived.",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]
            },
            |item| {
                vec![
                    Line::styled(item.title.clone(), Style::default().fg(Color::White).bold()),
                    Line::raw(""),
                    Line::raw(item.detail.clone()),
                    Line::raw(""),
                    label("severity", review_severity_label(item.severity)),
                    label("source", &item.source),
                    Line::raw(""),
                    Line::styled(
                        if item.jump.is_some() {
                            "Enter: jump to record   b: back"
                        } else {
                            "No direct jump target for this item."
                        },
                        Style::default().fg(Color::DarkGray),
                    ),
                ]
            },
        );
    frame.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(" Review detail ")
                .borders(Borders::ALL),
        ),
        columns[1],
    );
}

pub(super) fn review_items(analysis: &ProjectAnalysis) -> Vec<ReviewItem> {
    let mut items = Vec::new();
    add_finding_review_items(analysis, &mut items);
    add_verification_review_items(analysis, &mut items);
    add_connection_review_items(analysis, &mut items);
    add_workflow_gap_review_items(analysis, &mut items);
    items.sort_by(|left, right| {
        right
            .severity
            .cmp(&left.severity)
            .then_with(|| left.title.cmp(&right.title))
    });
    items
}

pub(super) fn add_finding_review_items(analysis: &ProjectAnalysis, items: &mut Vec<ReviewItem>) {
    for (index, finding) in analysis.findings.iter().enumerate() {
        let severity = match finding.severity {
            Severity::Error => ReviewSeverity::Critical,
            Severity::Warning => ReviewSeverity::Warning,
            Severity::Info if matches!(finding.rule_id.as_str(), "SUS023" | "SUS033") => {
                ReviewSeverity::Warning
            }
            Severity::Info => continue,
        };
        let jump = if finding.rule_id == "SUS023" {
            finding
                .subject
                .as_deref()
                .map(|id| ReviewJump::Verification(id.to_owned()))
        } else if finding.rule_id == "SUS033" {
            finding
                .subject
                .as_deref()
                .map(|id| ReviewJump::Decision(id.to_owned()))
        } else if matches!(
            finding.rule_id.as_str(),
            "SUS040" | "SUS041" | "SUS042" | "SUS043"
        ) {
            finding
                .subject
                .as_deref()
                .map(|id| ReviewJump::Work(id.to_owned()))
        } else {
            Some(ReviewJump::Finding(index))
        };
        items.push(ReviewItem {
            severity,
            title: format!("{}: {}", finding.rule_id, finding.title),
            detail: finding.detail.clone(),
            source: finding.source.clone(),
            jump,
        });
    }
}

pub(super) fn add_verification_review_items(
    analysis: &ProjectAnalysis,
    items: &mut Vec<ReviewItem>,
) {
    for verification in &analysis.verifications {
        let severity = match verification.status {
            VerificationStatus::Failed => ReviewSeverity::Critical,
            VerificationStatus::Inconclusive => ReviewSeverity::Warning,
            VerificationStatus::Passed => continue,
        };
        items.push(ReviewItem {
            severity,
            title: format!(
                "{} verification: {}",
                verification.status,
                expectation_title(analysis, &verification.expectation_id)
            ),
            detail: verification.detail.clone(),
            source: verification.source.clone(),
            jump: Some(ReviewJump::Verification(verification.id.clone())),
        });
    }
}

pub(super) fn add_connection_review_items(analysis: &ProjectAnalysis, items: &mut Vec<ReviewItem>) {
    for item in connection_items(analysis) {
        let severity = match item.category {
            ConnectionCategory::BlockedReview | ConnectionCategory::NeedsVerification => {
                ReviewSeverity::Warning
            }
            ConnectionCategory::GitBacked
            | ConnectionCategory::Unlinked
            | ConnectionCategory::Recorded => continue,
        };
        items.push(ReviewItem {
            severity,
            title: format!(
                "{}: {}",
                connection_category_label(item.category),
                item.work.title
            ),
            detail: format!(
                "Work record {} is {}. Review the connection and add verification or follow-up records as needed.",
                item.work.id,
                connection_category_label(item.category)
            ),
            source: "susumu:tui".to_owned(),
            jump: Some(ReviewJump::Work(item.work.id.clone())),
        });
    }
}

pub(super) fn add_workflow_gap_review_items(
    analysis: &ProjectAnalysis,
    items: &mut Vec<ReviewItem>,
) {
    for workflow in &analysis.workflows {
        let Some(entry_symbol) = workflow.entry_symbol.as_deref() else {
            continue;
        };
        let gaps = analysis
            .flows
            .iter()
            .filter(|flow| {
                flow.from == entry_symbol
                    && flow.to.is_none()
                    && flow.confidence != Confidence::External
            })
            .count();
        if gaps == 0 {
            continue;
        }
        let label = if gaps == 1 { "edge" } else { "edges" };
        items.push(ReviewItem {
            severity: ReviewSeverity::Attention,
            title: format!("{} has unresolved call {label}", workflow.trigger),
            detail: format!(
                "{} has {gaps} unresolved outgoing call {label}. This may be framework, library, generated, or dynamic behavior.",
                workflow.trigger
            ),
            source: "susumu:derived".to_owned(),
            jump: Some(ReviewJump::Workflow(workflow.id.clone())),
        });
    }
}

const fn review_severity_label(severity: ReviewSeverity) -> &'static str {
    match severity {
        ReviewSeverity::Critical => "critical",
        ReviewSeverity::Warning => "warning",
        ReviewSeverity::Attention => "attention",
    }
}

const fn review_severity_color(severity: ReviewSeverity) -> Color {
    match severity {
        ReviewSeverity::Critical => Color::LightRed,
        ReviewSeverity::Warning => Color::Yellow,
        ReviewSeverity::Attention => Color::LightCyan,
    }
}
