use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
    Frame,
};

use crate::api::types::{ProblemSummary, UserStats};

use super::status_bar::render_status_bar;

pub struct FilterState {
    pub easy: bool,
    pub medium: bool,
    pub hard: bool,
    pub hide_solved: bool,
    pub active_item: usize,
    pub open: bool,
}

impl FilterState {
    pub fn new() -> Self {
        Self {
            easy: true,
            medium: true,
            hard: true,
            hide_solved: false,
            active_item: 0,
            open: false,
        }
    }

    fn item_count(&self) -> usize {
        4 // Easy, Medium, Hard, Hide Solved
    }

    pub fn summary(&self) -> Option<String> {
        let all = self.easy && self.medium && self.hard && !self.hide_solved;
        if all {
            return None;
        }
        let mut parts = Vec::new();
        if self.easy { parts.push("E"); }
        if self.medium { parts.push("M"); }
        if self.hard { parts.push("H"); }
        let mut s = parts.join("+");
        if self.hide_solved {
            s.push_str(" -Solved");
        }
        Some(format!("[{s}]"))
    }
}

pub struct HomeState {
    pub table_state: TableState,
    pub problems: Vec<ProblemSummary>,
    pub filtered_indices: Vec<usize>,
    pub search_query: String,
    pub search_mode: bool,
    pub filter: FilterState,
    pub loading: bool,
    pub loading_buffer: Vec<ProblemSummary>,
    pub total_problems: i32,
    pub error_message: Option<String>,
    pub spinner_frame: usize,
    pub user_stats: Option<UserStats>,
    pub confirm_quit: bool,
    /// 0 = "Yes", 1 = "No"
    pub confirm_quit_selected: usize,
}

impl HomeState {
    pub fn new() -> Self {
        Self {
            table_state: TableState::default(),
            problems: Vec::new(),
            filtered_indices: Vec::new(),
            search_query: String::new(),
            search_mode: false,
            filter: FilterState::new(),
            loading: true,
            loading_buffer: Vec::new(),
            total_problems: 0,
            error_message: None,
            spinner_frame: 0,
            user_stats: None,
            confirm_quit: false,
            confirm_quit_selected: 0,
        }
    }

