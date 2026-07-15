use std::{
    fs,
    io::{self, Stdout},
    path::PathBuf,
    sync::OnceLock,
    time::Duration,
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style as SyntectStyle, Theme, ThemeSet},
    parsing::SyntaxSet,
};

use crate::{
    model::{
        Confidence, Decision, DecisionStatus, Expectation, ExpectationStatus, ExpectationTarget,
        FlowEdge, Language, Location, ProjectAnalysis, Severity, SourceFile, Verification,
        VerificationStatus, Work, WorkStatus, Workflow,
    },
    susu::write_susu,
};

const TICK_RATE: Duration = Duration::from_millis(200);
const TABS: [&str; 11] = [
    "Overview",
    "Review",
    "Expectations",
    "Verifications",
    "Decisions",
    "Work",
    "Connections",
    "Workflows",
    "Flows",
    "Findings",
    "Files",
];

/// Opens the interactive engineering workbench for an analysis model.
///
/// # Errors
///
/// Returns an error if the terminal cannot enter raw mode, events cannot be
/// read, a frame cannot be drawn, or terminal state cannot be restored.
pub fn run(analysis: ProjectAnalysis, output: Option<PathBuf>) -> Result<()> {
    enable_raw_mode().context("could not enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("could not enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("could not initialize terminal")?;
    terminal.clear()?;

    let mut app = App::new(analysis, output);
    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode().context("could not restore terminal mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("could not leave alternate screen")?;
    terminal.show_cursor()?;
    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| render(frame, app))?;
        if event::poll(TICK_RATE)?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.next_tab(),
                KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => app.previous_tab(),
                KeyCode::Down | KeyCode::Char('j') => app.next_item(),
                KeyCode::Up | KeyCode::Char('k') => app.previous_item(),
                KeyCode::Char('1') => app.set_tab(0),
                KeyCode::Char('2') => app.set_tab(1),
                KeyCode::Char('3') => app.set_tab(2),
                KeyCode::Char('4') => app.set_tab(3),
                KeyCode::Char('5') => app.set_tab(4),
                KeyCode::Char('6') => app.set_tab(5),
                KeyCode::Char('7') => app.set_tab(6),
                KeyCode::Char('8') => app.set_tab(7),
                KeyCode::Char('9') => app.set_tab(8),
                KeyCode::Char('0') => app.set_tab(TABS.len() - 1),
                KeyCode::Enter => app.activate_selected(),
                KeyCode::Char('b') => app.back(),
                KeyCode::Char('e') => app.export(false),
                KeyCode::Char('m') => app.export(true),
                _ => {}
            }
        }
    }
}

struct App {
    analysis: ProjectAnalysis,
    tab: usize,
    list_state: ListState,
    output: Option<PathBuf>,
    status: String,
    history: Vec<NavState>,
}

#[derive(Debug, Clone, Copy)]
struct NavState {
    tab: usize,
    selected: Option<usize>,
}

