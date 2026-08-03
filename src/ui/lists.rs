use std::collections::{HashMap, HashSet};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::api::types::{FavoriteList, FavoriteQuestion};

use super::status_bar::render_status_bar;

const SPINNER: [&str; 10] = [
    "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}",
    "\u{2807}", "\u{280f}",
];

/// One line in the flattened outline: a section header, a list header, or
/// a problem nested under an expanded list. Recomputed from `lists` +
/// `expanded` on demand rather than stored, so it can never drift out of
/// sync with them.
#[derive(Clone, Copy)]
enum Row {
    Section(Section),
    List(usize),
    Problem(usize, usize),
}

/// Lists LeetCode's `favoritesLists` query returns are split by the
/// `isWatched` flag: lists the user created themselves ("My Lists") vs.
/// public/curated lists they've saved from other users without owning
/// them ("Saved by Me").
#[derive(Clone, Copy, PartialEq)]
enum Section {
    Mine,
    Saved,
}

impl Section {
    const ALL: [Section; 2] = [Section::Mine, Section::Saved];

    fn title(self) -> &'static str {
        match self {
            Section::Mine => "My Lists",
            Section::Saved => "Saved by Me",
        }
    }

    fn matches(self, list: &FavoriteList) -> bool {
        list.is_watched == (self == Section::Saved)
    }
}

pub struct ListsState {
    pub lists: Vec<FavoriteList>,
    pub loading: bool,
    pub error_message: Option<String>,
    pub spinner_frame: usize,
    // Outline navigation: `cursor` indexes into the rows built by
    // `build_rows` (list headers, plus each expanded list's problems
    // inline beneath it).
    pub cursor: usize,
    pub expanded: HashSet<usize>,
    pub loading_questions: HashSet<usize>,
    pub question_errors: HashMap<usize, String>,
    // Create mode
    pub create_mode: bool,
    pub create_input: String,
    // Confirm delete
    pub confirm_delete: bool,
    /// Hides solved problems in expanded lists, so a long list (e.g. a
    /// 150-problem "saved by me" list) only shows what's left to do.
    pub hide_solved: bool,
    /// Rows visible in the outline viewport, set by `render_outline` each
    /// frame -- used to size `J`/`K` half-page jumps.
    pub visible_height: usize,
    // Search: filters expanded lists' problems by title/number. Stays
    // applied after Enter closes typing, like Home's `/` search.
    pub search_mode: bool,
    pub search_query: String,
}

impl ListsState {
    pub fn new() -> Self {
        Self {
            lists: Vec::new(),
            loading: true,
            error_message: None,
            spinner_frame: 0,
            cursor: 0,
            expanded: HashSet::new(),
            loading_questions: HashSet::new(),
            question_errors: HashMap::new(),
            create_mode: false,
            create_input: String::new(),
            confirm_delete: false,
            hide_solved: false,
            visible_height: 0,
            search_mode: false,
            search_query: String::new(),
        }
    }

    /// Whether a problem passes the active hide-solved / search filters.
    fn question_visible(&self, q: &FavoriteQuestion) -> bool {
        if self.hide_solved && q.status.as_deref() == Some("ac") {
            return false;
        }
        if self.search_query.is_empty() {
            return true;
        }
        let query = self.search_query.to_lowercase();
        q.title.to_lowercase().contains(&query) || q.question_id == self.search_query
    }

