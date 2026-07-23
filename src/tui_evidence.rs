#![allow(clippy::wildcard_imports)]

use super::*;

pub(super) fn render_workflows(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let workflow_order = workflow_order(&app.analysis);
    let items = workflow_order
        .iter()
        .filter_map(|index| app.analysis.workflows.get(*index))
        .map(|workflow| {
            let priority = workflow_priority(&app.analysis, &workflow.id);
            let color = match workflow.confidence {
                Confidence::Exact => Color::Green,
                Confidence::Likely => Color::LightCyan,
                Confidence::Ambiguous => Color::Yellow,
                Confidence::External => Color::DarkGray,
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>3} ", priority.map_or(0, |priority| priority.score)),
                    Style::default().fg(Color::LightMagenta),
                ),
                Span::styled(
                    format!("{:20}", workflow.trigger),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(&workflow.framework, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" Detected workflows ")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(45, 27, 52))),
        columns[0],
        &mut app.list_state,
    );

    let selected_workflow = app
        .list_state
        .selected()
        .and_then(|selected| workflow_order.get(selected))
        .and_then(|index| app.analysis.workflows.get(*index));
    let detail = selected_workflow.map_or_else(
        || {
            vec![
                Line::raw("No framework workflows detected."),
                Line::raw(""),
                Line::styled(
                    "Language adapters can add routes, jobs, events, and other triggers without changing the TUI.",
                    Style::default().fg(Color::DarkGray),
                ),
            ]
        },
        |workflow| workflow_detail_lines(&app.analysis, workflow),
    );
    let source = selected_workflow.map(workflow_source_target);
    render_detail_with_source(
        frame,
        &app.analysis,
        detail,
        source,
        columns[1],
        " Workflow evidence ",
    );
}

pub(super) fn workflow_detail_lines(
    analysis: &ProjectAnalysis,
    workflow: &Workflow,
) -> Vec<Line<'static>> {
    let priority = workflow_priority(analysis, &workflow.id);
    let file = analysis
        .files
        .iter()
        .find(|file| file.id == workflow.file_id)
        .map_or("unknown", |file| file.path.as_str());
    let entry = workflow
        .entry_symbol
        .as_deref()
        .map_or_else(|| "unresolved".to_owned(), |id| symbol_name(analysis, id));
    let mut lines = vec![
        Line::styled(
            workflow.trigger.clone(),
            Style::default().fg(Color::LightMagenta).bold(),
        ),
        Line::raw(""),
        label("kind", &workflow.kind.to_string()),
        label("framework", &workflow.framework),
        label(
            "handler",
            workflow.handler.as_deref().unwrap_or("inline/dynamic"),
        ),
        label("entry", &entry),
        label("file", file),
        label("location", &workflow.location.start_token()),
        label("confidence", &workflow.confidence.to_string()),
        label(
            "attention source",
            priority.map_or("not scored", |priority| priority.source.as_str()),
        ),
        label(
            "attention",
            &priority.map_or_else(|| "0".to_owned(), |priority| priority.score.to_string()),
        ),
        label(
            "reasons",
            priority.map_or("not scored", |priority| priority.detail.as_str()),
        ),
        Line::raw(""),
        Line::styled(
            confidence_explanation(workflow.confidence),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    append_workflow_traceability(analysis, workflow, &mut lines);
    append_workflow_work(analysis, workflow, &mut lines);
    append_workflow_decisions(analysis, workflow, &mut lines);
    lines
}

pub(super) fn append_workflow_traceability(
    analysis: &ProjectAnalysis,
    workflow: &Workflow,
    lines: &mut Vec<Line<'static>>,
) {
    let expectations = workflow_expectations(analysis, &workflow.id);
    lines.extend([Line::raw(""), section_title("Linked expectations")]);
    if expectations.is_empty() {
        lines.push(Line::styled(
            "No expectation records target this workflow.",
            Style::default().fg(Color::DarkGray),
        ));
        return;
    }

    for expectation in expectations {
        lines.extend(expectation_trace_lines(analysis, expectation));
    }
}

pub(super) fn expectation_trace_lines(
    analysis: &ProjectAnalysis,
    expectation: &Expectation,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{:10}", expectation.status),
                Style::default().fg(expectation_color(expectation.status)),
            ),
            Span::raw(" "),
            Span::styled(
                expectation.title.clone(),
                Style::default().fg(Color::White).bold(),
            ),
        ]),
        Line::styled(
            format!("source={} id={}", expectation.source, expectation.id),
            Style::default().fg(Color::DarkGray),
        ),
    ];

    let verifications = expectation_verifications(analysis, &expectation.id);
    if verifications.is_empty() {
        lines.push(Line::styled(
            "verification: no linked records",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for verification in verifications {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("verification {:12}", verification.status),
                    Style::default().fg(verification_color(verification.status)),
                ),
                Span::raw(format!(
                    " {} source={} evidence={}",
                    verification.id,
                    verification.source,
                    verification.evidence.as_deref().unwrap_or("-")
                )),
            ]));
        }
    }
    lines.push(Line::raw(""));
    lines
}