    pub fn rebuild_filter(&mut self) {
        let query = self.search_query.to_lowercase();
        self.filtered_indices = self
            .problems
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                // Difficulty filter
                let diff_ok = match p.difficulty.as_str() {
                    "Easy" => self.filter.easy,
                    "Medium" => self.filter.medium,
                    "Hard" => self.filter.hard,
                    _ => true,
                };
                if !diff_ok {
                    return false;
                }

                // Hide solved filter
                if self.filter.hide_solved && p.status.as_deref() == Some("ac") {
                    return false;
                }

                // Search filter
                if query.is_empty() {
                    return true;
                }
                p.title.to_lowercase().contains(&query)
                    || p.frontend_question_id == query
            })
            .map(|(i, _)| i)
            .collect();

        // Keep selection in bounds
        if self.filtered_indices.is_empty() {
            self.table_state.select(None);
        } else if let Some(selected) = self.table_state.selected() {
            if selected >= self.filtered_indices.len() {
                self.table_state.select(Some(self.filtered_indices.len() - 1));
            }
        } else {
            self.table_state.select(Some(0));
        }
    }

    pub fn selected_problem(&self) -> Option<&ProblemSummary> {
        let selected = self.table_state.selected()?;
        let idx = *self.filtered_indices.get(selected)?;
        self.problems.get(idx)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> HomeAction {
        // Ctrl+C is the universal force-quit escape hatch -- it bypasses
        // the confirmation dialog (and any other mode) entirely.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return HomeAction::Quit;
        }

        if self.confirm_quit {
            return self.handle_confirm_quit_key(key);
        }

        if self.filter.open {
            return self.handle_filter_key(key);
        }

        if self.search_mode {
            return self.handle_search_key(key);
        }

        match key.code {
            KeyCode::Char('q') => {
                self.confirm_quit = true;
                self.confirm_quit_selected = 0;
                HomeAction::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1);
                HomeAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1);
                HomeAction::None
            }
            KeyCode::Char('g') => {
                if !self.filtered_indices.is_empty() {
                    self.table_state.select(Some(0));
                }
                HomeAction::None
            }
            KeyCode::Char('G') => {
                if !self.filtered_indices.is_empty() {
                    self.table_state
                        .select(Some(self.filtered_indices.len() - 1));
                }
                HomeAction::None
            }
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_query.clear();
                HomeAction::None
            }
            KeyCode::Char('f') => {
                self.filter.open = true;
                HomeAction::None
            }
            KeyCode::Enter => {
                if let Some(problem) = self.selected_problem() {
                    HomeAction::OpenDetail(problem.title_slug.clone())
                } else {
                    HomeAction::None
                }
            }
            KeyCode::Char('o') => {
                if let Some(problem) = self.selected_problem() {
                    HomeAction::Scaffold(problem.title_slug.clone())
                } else {
                    HomeAction::None
                }
            }
            KeyCode::Char('a') => {
                if let Some(problem) = self.selected_problem() {
                    HomeAction::AddToList(problem.frontend_question_id.clone())
                } else {
                    HomeAction::None
                }
            }
            KeyCode::Char('L') => HomeAction::Lists,
            KeyCode::Char('S') => HomeAction::Settings,
            _ => HomeAction::None,
        }
    }

    fn handle_confirm_quit_key(&mut self, key: KeyEvent) -> HomeAction {
        match key.code {
            KeyCode::Esc => {
                self.confirm_quit = false;
                HomeAction::None
            }
            KeyCode::Enter => {
                self.confirm_quit = false;
                if self.confirm_quit_selected == 0 {
                    HomeAction::Quit
                } else {
                    HomeAction::None
                }
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') | KeyCode::Tab => {
                self.confirm_quit_selected = 1 - self.confirm_quit_selected;
                HomeAction::None
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.confirm_quit = false;
                HomeAction::Quit
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.confirm_quit = false;
                HomeAction::None
            }
            _ => HomeAction::None,
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> HomeAction {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.filter.active_item = (self.filter.active_item + 1) % self.filter.item_count();
                HomeAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.filter.active_item = (self.filter.active_item + self.filter.item_count() - 1)
                    % self.filter.item_count();
                HomeAction::None
            }
            KeyCode::Char(' ') => {
                match self.filter.active_item {
                    0 => self.filter.easy = !self.filter.easy,
                    1 => self.filter.medium = !self.filter.medium,
                    2 => self.filter.hard = !self.filter.hard,
                    3 => self.filter.hide_solved = !self.filter.hide_solved,
                    _ => {}
                }
                self.rebuild_filter();
                HomeAction::None
            }
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('f') => {
                self.filter.open = false;
                HomeAction::None
            }
            _ => HomeAction::None,
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> HomeAction {
        match key.code {
            KeyCode::Esc => {
                self.search_mode = false;
                self.search_query.clear();
                self.rebuild_filter();
                HomeAction::None
            }
            KeyCode::Enter => {
                self.search_mode = false;
                // If no local results and query is numeric, fetch from API
                if self.filtered_indices.is_empty()
                    && !self.search_query.is_empty()
                    && self.search_query.chars().all(|c| c.is_ascii_digit())
                {
                    let query = self.search_query.clone();
                    self.search_query.clear();
                    self.rebuild_filter();
                    return HomeAction::SearchFetch(query);
                }
                // Enter also selects current item
                if let Some(problem) = self.selected_problem() {
                    return HomeAction::OpenDetail(problem.title_slug.clone());
                }
                HomeAction::None
            }
            KeyCode::Down | KeyCode::Up => {
                let delta = if key.code == KeyCode::Down { 1 } else { -1 };
                self.move_selection(delta);
                HomeAction::None
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.rebuild_filter();
                HomeAction::None
            }
            KeyCode::Backspace => {
                if self.search_query.is_empty() {
                    self.search_mode = false;
                } else {
                    self.search_query.pop();
                    self.rebuild_filter();
                }
                HomeAction::None
            }
            _ => HomeAction::None,
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let current = self.table_state.selected().unwrap_or(0) as i32;
        let max = self.filtered_indices.len() as i32 - 1;
        let next = (current + delta).clamp(0, max) as usize;
        self.table_state.select(Some(next));
    }
}

pub enum HomeAction {
    None,
    Quit,
    OpenDetail(String),
    Scaffold(String),
    SearchFetch(String),
    AddToList(String),
    Settings,
    Lists,
}

pub fn render_home(frame: &mut Frame, area: Rect, state: &mut HomeState) {
    let has_stats = state.user_stats.is_some();
    let stats_height: u16 = if has_stats { 2 } else { 0 };

    let layout = Layout::vertical([
        Constraint::Length(1),            // title bar
        Constraint::Length(stats_height), // stats header
        Constraint::Min(3),              // table
        Constraint::Length(1),           // status bar
    ])
    .split(area);

    // Title bar
    render_title_bar(frame, layout[0], state);

    // Stats header
    if let Some(ref stats) = state.user_stats {
        render_stats_header(frame, layout[1], stats);
    }

    // Problem table
    if state.loading && state.problems.is_empty() {
        let spinner = ["\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}", "\u{2807}", "\u{280f}"];
        let s = spinner[state.spinner_frame % spinner.len()];
        let loading = Paragraph::new(format!(" {s} Loading problems..."))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, layout[2]);
    } else if let Some(ref err) = state.error_message {
        let error = Paragraph::new(format!(" Error: {err}"))
            .style(Style::default().fg(Color::Red));
        frame.render_widget(error, layout[2]);
    } else {
        render_table(frame, layout[2], state);
    }

    // Status bar
    let hints = if state.confirm_quit {
        vec![
            ("\u{2190}/\u{2192}", "Toggle"),
            ("Enter", "Confirm"),
            ("Esc", "Cancel"),
        ]
    } else if state.search_mode {
        vec![
            ("Enter", "Apply"),
            ("Esc", "Cancel"),
            ("type", "Filter"),
        ]
    } else {
        vec![
            ("j/k", "Navigate"),
            ("Enter", "View"),
            ("o", "Open"),
            ("a", "Add to List"),
            ("/", "Search"),
            ("f", "Filter"),
            ("L", "Lists"),
            ("S", "Settings"),
            ("q", "Quit"),
            ("?", "Help"),
        ]
    };
    render_status_bar(frame, layout[3], &hints);

    // Filter popup overlay
    if state.filter.open {
        render_filter_popup(frame, area, &state.filter);
    }

    // Quit confirmation overlay
    if state.confirm_quit {
        render_confirm_quit(frame, area, state.confirm_quit_selected);
    }
}

fn render_confirm_quit(frame: &mut Frame, area: Rect, selected: usize) {
    let w = 30u16.min(area.width.saturating_sub(4));
    let h = 6u16;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let overlay = Rect::new(x, y, w, h);

    frame.render_widget(Clear, overlay);

    let button_style = |is_selected: bool| {
        if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        }
    };

    let buttons = Line::from(vec![
        Span::styled(" Yes ", button_style(selected == 0)),
        Span::raw("   "),
        Span::styled(" No ", button_style(selected == 1)),
    ]);

    let text = vec![
        Line::from(""),
        Line::from("Quit leetui?"),
        Line::from(""),
        buttons,
    ];

    let p = Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Confirm Quit ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .style(Style::default().fg(Color::White));
    frame.render_widget(p, overlay);
}