    fn build_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for section in Section::ALL {
            let indices: Vec<usize> = self
                .lists
                .iter()
                .enumerate()
                .filter(|(_, l)| section.matches(l))
                .map(|(i, _)| i)
                .collect();
            if indices.is_empty() {
                continue;
            }
            rows.push(Row::Section(section));
            for i in indices {
                rows.push(Row::List(i));
                if self.expanded.contains(&i) {
                    for j in 0..self.lists[i].questions.len() {
                        if !self.question_visible(&self.lists[i].questions[j]) {
                            continue;
                        }
                        rows.push(Row::Problem(i, j));
                    }
                }
            }
        }
        rows
    }

    fn current_row(&self) -> Option<Row> {
        self.build_rows().get(self.cursor).copied()
    }

    /// The list a row belongs to, whether the row is the list header
    /// itself or one of its nested problems.
    fn current_list(&self) -> Option<&FavoriteList> {
        match self.current_row()? {
            Row::List(i) | Row::Problem(i, _) => self.lists.get(i),
            Row::Section(_) => None,
        }
    }

    /// Clamps the cursor into bounds and, since section headers aren't
    /// selectable rows, nudges it onto the nearest list/problem row.
    pub fn clamp_cursor(&mut self) {
        let rows = self.build_rows();
        let len = rows.len();
        if len == 0 {
            self.cursor = 0;
            return;
        }
        if self.cursor >= len {
            self.cursor = len - 1;
        }
        if matches!(rows[self.cursor], Row::Section(_)) {
            if let Some(idx) = rows[self.cursor..]
                .iter()
                .position(|r| !matches!(r, Row::Section(_)))
            {
                self.cursor += idx;
            } else if let Some(idx) = rows[..self.cursor]
                .iter()
                .rposition(|r| !matches!(r, Row::Section(_)))
            {
                self.cursor = idx;
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ListsAction {
        if self.confirm_delete {
            return self.handle_confirm_delete(key);
        }
        if self.create_mode {
            return self.handle_create_key(key);
        }
        if self.search_mode {
            return self.handle_search_key(key);
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => ListsAction::Back,
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_cursor(1);
                ListsAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_cursor(-1);
                ListsAction::None
            }
            KeyCode::Char('J') => {
                let half = (self.visible_height / 2).max(1) as i32;
                self.move_cursor(half);
                ListsAction::None
            }
            KeyCode::Char('K') => {
                let half = (self.visible_height / 2).max(1) as i32;
                self.move_cursor(-half);
                ListsAction::None
            }
            KeyCode::Char('g') => {
                self.jump_to_top();
                ListsAction::None
            }
            KeyCode::Char('G') => {
                self.jump_to_bottom();
                ListsAction::None
            }
            KeyCode::Char('h') => {
                self.hide_solved = !self.hide_solved;
                self.clamp_cursor();
                ListsAction::None
            }
            KeyCode::Char('/') => {
                self.search_mode = true;
                ListsAction::None
            }
            KeyCode::Char('l') => self.toggle_expand(),
            KeyCode::Enter | KeyCode::Char('o') => self.handle_enter(),
            KeyCode::Char('n') => {
                self.create_mode = true;
                self.create_input.clear();
                ListsAction::None
            }
            KeyCode::Char('d') => self.handle_delete_key(),
            _ => ListsAction::None,
        }
    }

    /// Steps the cursor `delta.abs()` selectable (non-section-header) rows
    /// in the given direction -- `delta` of +-1 for single-row j/k moves,
    /// larger magnitudes for J/K half-page jumps. Stops early at the first
    /// or last selectable row rather than overshooting out of bounds.
    fn move_cursor(&mut self, delta: i32) {
        let rows = self.build_rows();
        let len = rows.len();
        if len == 0 {
            return;
        }
        let step: i32 = if delta >= 0 { 1 } else { -1 };
        let count = delta.unsigned_abs();
        let mut next = self.cursor as i32;
        let mut moved = 0;
        while moved < count {
            let candidate = next + step;
            if candidate < 0 || candidate >= len as i32 {
                break;
            }
            next = candidate;
            if !matches!(rows[next as usize], Row::Section(_)) {
                self.cursor = next as usize;
                moved += 1;
            }
        }
    }

    /// Jumps to the first selectable row (`g`).
    fn jump_to_top(&mut self) {
        let rows = self.build_rows();
        if let Some(idx) = rows.iter().position(|r| !matches!(r, Row::Section(_))) {
            self.cursor = idx;
        }
    }

    /// Jumps to the last selectable row (`G`).
    fn jump_to_bottom(&mut self) {
        let rows = self.build_rows();
        if let Some(idx) = rows.iter().rposition(|r| !matches!(r, Row::Section(_))) {
            self.cursor = idx;
        }
    }

    fn row_index_of_list(&self, list_idx: usize) -> Option<usize> {
        self.build_rows()
            .iter()
            .position(|r| matches!(r, Row::List(i) if *i == list_idx))
    }

    /// Expands or collapses the list the cursor is currently on (or the
    /// parent list, if the cursor is on one of its nested problems).
    fn toggle_expand(&mut self) -> ListsAction {
        let list_idx = match self.current_row() {
            Some(Row::List(i)) | Some(Row::Problem(i, _)) => i,
            Some(Row::Section(_)) | None => return ListsAction::None,
        };

        if self.expanded.remove(&list_idx) {
            // Collapsing: if the cursor was on one of the now-hidden
            // problem rows, snap it back to the list header.
            if let Some(idx) = self.row_index_of_list(list_idx) {
                self.cursor = idx;
            }
            return ListsAction::None;
        }

        self.expanded.insert(list_idx);
        let needs_fetch = self
            .lists
            .get(list_idx)
            .is_some_and(|l| l.questions.is_empty());
        if needs_fetch && !self.loading_questions.contains(&list_idx) {
            self.loading_questions.insert(list_idx);
            self.question_errors.remove(&list_idx);
            if let Some(list) = self.lists.get(list_idx) {
                return ListsAction::FetchListQuestions(list.id_hash.clone());
            }
        }
        ListsAction::None
    }

    fn handle_enter(&mut self) -> ListsAction {
        match self.current_row() {
            Some(Row::Problem(list_idx, q_idx)) => self
                .lists
                .get(list_idx)
                .and_then(|l| l.questions.get(q_idx))
                .map(|q| ListsAction::OpenDetail(q.title_slug.clone()))
                .unwrap_or(ListsAction::None),
            Some(Row::List(_)) => self.toggle_expand(),
            Some(Row::Section(_)) | None => ListsAction::None,
        }
    }

    fn handle_delete_key(&mut self) -> ListsAction {
        match self.current_row() {
            Some(Row::List(_)) => {
                self.confirm_delete = true;
                ListsAction::None
            }
            Some(Row::Problem(list_idx, q_idx)) => self
                .lists
                .get(list_idx)
                .and_then(|list| {
                    list.questions.get(q_idx).map(|q| ListsAction::RemoveProblem {
                        id_hash: list.id_hash.clone(),
                        question_id: q.question_id.clone(),
                    })
                })
                .unwrap_or(ListsAction::None),
            Some(Row::Section(_)) | None => ListsAction::None,
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
                self.create_mode = false;
                if !self.create_input.trim().is_empty() {
                    let name = self.create_input.trim().to_string();
                    self.create_input.clear();
                    ListsAction::CreateList(name)
                } else {
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

    fn handle_search_key(&mut self, key: KeyEvent) -> ListsAction {
        match key.code {
            KeyCode::Esc => {
                self.search_mode = false;
                self.search_query.clear();
                self.clamp_cursor();
                ListsAction::None
            }
            KeyCode::Enter => {
                self.search_mode = false;
                self.clamp_cursor();
                ListsAction::None
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.clamp_cursor();
                ListsAction::None
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.clamp_cursor();
                ListsAction::None
            }
            _ => ListsAction::None,
        }
    }

    fn handle_confirm_delete(&mut self, key: KeyEvent) -> ListsAction {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.confirm_delete = false;
                if let Some(list) = self.current_list() {
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
        let s = SPINNER[state.spinner_frame % SPINNER.len()];
        let loading = Paragraph::new(format!(" {s} Loading lists..."))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, layout[1]);
    } else if let Some(ref err) = state.error_message {
        let error = Paragraph::new(format!(" Error: {err}"))
            .style(Style::default().fg(Color::Red));
        frame.render_widget(error, layout[1]);
    } else {
        render_outline(frame, layout[1], state);
    }

    // Status bar
    let hints = if state.create_mode {
        vec![("Enter", "Create"), ("Esc", "Cancel")]
    } else if state.confirm_delete {
        vec![("y", "Confirm"), ("any", "Cancel")]
    } else if state.search_mode {
        vec![("Enter", "Apply"), ("Esc", "Cancel"), ("type", "Filter")]
    } else {
        vec![
            ("l", "Expand/collapse"),
            ("Enter/o", "Open problem"),
            ("g/G", "Top/bottom"),
            ("J/K", "Half page"),
            ("h", "Hide solved"),
            ("/", "Search"),
            ("n", "New List"),
            ("d", "Delete"),
            ("q/Esc", "Back"),
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
        if let Some(list) = state.current_list() {
            render_confirm_delete(frame, area, &list.name, list.questions.len());
        }
    }
}

fn render_title_bar(frame: &mut Frame, area: Rect, state: &ListsState) {
    let mine_count = state.lists.iter().filter(|l| Section::Mine.matches(l)).count();
    let saved_count = state.lists.len() - mine_count;
    let mut count_text = if saved_count > 0 {
        format!("{mine_count} lists, {saved_count} saved")
    } else {
        format!("{mine_count} lists")
    };
    if state.hide_solved {
        count_text.push_str("  [-solved]");
    }

    let mut spans = vec![
        Span::styled(
            " Lists ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(count_text, Style::default().fg(Color::DarkGray)),
    ];

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

fn render_outline(frame: &mut Frame, area: Rect, state: &mut ListsState) {
    state.visible_height = area.height as usize;
    let mut items: Vec<ListItem<'static>> = Vec::new();

    for section in Section::ALL {
        let indices: Vec<usize> = state
            .lists
            .iter()
            .enumerate()
            .filter(|(_, l)| section.matches(l))
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            continue;
        }

        items.push(ListItem::new(Line::from(Span::styled(
            format!(" {}", section.title()),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ))));

        for i in indices {
            let list = &state.lists[i];
            let expanded = state.expanded.contains(&i);
            let chevron = if expanded { "\u{25be}" } else { "\u{25b8}" };
            let vis = if list.is_public_favorite {
                Span::styled("Public", Style::default().fg(Color::Green))
            } else {
                Span::styled("Private", Style::default().fg(Color::DarkGray))
            };

            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {chevron} "), Style::default().fg(Color::Cyan)),
                Span::styled(
                    list.name.clone(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{} problems", list.questions.len()),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                vis,
            ])));

            if !expanded {
                continue;
            }

            if state.loading_questions.contains(&i) {
                let s = SPINNER[state.spinner_frame % SPINNER.len()];
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("       {s} Loading problems..."),
                    Style::default().fg(Color::Yellow),
                ))));
            } else if let Some(err) = state.question_errors.get(&i) {
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("       Error: {err}"),
                    Style::default().fg(Color::Red),
                ))));
            } else if list.questions.is_empty() {
                items.push(ListItem::new(Line::from(Span::styled(
                    "       (no problems)",
                    Style::default().fg(Color::DarkGray),
                ))));
            } else {
                let visible: Vec<&FavoriteQuestion> = list
                    .questions
                    .iter()
                    .filter(|q| state.question_visible(q))
                    .collect();
                if visible.is_empty() {
                    let msg = if !state.search_query.is_empty() {
                        "       (no matches)"
                    } else {
                        "       (all solved)"
                    };
                    items.push(ListItem::new(Line::from(Span::styled(
                        msg,
                        Style::default().fg(Color::DarkGray),
                    ))));
                }
                for q in visible {
                    let status = match q.status.as_deref() {
                        Some("ac") => Span::styled("\u{2714}", Style::default().fg(Color::Green)),
                        Some("notac") => {
                            Span::styled("\u{25cf}", Style::default().fg(Color::Yellow))
                        }
                        _ => Span::raw(" "),
                    };
                    items.push(ListItem::new(Line::from(vec![
                        Span::raw(format!("       {}. ", q.question_id)),
                        status,
                        Span::raw(" "),
                        Span::styled(q.title.clone(), Style::default().fg(Color::White)),
                    ])));
                }
            }
        }
    }

    state.clamp_cursor();
    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(state.cursor));
    }

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("\u{25b8} ");

    frame.render_stateful_widget(list, area, &mut list_state);
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