impl App {
    fn new(analysis: ProjectAnalysis, output: Option<PathBuf>) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        let available_sources = available_source_count(&analysis);
        let source_status = format!(
            "{} files scanned - source preview {}/{}",
            analysis.files.len(),
            available_sources,
            analysis.files.len()
        );
        Self {
            status: source_status,
            analysis,
            tab: 0,
            list_state,
            output,
            history: Vec::new(),
        }
    }

    fn set_tab(&mut self, tab: usize) {
        self.tab = tab.min(TABS.len() - 1);
        self.list_state.select(Some(0));
    }

    fn next_tab(&mut self) {
        self.set_tab((self.tab + 1) % TABS.len());
    }

    fn previous_tab(&mut self) {
        self.set_tab((self.tab + TABS.len() - 1) % TABS.len());
    }

    fn item_count(&self) -> usize {
        match self.tab {
            1 => review_items(&self.analysis).len(),
            2 => self.analysis.expectations.len(),
            3 => self.analysis.verifications.len(),
            4 => self.analysis.decisions.len(),
            5 => self.analysis.works.len(),
            6 => connection_items(&self.analysis).len(),
            7 => self.analysis.workflows.len(),
            8 => self.analysis.flows.len(),
            9 => self.analysis.findings.len(),
            10 => self.analysis.files.len(),
            _ => 1,
        }
    }

    fn next_item(&mut self) {
        let count = self.item_count();
        if count == 0 {
            return;
        }
        let selected = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some((selected + 1) % count));
    }

    fn previous_item(&mut self) {
        let count = self.item_count();
        if count == 0 {
            return;
        }
        let selected = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some((selected + count - 1) % count));
    }

    fn export(&mut self, minified: bool) {
        let path = self.output.clone().unwrap_or_else(|| {
            let suffix = if minified { ".min.susu" } else { ".susu" };
            PathBuf::from(format!("{}{}", self.analysis.project_name, suffix))
        });
        match write_susu(&self.analysis, minified)
            .and_then(|source| fs::write(&path, source).context("could not write artifact"))
        {
            Ok(()) => self.status = format!("Exported {}", path.display()),
            Err(error) => self.status = format!("Export failed: {error:#}"),
        }
    }

    fn activate_selected(&mut self) {
        let Some(index) = self.list_state.selected() else {
            return;
        };
        let target = match self.tab {
            1 => review_items(&self.analysis)
                .get(index)
                .and_then(|item| item.jump.clone()),
            6 => connection_items(&self.analysis)
                .get(index)
                .map(|item| ReviewJump::Work(item.work.id.clone())),
            _ => None,
        };
        if let Some(target) = target {
            self.history.push(NavState {
                tab: self.tab,
                selected: self.list_state.selected(),
            });
            self.jump_to(target);
        }
    }

    fn back(&mut self) {
        if let Some(state) = self.history.pop() {
            self.tab = state.tab;
            self.list_state.select(state.selected);
        }
    }

    fn jump_to(&mut self, target: ReviewJump) {
        match target {
            ReviewJump::Finding(index) => {
                if index < self.analysis.findings.len() {
                    self.tab = 9;
                    self.list_state.select(Some(index));
                }
            }
            ReviewJump::Verification(id) => {
                if let Some(index) = self
                    .analysis
                    .verifications
                    .iter()
                    .position(|verification| verification.id == id)
                {
                    self.tab = 3;
                    self.list_state.select(Some(index));
                }
            }
            ReviewJump::Decision(id) => {
                if let Some(index) = self
                    .analysis
                    .decisions
                    .iter()
                    .position(|decision| decision.id == id)
                {
                    self.tab = 4;
                    self.list_state.select(Some(index));
                }
            }
            ReviewJump::Work(id) => {
                if let Some(index) = self.analysis.works.iter().position(|work| work.id == id) {
                    self.tab = 5;
                    self.list_state.select(Some(index));
                }
            }
            ReviewJump::Workflow(id) => {
                let order = workflow_order(&self.analysis);
                if let Some(index) = order.iter().position(|workflow_index| {
                    self.analysis
                        .workflows
                        .get(*workflow_index)
                        .is_some_and(|workflow| workflow.id == id)
                }) {
                    self.tab = 7;
                    self.list_state.select(Some(index));
                }
            }
        }
    }
}