pub(super) fn append_workflow_decisions(
    analysis: &ProjectAnalysis,
    workflow: &Workflow,
    lines: &mut Vec<Line<'static>>,
) {
    let decisions = workflow_decisions(analysis, &workflow.id);
    lines.extend([Line::raw(""), section_title("Linked decisions")]);
    if decisions.is_empty() {
        lines.push(Line::styled(
            "No decision records target this workflow.",
            Style::default().fg(Color::DarkGray),
        ));
        return;
    }

    for decision in decisions {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:10}", decision.status),
                Style::default().fg(decision_color(decision.status)),
            ),
            Span::raw(" "),
            Span::styled(
                decision.title.clone(),
                Style::default().fg(Color::White).bold(),
            ),
        ]));
        lines.push(Line::styled(
            format!(
                "source={} id={} basis={}",
                decision.source,
                decision.id,
                decision.basis.as_deref().unwrap_or("-")
            ),
            Style::default().fg(Color::DarkGray),
        ));
    }
}

pub(super) fn append_workflow_work(
    analysis: &ProjectAnalysis,
    workflow: &Workflow,
    lines: &mut Vec<Line<'static>>,
) {
    let works = workflow_works(analysis, &workflow.id);
    lines.extend([Line::raw(""), section_title("Linked work")]);
    if works.is_empty() {
        lines.push(Line::styled(
            "No work records target this workflow.",
            Style::default().fg(Color::DarkGray),
        ));
        return;
    }

    for work in works {
        let expectation = work
            .expectation_id
            .as_deref()
            .map_or_else(|| "-".to_owned(), |id| expectation_title(analysis, id));
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:11}", work.status),
                Style::default().fg(work_status_color(work.status)),
            ),
            Span::raw(" "),
            Span::styled(work.title.clone(), Style::default().fg(Color::White).bold()),
        ]));
        lines.push(Line::styled(
            format!(
                "kind={} source={} evidence={} expectation={} id={}",
                work.kind,
                work.source,
                work.evidence.as_deref().unwrap_or("-"),
                expectation,
                work.id
            ),
            Style::default().fg(Color::DarkGray),
        ));
    }
}

pub(super) fn render_flows(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = app
        .analysis
        .flows
        .iter()
        .map(|flow| {
            let from = symbol_name(&app.analysis, &flow.from);
            let target = flow.to.as_deref().map_or_else(
                || format!("? {}", flow.call),
                |id| symbol_name(&app.analysis, id),
            );
            let color = match flow.confidence {
                Confidence::Exact => Color::Green,
                Confidence::Likely => Color::LightCyan,
                Confidence::Ambiguous => Color::Yellow,
                Confidence::External => Color::DarkGray,
            };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{from} ")),
                Span::styled("->", Style::default().fg(color)),
                Span::raw(format!(" {target}")),
            ]))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().title(" Call flow ").borders(Borders::ALL))
        .highlight_symbol("> ")
        .highlight_style(Style::default().bg(Color::Rgb(25, 45, 52)));
    frame.render_stateful_widget(list, columns[0], &mut app.list_state);

    let selected_flow = app
        .list_state
        .selected()
        .and_then(|index| app.analysis.flows.get(index));
    let detail = selected_flow.map_or_else(
        || vec![Line::raw("No flows found.")],
        |flow| {
            let file = flow_file(&app.analysis, flow).map_or("unknown", |file| file.path.as_str());
            vec![
                Line::styled(
                    flow.call.clone(),
                    Style::default().fg(Color::LightCyan).bold(),
                ),
                Line::raw(""),
                label("file", file),
                label("location", &flow.location.start_token()),
                label("confidence", &flow.confidence.to_string()),
                Line::raw(""),
                Line::styled(
                    confidence_explanation(flow.confidence),
                    Style::default().fg(Color::DarkGray),
                ),
            ]
        },
    );
    let source = selected_flow.and_then(|flow| flow_source_target(&app.analysis, flow));
    render_detail_with_source(
        frame,
        &app.analysis,
        detail,
        source,
        columns[1],
        " Evidence ",
    );
}

