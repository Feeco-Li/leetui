use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

use crate::api::types::FavoriteList;

use super::status_bar::render_status_bar;

pub struct ListsState {
    pub lists: Vec<FavoriteList>,
    pub loading: bool,
    pub error_message: Option<String>,
    pub spinner_frame: usize,
    // List browser
    pub list_table_state: TableState,
    // Problem view within a list
    pub viewing_list: Option<usize>,
    pub problem_table_state: TableState,
    pub problems_loading: bool,
    pub problems_error: Option<String>,
    // Create mode
    pub create_mode: bool,
    pub create_input: String,
    // Confirm delete
    pub confirm_delete: bool,
}

impl ListsState {
    pub fn new() -> Self {
        Self {
            lists: Vec::new(),
            loading: true,
            error_message: None,
            spinner_frame: 0,
            list_table_state: TableState::default(),
            viewing_list: None,
            problem_table_state: TableState::default(),
            problems_loading: false,
            problems_error: None,
            create_mode: false,
            create_input: String::new(),
            confirm_delete: false,
        }
    }

    pub fn selected_list(&self) -> Option<&FavoriteList> {
        let idx = self.list_table_state.selected()?;
        self.lists.get(idx)
    }

    pub fn selected_list_idx(&self) -> Option<usize> {
        self.list_table_state.selected()
    }

    fn viewing_list_ref(&self) -> Option<&FavoriteList> {
        let idx = self.viewing_list?;
        self.lists.get(idx)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ListsAction {
        // Confirm delete dialog
        if self.confirm_delete {
            return self.handle_confirm_delete(key);
        }

        // Create mode
        if self.create_mode {
            return self.handle_create_key(key);
        }

        // Problem view within a list
        if self.viewing_list.is_some() {
            return self.handle_problem_key(key);
        }

        // List browser
        self.handle_list_key(key)
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> ListsAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => ListsAction::Back,
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_list_selection(1);
                ListsAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_list_selection(-1);
                ListsAction::None
            }
            KeyCode::Enter => {
                if let Some(idx) = self.list_table_state.selected() {
                    self.viewing_list = Some(idx);
                    self.problem_table_state = TableState::default();
                    self.problems_error = None;
                    if let Some(list) = self.lists.get(idx) {
                        if !list.questions.is_empty() {
                            self.problem_table_state.select(Some(0));
                            self.problems_loading = false;
                        } else {
                            // The batch favorites query never returns nested
                            // `questions` for custom lists, so fetch this
                            // one directly.
                            self.problems_loading = true;
                            return ListsAction::FetchListQuestions(list.id_hash.clone());
                        }
                    }
                }
                ListsAction::None
            }
            KeyCode::Char('n') => {
                self.create_mode = true;
                self.create_input.clear();
                ListsAction::None
            }
            KeyCode::Char('d') => {
                if self.selected_list().is_some() {
                    self.confirm_delete = true;
                }
                ListsAction::None
            }
            _ => ListsAction::None,
        }
    }

    fn handle_problem_key(&mut self, key: KeyEvent) -> ListsAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('b') => {
                self.viewing_list = None;
                self.problems_loading = false;
                self.problems_error = None;
                ListsAction::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_problem_selection(1);
                ListsAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_problem_selection(-1);
                ListsAction::None
            }
            KeyCode::Enter => {
                if let Some(list) = self.viewing_list_ref() {
                    if let Some(idx) = self.problem_table_state.selected() {
                        if let Some(q) = list.questions.get(idx) {
                            return ListsAction::OpenDetail(q.title_slug.clone());
                        }
                    }
                }
                ListsAction::None
            }
            KeyCode::Char('d') => {
                if let Some(list) = self.viewing_list_ref() {
                    if let Some(idx) = self.problem_table_state.selected() {
                        if let Some(q) = list.questions.get(idx) {
                            return ListsAction::RemoveProblem {
                                id_hash: list.id_hash.clone(),
                                question_id: q.question_id.clone(),
                            };
                        }
                    }
                }
                ListsAction::None
            }
            _ => ListsAction::None,
        }
    }

    fn handle_create_key(&mut self, key: KeyEvent) -> ListsAction {
        match key.code {
            KeyCode::Esc => {
                self.create_mode = false;
                self.create_input.clear();
                ListsAction::None
            }
            KeyCode::Enter => {
                if !self.create_input.trim().is_empty() {
                    let name = self.create_input.trim().to_string();
                    self.create_mode = false;
                    self.create_input.clear();
                    ListsAction::CreateList(name)
                } else {
                    self.create_mode = false;
                    self.create_input.clear();
                    ListsAction::None
                }
            }
            KeyCode::Char(c) => {
                self.create_input.push(c);
                ListsAction::None
            }
            KeyCode::Backspace => {
                self.create_input.pop();
                ListsAction::None
            }
            _ => ListsAction::None,
        }
    }

    fn handle_confirm_delete(&mut self, key: KeyEvent) -> ListsAction {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.confirm_delete = false;
                if let Some(list) = self.selected_list() {
                    return ListsAction::DeleteList(list.id_hash.clone());
                }
                ListsAction::None
            }
            _ => {
                self.confirm_delete = false;
                ListsAction::None
            }
        }
    }

    fn move_list_selection(&mut self, delta: i32) {
        if self.lists.is_empty() {
            return;
        }
        let current = self.list_table_state.selected().unwrap_or(0) as i32;
        let max = self.lists.len() as i32 - 1;
        let next = (current + delta).clamp(0, max) as usize;
        self.list_table_state.select(Some(next));
    }

    fn move_problem_selection(&mut self, delta: i32) {
        let count = self
            .viewing_list_ref()
            .map(|l| l.questions.len())
            .unwrap_or(0);
        if count == 0 {
            return;
        }
        let current = self.problem_table_state.selected().unwrap_or(0) as i32;
        let max = count as i32 - 1;
        let next = (current + delta).clamp(0, max) as usize;
        self.problem_table_state.select(Some(next));
    }
}

