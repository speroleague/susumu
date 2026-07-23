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

#[path = "tui_evidence.rs"]
mod tui_evidence;
#[path = "tui_records.rs"]
mod tui_records;
#[path = "tui_source.rs"]
mod tui_source;
#[path = "tui_summary.rs"]
mod tui_summary;

#[allow(clippy::wildcard_imports)]
use tui_evidence::*;
#[allow(clippy::wildcard_imports)]
use tui_records::*;
#[allow(clippy::wildcard_imports)]
use tui_source::*;
#[allow(clippy::wildcard_imports)]
use tui_summary::*;

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

    let mut app = App::from_analysis(analysis, output);
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
            && key.kind == KeyEventKind::Press
            && handle_key(app, key.code)
        {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Tab
        | KeyCode::Right
        | KeyCode::Char(
            'l' | 'h' | 'j' | 'k' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' | '0',
        )
        | KeyCode::BackTab
        | KeyCode::Left
        | KeyCode::Down
        | KeyCode::Up => handle_navigation_key(app, code),
        KeyCode::Enter | KeyCode::Char('b' | 'e' | 'm') => handle_action_key(app, code),
        _ => false,
    }
}

fn handle_navigation_key(app: &mut App, code: KeyCode) -> bool {
    match code {
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
        _ => {}
    }
    false
}

fn handle_action_key(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Enter => app.activate_selected(),
        KeyCode::Char('b') => app.back(),
        KeyCode::Char('e') => app.export(false),
        KeyCode::Char('m') => app.export(true),
        _ => {}
    }
    false
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
    fn from_analysis(analysis: ProjectAnalysis, output: Option<PathBuf>) -> Self {
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
    render_content(frame, app, sections[2]);
    render_footer(frame, app, sections[3]);
}

fn render_content(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    match app.tab {
        0..=2 => render_summary_content(frame, app, area),
        3..=6 => render_record_content(frame, app, area),
        7..=10 => render_evidence_content(frame, app, area),
        _ => {}
    }
}

fn render_summary_content(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    match app.tab {
        0 => render_overview(frame, app, area),
        1 => render_review(frame, app, area),
        2 => render_expectations(frame, app, area),
        _ => {}
    }
}

fn render_record_content(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    match app.tab {
        3 => render_verifications(frame, app, area),
        4 => render_decisions(frame, app, area),
        5 => render_works(frame, app, area),
        6 => render_connections(frame, app, area),
        _ => {}
    }
}

fn render_evidence_content(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    match app.tab {
        7 => render_workflows(frame, app, area),
        8 => render_flows(frame, app, area),
        9 => render_findings(frame, app, area),
        10 => render_files(frame, app, area),
        _ => {}
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
        let mut app = App::from_analysis(analysis, None);
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
        let mut app = App::from_analysis(analysis, None);
        app.set_tab(6);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
    }
}