pub(super) fn render_findings(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = app
        .analysis
        .findings
        .iter()
        .map(|finding| {
            let color = match finding.severity {
                Severity::Error => Color::LightRed,
                Severity::Warning => Color::Yellow,
                Severity::Info => Color::LightCyan,
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:7}", finding.severity),
                    Style::default().fg(color),
                ),
                Span::raw(format!(" {}  {}", finding.rule_id, finding.title)),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" Deterministic findings ")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(52, 45, 25))),
        columns[0],
        &mut app.list_state,
    );

    let selected_finding = app
        .list_state
        .selected()
        .and_then(|index| app.analysis.findings.get(index));
    let detail = selected_finding.map_or_else(
        || vec![Line::raw("No findings. That is not proof of correctness.")],
        |finding| {
            let mut lines = vec![
                Line::styled(
                    finding.title.clone(),
                    Style::default().fg(Color::White).bold(),
                ),
                Line::raw(""),
                Line::raw(finding.detail.clone()),
                Line::raw(""),
                label("rule", &finding.rule_id),
                label("source", &finding.source),
                label("severity", &finding.severity.to_string()),
            ];
            if let Some(file_id) = &finding.file_id
                && let Some(file) = app.analysis.files.iter().find(|file| &file.id == file_id)
            {
                lines.push(label("file", &file.path));
            }
            lines
        },
    );
    let source = selected_finding.and_then(finding_source_target);
    render_detail_with_source(frame, &app.analysis, detail, source, columns[1], " Review ");
}

pub(super) fn render_files(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = app
        .analysis
        .files
        .iter()
        .map(|file| file_list_item(&app.analysis, file))
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" Source files ")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(25, 45, 52))),
        columns[0],
        &mut app.list_state,
    );

    let selected_file = app
        .list_state
        .selected()
        .and_then(|index| app.analysis.files.get(index));
    let detail = selected_file.map_or_else(
        || vec![Line::raw("No supported source files found.")],
        |file| file_detail(&app.analysis, file),
    );
    let source = selected_file.map(file_source_target);
    render_detail_with_source(
        frame,
        &app.analysis,
        detail,
        source,
        columns[1],
        " File evidence ",
    );
}

pub(super) fn file_list_item(analysis: &ProjectAnalysis, file: &SourceFile) -> ListItem<'static> {
    let available = source_file_available(analysis, file);
    let marker = if available { "*" } else { "-" };
    let marker_color = if available {
        Color::LightGreen
    } else {
        Color::DarkGray
    };
    ListItem::new(Line::from(vec![
        Span::styled(marker, Style::default().fg(marker_color)),
        Span::raw(" "),
        Span::styled(
            format!("{:10}", file.language),
            Style::default().fg(Color::LightCyan),
        ),
        Span::raw(file.path.clone()),
    ]))
}

pub(super) fn file_detail(analysis: &ProjectAnalysis, file: &SourceFile) -> Vec<Line<'static>> {
    let symbols = analysis
        .symbols
        .iter()
        .filter(|symbol| symbol.file_id == file.id && symbol.name != "<module>")
        .count();
    let dependencies = analysis
        .dependencies
        .iter()
        .filter(|dependency| dependency.file_id == file.id)
        .count();
    vec![
        Line::styled(file.path.clone(), Style::default().fg(Color::White).bold()),
        Line::raw(""),
        label("language", &file.language.to_string()),
        label("lines", &file.lines.to_string()),
        label("bytes", &file.bytes.to_string()),
        label("symbols", &symbols.to_string()),
        label("dependencies", &dependencies.to_string()),
        label(
            "source",
            if source_file_available(analysis, file) {
                "available"
            } else {
                "unavailable"
            },
        ),
    ]
}

pub(super) fn render_footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
        Span::raw(" quit  "),
        Span::styled(
            " j/k ",
            Style::default().fg(Color::Black).bg(Color::DarkGray),
        ),
        Span::raw(" move  "),
        Span::styled(
            " Enter ",
            Style::default().fg(Color::Black).bg(Color::DarkGray),
        ),
        Span::raw(" jump  "),
        Span::styled(" b ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
        Span::raw(" back  "),
        Span::styled(" e ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
        Span::raw(" export  "),
        Span::styled(" m ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
        Span::raw(" minify  "),
        Span::styled(
            format!("  {}", app.status),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), area);
}
