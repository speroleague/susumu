#![allow(clippy::wildcard_imports)]

use super::*;

pub(super) fn detail_columns(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area)
}

pub(super) fn source_availability_text(analysis: &ProjectAnalysis) -> String {
    format!(
        "{}/{} available",
        available_source_count(analysis),
        analysis.files.len()
    )
}

pub(super) fn available_source_count(analysis: &ProjectAnalysis) -> usize {
    analysis
        .files
        .iter()
        .filter(|file| source_file_available(analysis, file))
        .count()
}

pub(super) fn source_file_available(analysis: &ProjectAnalysis, file: &SourceFile) -> bool {
    source_file_path(analysis, file).is_file()
}

pub(super) fn source_file_path(analysis: &ProjectAnalysis, file: &SourceFile) -> PathBuf {
    PathBuf::from(&analysis.root).join(&file.path)
}

#[derive(Debug, Clone)]
pub(super) struct SourceTarget {
    pub(super) file_id: String,
    pub(super) location: Option<Location>,
}

pub(super) fn render_detail_with_source(
    frame: &mut Frame<'_>,
    analysis: &ProjectAnalysis,
    detail: Vec<Line<'static>>,
    source: Option<SourceTarget>,
    area: Rect,
    title: &'static str,
) {
    let Some(source) = source else {
        frame.render_widget(
            Paragraph::new(detail)
                .wrap(Wrap { trim: true })
                .block(Block::default().title(title).borders(Borders::ALL)),
            area,
        );
        return;
    };

    let panes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);
    frame.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: true })
            .block(Block::default().title(title).borders(Borders::ALL)),
        panes[0],
    );
    render_source_preview(frame, analysis, &source, panes[1]);
}

pub(super) fn render_source_preview(
    frame: &mut Frame<'_>,
    analysis: &ProjectAnalysis,
    source: &SourceTarget,
    area: Rect,
) {
    let lines = source_preview_lines(analysis, source);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(" Source preview ")
                .borders(Borders::ALL),
        ),
        area,
    );
}

pub(super) fn source_preview_lines(
    analysis: &ProjectAnalysis,
    source: &SourceTarget,
) -> Vec<Line<'static>> {
    let Some(file) = analysis.files.iter().find(|file| file.id == source.file_id) else {
        return vec![Line::styled(
            format!(
                "File id {} is not present in this artifact.",
                source.file_id
            ),
            Style::default().fg(Color::DarkGray),
        )];
    };
    let path = source_file_path(analysis, file);
    let Ok(source_text) = fs::read_to_string(&path) else {
        return vec![
            Line::styled(file.path.clone(), Style::default().fg(Color::White).bold()),
            Line::raw(""),
            Line::styled(
                format!("Source file could not be read at {}", path.display()),
                Style::default().fg(Color::DarkGray),
            ),
            Line::styled(
                "Artifacts remain useful without source, but code preview requires the original tree.",
                Style::default().fg(Color::DarkGray),
            ),
        ];
    };

    let source_lines = source_text.lines().collect::<Vec<_>>();
    let line_count = source_lines.len().max(1);
    let (start, end) = preview_window(source.location.as_ref(), line_count);
    let mut output = vec![
        Line::styled(file.path.clone(), Style::default().fg(Color::White).bold()),
        Line::raw(""),
    ];
    for number in start..=end {
        let text = source_lines.get(number - 1).copied().unwrap_or_default();
        output.push(highlighted_code_line(
            file.language,
            number,
            text,
            source.location.as_ref(),
        ));
    }
    output
}

pub(super) fn preview_window(location: Option<&Location>, line_count: usize) -> (usize, usize) {
    if let Some(location) = location {
        let start = location.start_line.saturating_sub(4).max(1);
        let end = (location.end_line + 6).min(line_count);
        return (start, end.max(start));
    }
    (1, line_count.min(24))
}

pub(super) fn highlighted_code_line(
    language: Language,
    number: usize,
    text: &str,
    location: Option<&Location>,
) -> Line<'static> {
    let selected = location
        .is_some_and(|location| number >= location.start_line && number <= location.end_line);
    let mut spans = vec![Span::styled(
        if selected {
            format!(">{number:>4} ")
        } else {
            format!(" {number:>4} ")
        },
        Style::default().fg(if selected {
            Color::LightMagenta
        } else {
            Color::DarkGray
        }),
    )];
    spans.extend(syntax_spans(language, text));
    let line = Line::from(spans);
    if selected {
        line.style(Style::default().bg(Color::Rgb(38, 28, 54)))
    } else {
        line
    }
}