pub enum ListsAction {
    None,
    Back,
    OpenDetail(String),
    CreateList(String),
    DeleteList(String),
    RemoveProblem { id_hash: String, question_id: String },
    FetchListQuestions(String),
}

pub fn render_lists(frame: &mut Frame, area: Rect, state: &mut ListsState) {
    let layout = Layout::vertical([
        Constraint::Length(1), // title bar
        Constraint::Min(3),   // content
        Constraint::Length(1), // status bar
    ])
    .split(area);

    // Title bar
    render_title_bar(frame, layout[0], state);

    // Content
    if state.loading && state.lists.is_empty() {
        let spinner = ["\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}", "\u{2807}", "\u{280f}"];
        let s = spinner[state.spinner_frame % spinner.len()];
        let loading = Paragraph::new(format!(" {s} Loading lists..."))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, layout[1]);
    } else if let Some(ref err) = state.error_message {
        let error = Paragraph::new(format!(" Error: {err}"))
            .style(Style::default().fg(Color::Red));
        frame.render_widget(error, layout[1]);
    } else if state.viewing_list.is_some() {
        render_problem_table(frame, layout[1], state);
    } else {
        render_list_table(frame, layout[1], state);
    }

    // Status bar
    let hints = if state.create_mode {
        vec![("Enter", "Create"), ("Esc", "Cancel")]
    } else if state.confirm_delete {
        vec![("y", "Confirm"), ("any", "Cancel")]
    } else if state.viewing_list.is_some() {
        vec![
            ("j/k", "Navigate"),
            ("Enter", "View"),
            ("d", "Remove"),
            ("b/Esc", "Back"),
            ("?", "Help"),
        ]
    } else {
        vec![
            ("j/k", "Navigate"),
            ("Enter", "Open"),
            ("n", "New List"),
            ("d", "Delete"),
            ("Esc", "Back"),
            ("?", "Help"),
        ]
    };
    render_status_bar(frame, layout[2], &hints);

    // Create overlay
    if state.create_mode {
        render_create_overlay(frame, area, &state.create_input);
    }

    // Confirm delete overlay
    if state.confirm_delete {
        if let Some(list) = state.selected_list() {
            render_confirm_delete(frame, area, &list.name, list.questions.len());
        }
    }
}

