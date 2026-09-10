#![allow(clippy::wildcard_imports)]

use super::*;

pub(super) fn render_expectations(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = app
        .analysis
        .expectations
        .iter()
        .map(|expectation| {
            let color = expectation_color(expectation.status);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:10}", expectation.status),
                    Style::default().fg(color),
                ),
                Span::styled(
                    format!(" {:9} ", expectation.target),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(&expectation.title),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" Authored expectations ")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(45, 27, 52))),
        columns[0],
        &mut app.list_state,
    );

    let detail = app
        .list_state
        .selected()
        .and_then(|index| app.analysis.expectations.get(index))
        .map_or_else(
            || {
                vec![
                    Line::raw("No authored expectations in this artifact."),
                    Line::raw(""),
                    Line::styled(
                        "Scans observe code. Requirements, policies, and business intent enter as explicit records.",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]
            },
            |expectation| {
                vec![
                    Line::styled(&expectation.title, Style::default().fg(Color::White).bold()),
                    Line::raw(""),
                    Line::raw(&expectation.detail),
                    Line::raw(""),
                    label("id", &expectation.id),
                    label("status", &expectation.status.to_string()),
                    label("target", &expectation.target.to_string()),
                    label(
                        "subject",
                        &expectation_subject(
                            &app.analysis,
                            expectation.target,
                            expectation.subject.as_deref(),
                        ),
                    ),
                    label("source", &expectation.source),
                ]
            },
        );
    frame.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(" Intent record ")
                .borders(Borders::ALL),
        ),
        columns[1],
    );
}

pub(super) fn render_verifications(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = app
        .analysis
        .verifications
        .iter()
        .map(|verification| {
            let color = verification_color(verification.status);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:12}", verification.status),
                    Style::default().fg(color),
                ),
                Span::raw(" "),
                Span::raw(expectation_title(
                    &app.analysis,
                    &verification.expectation_id,
                )),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" Verification records ")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(27, 45, 32))),
        columns[0],
        &mut app.list_state,
    );

    let detail = app
        .list_state
        .selected()
        .and_then(|index| app.analysis.verifications.get(index))
        .map_or_else(
            || {
                vec![
                    Line::raw("No verification records in this artifact."),
                    Line::raw(""),
                    Line::styled(
                        "Verification records say how an expectation was checked and what evidence backs that result.",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]
            },
            |verification| {
                vec![
                    Line::styled(
                        expectation_title(&app.analysis, &verification.expectation_id),
                        Style::default().fg(Color::White).bold(),
                    ),
                    Line::raw(""),
                    Line::raw(&verification.detail),
                    Line::raw(""),
                    label("id", &verification.id),
                    label("status", &verification.status.to_string()),
                    label("expectation", &verification.expectation_id),
                    label("method", &verification.method),
                    label("source", &verification.source),
                    label("evidence", verification.evidence.as_deref().unwrap_or("-")),
                    label("basis", verification.basis.as_deref().unwrap_or("-")),
                ]
            },
        );
    frame.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(" Verification evidence ")
                .borders(Borders::ALL),
        ),
        columns[1],
    );
}

pub(super) fn render_decisions(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = app
        .analysis
        .decisions
        .iter()
        .map(|decision| {
            let color = decision_color(decision.status);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:10}", decision.status),
                    Style::default().fg(color),
                ),
                Span::styled(
                    format!(" {:9} ", decision.target),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(&decision.title),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" Decision records ")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(27, 36, 52))),
        columns[0],
        &mut app.list_state,
    );

    let detail = app
        .list_state
        .selected()
        .and_then(|index| app.analysis.decisions.get(index))
        .map_or_else(
            || {
                vec![
                    Line::raw("No decision records in this artifact."),
                    Line::raw(""),
                    Line::styled(
                        "Decisions record authored judgment: approvals, exceptions, tradeoffs, and unresolved business choices.",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]
            },
            |decision| {
                vec![
                    Line::styled(&decision.title, Style::default().fg(Color::White).bold()),
                    Line::raw(""),
                    Line::raw(&decision.detail),
                    Line::raw(""),
                    label("id", &decision.id),
                    label("status", &decision.status.to_string()),
                    label("target", &decision.target.to_string()),
                    label("basis", decision.basis.as_deref().unwrap_or("-")),
                    label(
                        "subject",
                        &expectation_subject(
                            &app.analysis,
                            decision.target,
                            decision.subject.as_deref(),
                        ),
                    ),
                    label("source", &decision.source),
                ]
            },
        );
    frame.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(" Judgment record ")
                .borders(Borders::ALL),
        ),
        columns[1],
    );
}