pub(super) fn syntax_spans(language: Language, text: &str) -> Vec<Span<'static>> {
    let syntax_set = syntax_set();
    let syntax = syntax_for_language(syntax_set, language);
    let mut highlighter = HighlightLines::new(syntax, syntax_theme());
    match highlighter.highlight_line(text, syntax_set) {
        Ok(ranges) => ranges
            .into_iter()
            .map(|(style, segment)| Span::styled(segment.to_owned(), ratatui_style(style)))
            .collect(),
        Err(_) => vec![Span::raw(text.to_owned())],
    }
}

pub(super) fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

pub(super) fn syntax_theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let themes = ThemeSet::load_defaults();
        themes
            .themes
            .get("base16-ocean.dark")
            .or_else(|| themes.themes.values().next())
            .cloned()
            .unwrap_or_default()
    })
}

pub(super) fn syntax_for_language(
    syntax_set: &SyntaxSet,
    language: Language,
) -> &syntect::parsing::SyntaxReference {
    let extension = match language {
        Language::Rust => "rs",
        Language::Php => "php",
        Language::Python => "py",
        Language::JavaScript => "js",
        Language::TypeScript => "ts",
        Language::Tsx => "tsx",
        Language::Vue => "vue",
    };
    syntax_set
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text())
}

const fn ratatui_style(style: SyntectStyle) -> Style {
    let foreground = style.foreground;
    Style::new().fg(Color::Rgb(foreground.r, foreground.g, foreground.b))
}

pub(super) fn workflow_source_target(workflow: &Workflow) -> SourceTarget {
    SourceTarget {
        file_id: workflow.file_id.clone(),
        location: Some(workflow.location.clone()),
    }
}

pub(super) fn work_source_target(analysis: &ProjectAnalysis, work: &Work) -> Option<SourceTarget> {
    if work.target != ExpectationTarget::Workflow {
        return None;
    }
    work.subject
        .as_deref()
        .and_then(|id| analysis.workflows.iter().find(|workflow| workflow.id == id))
        .map(workflow_source_target)
}

pub(super) fn commit_evidence_hash(evidence: &str) -> Option<&str> {
    evidence
        .strip_prefix("commit:")
        .filter(|hash| !hash.is_empty())
}

pub(super) fn flow_source_target(
    analysis: &ProjectAnalysis,
    flow: &FlowEdge,
) -> Option<SourceTarget> {
    let file = flow_file(analysis, flow)?;
    Some(SourceTarget {
        file_id: file.id.clone(),
        location: Some(flow.location.clone()),
    })
}

pub(super) fn finding_source_target(finding: &crate::model::Finding) -> Option<SourceTarget> {
    Some(SourceTarget {
        file_id: finding.file_id.clone()?,
        location: finding.location.clone(),
    })
}

pub(super) fn file_source_target(file: &SourceFile) -> SourceTarget {
    SourceTarget {
        file_id: file.id.clone(),
        location: None,
    }
}

pub(super) fn flow_file<'a>(
    analysis: &'a ProjectAnalysis,
    flow: &FlowEdge,
) -> Option<&'a SourceFile> {
    analysis
        .symbols
        .iter()
        .find(|symbol| symbol.id == flow.from)
        .and_then(|symbol| analysis.files.iter().find(|file| file.id == symbol.file_id))
}

pub(super) fn symbol_name(analysis: &ProjectAnalysis, id: &str) -> String {
    analysis
        .symbols
        .iter()
        .find(|symbol| symbol.id == id)
        .map_or_else(|| id.to_owned(), |symbol| symbol.name.clone())
}

pub(super) fn expectation_subject(
    analysis: &ProjectAnalysis,
    target: ExpectationTarget,
    subject: Option<&str>,
) -> String {
    match target {
        ExpectationTarget::Project => analysis.project_name.clone(),
        ExpectationTarget::File => subject
            .and_then(|id| analysis.files.iter().find(|file| file.id == id))
            .map_or_else(
                || subject.unwrap_or("-").to_owned(),
                |file| file.path.clone(),
            ),
        ExpectationTarget::Symbol => {
            subject.map_or_else(|| "-".to_owned(), |id| symbol_name(analysis, id))
        }
        ExpectationTarget::Workflow => subject
            .and_then(|id| analysis.workflows.iter().find(|workflow| workflow.id == id))
            .map_or_else(
                || subject.unwrap_or("-").to_owned(),
                |workflow| workflow.trigger.clone(),
            ),
    }
}