fn render_stats_header(frame: &mut Frame, area: Rect, stats: &UserStats) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
    ]).split(area);

    let total_solved = stats.easy_solved + stats.medium_solved + stats.hard_solved;
    let total_all = stats.easy_total + stats.medium_total + stats.hard_total;

    // Row 0: username + total
    let line0 = Line::from(vec![
        Span::styled(
            format!("  {} ", stats.username),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{total_solved}/{total_all} solved"),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(line0), rows[0]);

    // Row 1: Easy x/y  Med x/y  Hard x/y
    let line1 = Line::from(vec![
        Span::styled("  Easy ", Style::default().fg(Color::Green)),
        Span::styled(
            format!("{}/{}", stats.easy_solved, stats.easy_total),
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled("Med ", Style::default().fg(Color::Yellow)),
        Span::styled(
            format!("{}/{}", stats.medium_solved, stats.medium_total),
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled("Hard ", Style::default().fg(Color::Red)),
        Span::styled(
            format!("{}/{}", stats.hard_solved, stats.hard_total),
            Style::default().fg(Color::White),
        ),
    ]);
    frame.render_widget(Paragraph::new(line1), rows[1]);
}

fn render_title_bar(frame: &mut Frame, area: Rect, state: &HomeState) {
    let mut spans = vec![
        Span::styled(
            " LeetCode ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];

    if state.loading && !state.problems.is_empty() {
        let spinner = ["\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}", "\u{2807}", "\u{280f}"];
        let s = spinner[state.spinner_frame % spinner.len()];
        spans.push(Span::styled(
            format!("{s} Loading... {}/{} ", state.loading_buffer.len(), state.total_problems),
            Style::default().fg(Color::Yellow),
        ));
    } else {
        if let Some(summary) = state.filter.summary() {
            spans.push(Span::styled(
                format!("{summary} "),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        spans.push(Span::styled(
            format!(
                "{} / {} problems",
                state.filtered_indices.len(),
                state.total_problems
            ),
            Style::default().fg(Color::DarkGray),
        ));
    }

    if state.search_mode || !state.search_query.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("/{}", state.search_query),
            Style::default().fg(Color::Cyan),
        ));
        if state.search_mode {
            spans.push(Span::styled("\u{258e}", Style::default().fg(Color::Cyan)));
        }
    }

    let title = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
    frame.render_widget(title, area);
}

fn render_table(frame: &mut Frame, area: Rect, state: &mut HomeState) {
    let header = Row::new([
        Cell::from(" "),
        Cell::from(" # "),
        Cell::from("Title"),
        Cell::from("Difficulty"),
        Cell::from("AC Rate"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(0);

    let rows: Vec<Row> = state
        .filtered_indices
        .iter()
        .map(|&idx| {
            let p = &state.problems[idx];
            let diff_color = match p.difficulty.as_str() {
                "Easy" => Color::Green,
                "Medium" => Color::Yellow,
                "Hard" => Color::Red,
                _ => Color::White,
            };
            let paid = if p.is_paid_only { " \u{1f512}" } else { "" };
            let status_cell = match p.status.as_deref() {
                Some("ac") => Cell::from(Span::styled(" \u{2714}", Style::default().fg(Color::Green))),
                Some("notac") => Cell::from(Span::styled(" \u{25cf}", Style::default().fg(Color::Yellow))),
                _ => Cell::from("  "),
            };
            Row::new([
                status_cell,
                Cell::from(format!(" {}", p.frontend_question_id)),
                Cell::from(format!("{}{}", p.title, paid)),
                Cell::from(Span::styled(
                    p.difficulty.clone(),
                    Style::default().fg(diff_color),
                )),
                Cell::from(format!("{:.1}%", p.ac_rate)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(3),
        Constraint::Length(6),
        Constraint::Min(20),
        Constraint::Length(10),
        Constraint::Length(8),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("\u{25b8} ");

    frame.render_stateful_widget(table, area, &mut state.table_state);
}

fn render_filter_popup(frame: &mut Frame, area: Rect, filter: &FilterState) {
    let popup_width = 30u16.min(area.width.saturating_sub(4));
    let popup_height = 9u16;
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Filter ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));
    frame.render_widget(block, popup_area);

    let inner = Rect::new(popup_area.x + 2, popup_area.y + 1, popup_area.width.saturating_sub(4), popup_area.height.saturating_sub(2));
    let items = [
        ("Easy", filter.easy, Color::Green),
        ("Medium", filter.medium, Color::Yellow),
        ("Hard", filter.hard, Color::Red),
        ("Hide Solved", filter.hide_solved, Color::Cyan),
    ];

    let mut constraints: Vec<Constraint> = items.iter().map(|_| Constraint::Length(1)).collect();
    constraints.push(Constraint::Length(1)); // blank
    constraints.push(Constraint::Length(1)); // hint
    let rows = Layout::vertical(constraints).split(inner);

    for (i, ((label, checked, color), row)) in items.iter().zip(rows.iter()).enumerate() {
        let marker = if *checked { "\u{25c9}" } else { "\u{25cb}" };
        let highlight = i == filter.active_item;
        let style = if highlight {
            Style::default().fg(*color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(*color)
        };
        let prefix = if highlight { "\u{25b8} " } else { "  " };
        let line = Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(format!("{marker} "), style),
            Span::styled(*label, style),
        ]);
        frame.render_widget(Paragraph::new(line), *row);
    }

    // Hint at bottom
    let hint = Paragraph::new(Line::from(Span::styled(
        "  Space: toggle  Esc: close",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(hint, rows[items.len() + 1]);
}