fn render(frame: &mut Frame<'_>, app: &mut App) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, app, sections[0]);
    render_tabs(frame, app, sections[1]);
    match app.tab {
        0 => render_overview(frame, app, sections[2]),
        1 => render_review(frame, app, sections[2]),
        2 => render_expectations(frame, app, sections[2]),
        3 => render_verifications(frame, app, sections[2]),
        4 => render_decisions(frame, app, sections[2]),
        5 => render_works(frame, app, sections[2]),
        6 => render_connections(frame, app, sections[2]),
        7 => render_workflows(frame, app, sections[2]),
        8 => render_flows(frame, app, sections[2]),
        9 => render_findings(frame, app, sections[2]),
        10 => render_files(frame, app, sections[2]),
        _ => {}
    }
    render_footer(frame, app, sections[3]);
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
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

fn render_tabs(frame: &mut Frame<'_>, app: &App, area: Rect) {
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

fn render_overview(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(area);
    render_overview_metrics(frame, app, columns[0]);
    render_overview_languages(frame, app, columns[1]);
}

fn render_overview_metrics(frame: &mut Frame<'_>, app: &App, area: Rect) {
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

fn render_overview_languages(frame: &mut Frame<'_>, app: &App, area: Rect) {
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
struct ReviewItem {
    severity: ReviewSeverity,
    title: String,
    detail: String,
    source: String,
    jump: Option<ReviewJump>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ReviewSeverity {
    Attention,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
enum ReviewJump {
    Finding(usize),
    Verification(String),
    Decision(String),
    Work(String),
    Workflow(String),
}

fn render_review(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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

fn review_items(analysis: &ProjectAnalysis) -> Vec<ReviewItem> {
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

fn add_finding_review_items(analysis: &ProjectAnalysis, items: &mut Vec<ReviewItem>) {
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

fn add_verification_review_items(analysis: &ProjectAnalysis, items: &mut Vec<ReviewItem>) {
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

fn add_connection_review_items(analysis: &ProjectAnalysis, items: &mut Vec<ReviewItem>) {
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

fn add_workflow_gap_review_items(analysis: &ProjectAnalysis, items: &mut Vec<ReviewItem>) {
    for workflow in &analysis.workflows {
        let Some(entry_symbol) = workflow.entry_symbol.as_deref() else {
            continue;
        };
        let gaps = analysis
            .flows
            .iter()
            .filter(|flow| flow.from == entry_symbol && flow.to.is_none())
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

fn render_expectations(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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

fn render_verifications(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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

fn render_decisions(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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

fn render_works(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = app
        .analysis
        .works
        .iter()
        .map(|work| {
            let color = work_status_color(work.status);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:11}", work.status), Style::default().fg(color)),
                Span::styled(
                    format!(" {:14} ", work.kind),
                    Style::default().fg(Color::LightCyan),
                ),
                Span::raw(&work.title),
            ]))
        })
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
            || {
                vec![
                    Line::raw("No work records in this artifact."),
                    Line::raw(""),
                    Line::styled(
                        "Work records describe activity performed by humans, agents, imports, or automation. They do not prove verification by themselves.",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]
            },
            |work| {
                let expectation = work
                    .expectation_id
                    .as_deref()
                    .map_or_else(|| "-".to_owned(), |id| expectation_title(&app.analysis, id));
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
                        &expectation_subject(
                            &app.analysis,
                            work.target,
                            work.subject.as_deref(),
                        ),
                    ),
                    label("expectation", &expectation),
                    label("source", &work.source),
                    label("evidence", work.evidence.as_deref().unwrap_or("-")),
                ]
            },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ConnectionCategory {
    BlockedReview,
    NeedsVerification,
    GitBacked,
    Unlinked,
    Recorded,
}

struct ConnectionItem<'a> {
    category: ConnectionCategory,
    work: &'a Work,
}

fn render_connections(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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

fn connection_items(analysis: &ProjectAnalysis) -> Vec<ConnectionItem<'_>> {
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

fn connection_category(analysis: &ProjectAnalysis, work: &Work) -> ConnectionCategory {
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

const fn connection_category_label(category: ConnectionCategory) -> &'static str {
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

fn connection_detail_lines(
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

fn append_connection_verifications(
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

fn render_workflows(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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

fn workflow_detail_lines(analysis: &ProjectAnalysis, workflow: &Workflow) -> Vec<Line<'static>> {
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

fn append_workflow_traceability(
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

fn expectation_trace_lines(
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

fn append_workflow_decisions(
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

fn append_workflow_work(
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

fn render_flows(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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

fn render_findings(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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

fn render_files(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let columns = detail_columns(area);
    let items = app
        .analysis
        .files
        .iter()
        .map(|file| {
            let available = source_file_available(&app.analysis, file);
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
                Span::raw(&file.path),
            ]))
        })
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
        |file| {
            let symbols = app
                .analysis
                .symbols
                .iter()
                .filter(|symbol| symbol.file_id == file.id && symbol.name != "<module>")
                .count();
            let dependencies = app
                .analysis
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
                    if source_file_available(&app.analysis, file) {
                        "available"
                    } else {
                        "unavailable"
                    },
                ),
            ]
        },
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

fn render_footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
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

fn detail_columns(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area)
}

fn source_availability_text(analysis: &ProjectAnalysis) -> String {
    format!(
        "{}/{} available",
        available_source_count(analysis),
        analysis.files.len()
    )
}

fn available_source_count(analysis: &ProjectAnalysis) -> usize {
    analysis
        .files
        .iter()
        .filter(|file| source_file_available(analysis, file))
        .count()
}

fn source_file_available(analysis: &ProjectAnalysis, file: &SourceFile) -> bool {
    source_file_path(analysis, file).is_file()
}

fn source_file_path(analysis: &ProjectAnalysis, file: &SourceFile) -> PathBuf {
    PathBuf::from(&analysis.root).join(&file.path)
}

#[derive(Debug, Clone)]
struct SourceTarget {
    file_id: String,
    location: Option<Location>,
}

fn render_detail_with_source(
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

fn render_source_preview(
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

fn source_preview_lines(analysis: &ProjectAnalysis, source: &SourceTarget) -> Vec<Line<'static>> {
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

fn preview_window(location: Option<&Location>, line_count: usize) -> (usize, usize) {
    if let Some(location) = location {
        let start = location.start_line.saturating_sub(4).max(1);
        let end = (location.end_line + 6).min(line_count);
        return (start, end.max(start));
    }
    (1, line_count.min(24))
}

fn highlighted_code_line(
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

fn syntax_spans(language: Language, text: &str) -> Vec<Span<'static>> {
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

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn syntax_theme() -> &'static Theme {
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

fn syntax_for_language(
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
    };
    syntax_set
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text())
}

const fn ratatui_style(style: SyntectStyle) -> Style {
    let foreground = style.foreground;
    Style::new().fg(Color::Rgb(foreground.r, foreground.g, foreground.b))
}

fn workflow_source_target(workflow: &Workflow) -> SourceTarget {
    SourceTarget {
        file_id: workflow.file_id.clone(),
        location: Some(workflow.location.clone()),
    }
}

fn work_source_target(analysis: &ProjectAnalysis, work: &Work) -> Option<SourceTarget> {
    if work.target != ExpectationTarget::Workflow {
        return None;
    }
    work.subject
        .as_deref()
        .and_then(|id| analysis.workflows.iter().find(|workflow| workflow.id == id))
        .map(workflow_source_target)
}

fn commit_evidence_hash(evidence: &str) -> Option<&str> {
    evidence
        .strip_prefix("commit:")
        .filter(|hash| !hash.is_empty())
}

fn flow_source_target(analysis: &ProjectAnalysis, flow: &FlowEdge) -> Option<SourceTarget> {
    let file = flow_file(analysis, flow)?;
    Some(SourceTarget {
        file_id: file.id.clone(),
        location: Some(flow.location.clone()),
    })
}

fn finding_source_target(finding: &crate::model::Finding) -> Option<SourceTarget> {
    Some(SourceTarget {
        file_id: finding.file_id.clone()?,
        location: finding.location.clone(),
    })
}

fn file_source_target(file: &SourceFile) -> SourceTarget {
    SourceTarget {
        file_id: file.id.clone(),
        location: None,
    }
}

fn flow_file<'a>(analysis: &'a ProjectAnalysis, flow: &FlowEdge) -> Option<&'a SourceFile> {
    analysis
        .symbols
        .iter()
        .find(|symbol| symbol.id == flow.from)
        .and_then(|symbol| analysis.files.iter().find(|file| file.id == symbol.file_id))
}

fn symbol_name(analysis: &ProjectAnalysis, id: &str) -> String {
    analysis
        .symbols
        .iter()
        .find(|symbol| symbol.id == id)
        .map_or_else(|| id.to_owned(), |symbol| symbol.name.clone())
}

fn expectation_subject(
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

fn expectation_title(analysis: &ProjectAnalysis, id: &str) -> String {
    analysis
        .expectations
        .iter()
        .find(|expectation| expectation.id == id)
        .map_or_else(|| id.to_owned(), |expectation| expectation.title.clone())
}

fn workflow_expectations<'a>(
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

fn expectation_verifications<'a>(
    analysis: &'a ProjectAnalysis,
    expectation_id: &str,
) -> Vec<&'a Verification> {
    analysis
        .verifications
        .iter()
        .filter(|verification| verification.expectation_id == expectation_id)
        .collect()
}

fn workflow_decisions<'a>(analysis: &'a ProjectAnalysis, workflow_id: &str) -> Vec<&'a Decision> {
    analysis
        .decisions
        .iter()
        .filter(|decision| {
            decision.target == ExpectationTarget::Workflow
                && decision.subject.as_deref() == Some(workflow_id)
        })
        .collect()
}

fn workflow_works<'a>(analysis: &'a ProjectAnalysis, workflow_id: &str) -> Vec<&'a Work> {
    analysis
        .works
        .iter()
        .filter(|work| {
            work.target == ExpectationTarget::Workflow
                && work.subject.as_deref() == Some(workflow_id)
        })
        .collect()
}

fn workflow_order(analysis: &ProjectAnalysis) -> Vec<usize> {
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

fn workflow_priority<'a>(
    analysis: &'a ProjectAnalysis,
    workflow_id: &str,
) -> Option<&'a crate::model::WorkflowPriority> {
    analysis
        .workflow_priorities
        .iter()
        .find(|priority| priority.workflow_id == workflow_id)
}

fn label(name: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{name:>11}  "),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(value.to_owned()),
    ])
}

fn section_title(title: &'static str) -> Line<'static> {
    Line::styled(title, Style::default().fg(Color::LightCyan).bold())
}

const fn expectation_color(status: ExpectationStatus) -> Color {
    match status {
        ExpectationStatus::Accepted => Color::LightGreen,
        ExpectationStatus::Proposed => Color::Yellow,
        ExpectationStatus::Superseded => Color::DarkGray,
    }
}

const fn verification_color(status: VerificationStatus) -> Color {
    match status {
        VerificationStatus::Passed => Color::Green,
        VerificationStatus::Failed => Color::LightRed,
        VerificationStatus::Inconclusive => Color::Yellow,
    }
}

const fn decision_color(status: DecisionStatus) -> Color {
    match status {
        DecisionStatus::Accepted => Color::LightGreen,
        DecisionStatus::Proposed => Color::Yellow,
        DecisionStatus::Rejected => Color::LightRed,
        DecisionStatus::Superseded => Color::DarkGray,
    }
}

const fn work_status_color(status: WorkStatus) -> Color {
    match status {
        WorkStatus::Completed => Color::LightGreen,
        WorkStatus::InProgress => Color::LightCyan,
        WorkStatus::Blocked => Color::LightRed,
        WorkStatus::Proposed => Color::Yellow,
        WorkStatus::Superseded => Color::DarkGray,
    }
}

const fn confidence_explanation(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::Exact => "One same-file symbol matches this call.",
        Confidence::Likely => "One project-wide symbol matches this call.",
        Confidence::Ambiguous => "Multiple symbols match. Susumu did not choose one.",
        Confidence::External => "No project symbol matches; this is probably external or dynamic.",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::model::{SCHEMA_VERSION, WorkKind};

    #[test]
    fn overview_renders_with_an_empty_evidence_model() {
        let analysis = ProjectAnalysis {
            schema_version: SCHEMA_VERSION,
            project_name: "empty".to_owned(),
            root: ".".to_owned(),
            generated_unix_seconds: 0,
            files: Vec::new(),
            symbols: Vec::new(),
            dependencies: Vec::new(),
            workflows: Vec::new(),
            workflow_priorities: Vec::new(),
            flows: Vec::new(),
            expectations: Vec::new(),
            verifications: Vec::new(),
            decisions: Vec::new(),
            works: Vec::new(),
            findings: Vec::new(),
        };
        let mut app = App::new(analysis, None);
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
    }

    #[test]
    fn connections_tab_renders_git_work_records() {
        let analysis = ProjectAnalysis {
            schema_version: SCHEMA_VERSION,
            project_name: "connections".to_owned(),
            root: ".".to_owned(),
            generated_unix_seconds: 0,
            files: Vec::new(),
            symbols: Vec::new(),
            dependencies: Vec::new(),
            workflows: Vec::new(),
            workflow_priorities: Vec::new(),
            flows: Vec::new(),
            expectations: Vec::new(),
            verifications: Vec::new(),
            decisions: Vec::new(),
            works: vec![Work {
                id: "wk_git_abc123".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                expectation_id: None,
                kind: WorkKind::Implementation,
                status: WorkStatus::Completed,
                source: "import:git-connect".to_owned(),
                evidence: Some("commit:abc123".to_owned()),
                title: "Import connected commit".to_owned(),
                detail: "Generated by git connect.".to_owned(),
            }],
            findings: Vec::new(),
        };
        let mut app = App::new(analysis, None);
        app.set_tab(6);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
    }
}