pub(super) fn expectation_title(analysis: &ProjectAnalysis, id: &str) -> String {
    analysis
        .expectations
        .iter()
        .find(|expectation| expectation.id == id)
        .map_or_else(|| id.to_owned(), |expectation| expectation.title.clone())
}

pub(super) fn workflow_expectations<'a>(
    analysis: &'a ProjectAnalysis,
    workflow_id: &str,
) -> Vec<&'a Expectation> {
    analysis
        .expectations
        .iter()
        .filter(|expectation| {
            expectation.target == ExpectationTarget::Workflow
                && expectation.subject.as_deref() == Some(workflow_id)
        })
        .collect()
}

pub(super) fn expectation_verifications<'a>(
    analysis: &'a ProjectAnalysis,
    expectation_id: &str,
) -> Vec<&'a Verification> {
    analysis
        .verifications
        .iter()
        .filter(|verification| verification.expectation_id == expectation_id)
        .collect()
}

pub(super) fn workflow_decisions<'a>(
    analysis: &'a ProjectAnalysis,
    workflow_id: &str,
) -> Vec<&'a Decision> {
    analysis
        .decisions
        .iter()
        .filter(|decision| {
            decision.target == ExpectationTarget::Workflow
                && decision.subject.as_deref() == Some(workflow_id)
        })
        .collect()
}

pub(super) fn workflow_works<'a>(
    analysis: &'a ProjectAnalysis,
    workflow_id: &str,
) -> Vec<&'a Work> {
    analysis
        .works
        .iter()
        .filter(|work| {
            work.target == ExpectationTarget::Workflow
                && work.subject.as_deref() == Some(workflow_id)
        })
        .collect()
}

pub(super) fn workflow_order(analysis: &ProjectAnalysis) -> Vec<usize> {
    let mut ordered = Vec::new();
    for priority in &analysis.workflow_priorities {
        if let Some(index) = analysis
            .workflows
            .iter()
            .position(|workflow| workflow.id == priority.workflow_id)
        {
            ordered.push(index);
        }
    }
    for index in 0..analysis.workflows.len() {
        if !ordered.contains(&index) {
            ordered.push(index);
        }
    }
    ordered
}

pub(super) fn workflow_priority<'a>(
    analysis: &'a ProjectAnalysis,
    workflow_id: &str,
) -> Option<&'a crate::model::WorkflowPriority> {
    analysis
        .workflow_priorities
        .iter()
        .find(|priority| priority.workflow_id == workflow_id)
}

pub(super) fn label(name: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{name:>11}  "),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(value.to_owned()),
    ])
}

pub(super) fn section_title(title: &'static str) -> Line<'static> {
    Line::styled(title, Style::default().fg(Color::LightCyan).bold())
}

pub(super) const fn expectation_color(status: ExpectationStatus) -> Color {
    match status {
        ExpectationStatus::Accepted => Color::LightGreen,
        ExpectationStatus::Proposed => Color::Yellow,
        ExpectationStatus::Superseded => Color::DarkGray,
    }
}

pub(super) const fn verification_color(status: VerificationStatus) -> Color {
    match status {
        VerificationStatus::Passed => Color::Green,
        VerificationStatus::Failed => Color::LightRed,
        VerificationStatus::Inconclusive => Color::Yellow,
    }
}

pub(super) const fn decision_color(status: DecisionStatus) -> Color {
    match status {
        DecisionStatus::Accepted => Color::LightGreen,
        DecisionStatus::Proposed => Color::Yellow,
        DecisionStatus::Rejected => Color::LightRed,
        DecisionStatus::Superseded => Color::DarkGray,
    }
}

pub(super) const fn work_status_color(status: WorkStatus) -> Color {
    match status {
        WorkStatus::Completed => Color::LightGreen,
        WorkStatus::InProgress => Color::LightCyan,
        WorkStatus::Blocked => Color::LightRed,
        WorkStatus::Proposed => Color::Yellow,
        WorkStatus::Superseded => Color::DarkGray,
    }
}

pub(super) const fn confidence_explanation(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::Exact => "One same-file symbol matches this call.",
        Confidence::Likely => "One project-wide symbol matches this call.",
        Confidence::Ambiguous => "Multiple symbols match. Susumu did not choose one.",
        Confidence::External => "No project symbol matches; this is probably external or dynamic.",
    }
}