fn render_title_bar(frame: &mut Frame, area: Rect, state: &ListsState) {
    let mut spans = vec![
        Span::styled(
            " Lists ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];

    if let Some(list) = state.viewing_list.and_then(|i| state.lists.get(i)) {
        spans.push(Span::styled(
            format!("{} ", list.name),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{} problems", list.questions.len()),
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        spans.push(Span::styled(
            format!("{} lists", state.lists.len()),
            Style::default().fg(Color::DarkGray),
        ));
    }

    let title = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
    frame.render_widget(title, area);
}

fn render_list_table(frame: &mut Frame, area: Rect, state: &mut ListsState) {
    let header = Row::new([
        Cell::from("Name"),
        Cell::from("Problems"),
        Cell::from("Visibility"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = state
        .lists
        .iter()
        .map(|list| {
            let vis = if list.is_public_favorite {
                Span::styled("Public", Style::default().fg(Color::Green))
            } else {
                Span::styled("Private", Style::default().fg(Color::DarkGray))
            };
            Row::new([
                Cell::from(format!(" {}", list.name)),
                Cell::from(format!("{}", list.questions.len())),
                Cell::from(vis),
            ])
        })
        .collect();

    let widths = [
        Constraint::Min(20),
        Constraint::Length(10),
        Constraint::Length(10),
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

    frame.render_stateful_widget(table, area, &mut state.list_table_state);
}

fn render_problem_table(frame: &mut Frame, area: Rect, state: &mut ListsState) {
    if state.problems_loading {
        let spinner = ["\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}", "\u{2807}", "\u{280f}"];
        let s = spinner[state.spinner_frame % spinner.len()];
        let loading = Paragraph::new(format!(" {s} Loading problems..."))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, area);
        return;
    }

    if let Some(ref err) = state.problems_error {
        let error = Paragraph::new(format!(" Error: {err}"))
            .style(Style::default().fg(Color::Red));
        frame.render_widget(error, area);
        return;
    }

    let list = match state.viewing_list.and_then(|i| state.lists.get(i)) {
        Some(l) => l,
        None => return,
    };

    let header = Row::new([
        Cell::from(" "),
        Cell::from("Title"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = list
        .questions
        .iter()
        .map(|q| {
            let status_cell = match q.status.as_deref() {
                Some("ac") => Cell::from(Span::styled(
                    " \u{2714}",
                    Style::default().fg(Color::Green),
                )),
                Some("notac") => Cell::from(Span::styled(
                    " \u{25cf}",
                    Style::default().fg(Color::Yellow),
                )),
                _ => Cell::from("  "),
            };
            Row::new([
                status_cell,
                Cell::from(format!(" {}", q.title)),
            ])
        })
        .collect();

    let widths = [Constraint::Length(3), Constraint::Min(20)];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("\u{25b8} ");

    frame.render_stateful_widget(table, area, &mut state.problem_table_state);
}

fn render_create_overlay(frame: &mut Frame, area: Rect, input: &str) {
    let w = 40u16.min(area.width.saturating_sub(4));
    let h = 5u16;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let overlay = Rect::new(x, y, w, h);

    frame.render_widget(Clear, overlay);
    let text = format!("\n {input}\u{258e}");
    let p = Paragraph::new(text)
        .block(
            Block::default()
                .title(" New List ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false });
    frame.render_widget(p, overlay);
}

fn render_confirm_delete(frame: &mut Frame, area: Rect, name: &str, problem_count: usize) {
    let w = 44u16.min(area.width.saturating_sub(4));
    let h = 5u16;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let overlay = Rect::new(x, y, w, h);

    frame.render_widget(Clear, overlay);
    let count_hint = if problem_count > 0 {
        format!(" ({problem_count} problems)")
    } else {
        String::new()
    };
    let text = format!("\n Delete \"{name}\"{count_hint}?\n (y) Yes  (any) Cancel");
    let p = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Confirm Delete ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        )
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: true });
    frame.render_widget(p, overlay);
}