pub(super) fn render_works(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = app
        .analysis
        .works
        .iter()
        .map(work_list_item)
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" Work records ")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(24, 43, 48))),
        columns[0],
        &mut app.list_state,
    );

    let detail = app
        .list_state
        .selected()
        .and_then(|index| app.analysis.works.get(index))
        .map_or_else(
            || empty_work_detail(),
            |work| work_detail(&app.analysis, work),
        );
    frame.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(" Activity record ")
                .borders(Borders::ALL),
        ),
        columns[1],
    );
}

pub(super) fn render_review_threads(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = app
        .analysis
        .review_threads
        .iter()
        .map(review_thread_list_item)
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" Review threads ")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(38, 35, 50))),
        columns[0],
        &mut app.list_state,
    );

    let detail = app
        .list_state
        .selected()
        .and_then(|index| app.analysis.review_threads.get(index))
        .map_or_else(empty_review_thread_detail, review_thread_detail);
    frame.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(" Discussion record ")
                .borders(Borders::ALL),
        ),
        columns[1],
    );
}

fn review_thread_list_item(review: &ReviewThread) -> ListItem<'static> {
    let color = review_status_color(review.status);
    ListItem::new(Line::from(vec![
        Span::styled(format!("{:10}", review.status), Style::default().fg(color)),
        Span::styled(
            format!(" owner={:<16} ", review.owner.as_deref().unwrap_or("-")),
            Style::default().fg(Color::LightCyan),
        ),
        Span::raw(review.title.clone()),
    ]))
}

fn empty_review_thread_detail<'a>() -> Vec<Line<'a>> {
    vec![
        Line::raw("No review threads in this artifact."),
        Line::raw(""),
        Line::styled(
            "Review threads preserve authored discussion and ownership. They do not prove verification or approval.",
            Style::default().fg(Color::DarkGray),
        ),
    ]
}

fn review_thread_detail(review: &ReviewThread) -> Vec<Line<'_>> {
    vec![
        Line::styled(&review.title, Style::default().fg(Color::White).bold()),
        Line::raw(""),
        Line::raw(&review.detail),
        Line::raw(""),
        label("id", &review.id),
        label("status", &review.status.to_string()),
        label("target", &review.target.to_string()),
        label("subject", review.subject.as_deref().unwrap_or("-")),
        label(
            "anchor",
            review
                .anchor
                .as_ref()
                .map(ToString::to_string)
                .as_deref()
                .unwrap_or("-"),
        ),
        label("kind", &review.kind.to_string()),
        label("parent", review.parent.as_deref().unwrap_or("-")),
        label("owner", review.owner.as_deref().unwrap_or("-")),
        label("source", &review.source),
    ]
}

pub(super) fn work_list_item(work: &Work) -> ListItem<'static> {
    let color = work_status_color(work.status);
    ListItem::new(Line::from(vec![
        Span::styled(format!("{:11}", work.status), Style::default().fg(color)),
        Span::styled(
            format!(" {:14} ", work.kind),
            Style::default().fg(Color::LightCyan),
        ),
        Span::raw(work.title.clone()),
    ]))
}

pub(super) fn empty_work_detail() -> Vec<Line<'static>> {
    vec![
        Line::raw("No work records in this artifact."),
        Line::raw(""),
        Line::styled(
            "Work records describe activity performed by humans, agents, imports, or automation. They do not prove verification by themselves.",
            Style::default().fg(Color::DarkGray),
        ),
    ]
}

pub(super) fn work_detail<'a>(analysis: &'a ProjectAnalysis, work: &'a Work) -> Vec<Line<'a>> {
    let expectation = work
        .expectation_id
        .as_deref()
        .map_or_else(|| "-".to_owned(), |id| expectation_title(analysis, id));
    vec![
        Line::styled(&work.title, Style::default().fg(Color::White).bold()),
        Line::raw(""),
        Line::raw(&work.detail),
        Line::raw(""),
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
    ]
}
