use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::api::types::QuestionDetail;

use super::rich_text::html_to_lines;
use super::status_bar::render_status_bar;

pub struct DetailState {
    pub detail: QuestionDetail,
    pub content_lines: Vec<Line<'static>>,
    pub scroll_offset: u16,
    pub content_height: u16,
}

impl DetailState {
    pub fn new(detail: QuestionDetail) -> Self {
        let mut content_lines = if detail.is_paid_only && detail.content.is_none() {
            vec![Line::from(Span::styled(
                " Premium content — not available without authentication.",
                Style::default().fg(Color::Yellow),
            ))]
        } else if let Some(ref html) = detail.content {
            html_to_lines(html)
        } else {
            vec![Line::from(Span::styled(
                " No content available.",
                Style::default().fg(Color::DarkGray),
            ))]
        };

        if !detail.hints.is_empty() {
            content_lines.push(Line::from(""));
            content_lines.push(Line::from(Span::styled(
                "Hints:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            for (i, hint) in detail.hints.iter().enumerate() {
                content_lines.push(Line::from(""));
                content_lines.push(Line::from(Span::styled(
                    format!("Hint {}", i + 1),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )));
                content_lines.extend(html_to_lines(hint));
            }
        }

        Self {
            detail,
            content_lines,
            scroll_offset: 0,
            content_height: 0,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> DetailAction {
        match key.code {
            KeyCode::Char('b') | KeyCode::Esc => DetailAction::Back,
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll(1);
                DetailAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll(-1);
                DetailAction::None
            }
            KeyCode::Char('d') => {
                self.scroll(self.content_height as i32 / 2);
                DetailAction::None
            }
            KeyCode::Char('u') => {
                self.scroll(-(self.content_height as i32 / 2));
                DetailAction::None
            }
            KeyCode::Char('o') => DetailAction::Scaffold(self.detail.title_slug.clone()),
            KeyCode::Char('a') => DetailAction::AddToList(self.detail.question_id.clone()),
            KeyCode::Char('r') => DetailAction::RunCode,
            KeyCode::Char('s') => DetailAction::SubmitCode,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                DetailAction::Quit
            }
            KeyCode::Char('c') => DetailAction::Commit,
            _ => DetailAction::None,
        }
    }

    fn scroll(&mut self, delta: i32) {
        let new_offset = self.scroll_offset as i32 + delta;
        self.scroll_offset = new_offset.max(0) as u16;
    }
}

pub enum DetailAction {
    None,
    Back,
    Quit,
    Scaffold(String),
    AddToList(String),
    RunCode,
    SubmitCode,
    Commit,
}

pub fn render_detail(frame: &mut Frame, area: Rect, state: &mut DetailState) {
    let layout = Layout::vertical([
        Constraint::Length(3), // title bar
        Constraint::Min(3),   // content
        Constraint::Length(1), // status bar
    ])
    .split(area);

    // Title bar
    render_detail_title(frame, layout[0], state);

    // Content area
    state.content_height = layout[1].height;

    let total_lines = state.content_lines.len() as u16;
    let max_scroll = total_lines.saturating_sub(state.content_height);
    if state.scroll_offset > max_scroll {
        state.scroll_offset = max_scroll;
    }

    // Add left padding to each line
    let padded_lines: Vec<Line> = state
        .content_lines
        .iter()
        .map(|line| {
            let mut spans = vec![Span::raw("  ")];
            spans.extend(line.spans.iter().cloned());
            Line::from(spans)
        })
        .collect();

    let content = Paragraph::new(padded_lines)
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll_offset, 0));

    frame.render_widget(content, layout[1]);

    // Scroll indicator
    if total_lines > state.content_height {
        let pct = if max_scroll > 0 {
            (state.scroll_offset as f64 / max_scroll as f64 * 100.0) as u16
        } else {
            100
        };
        let indicator = format!(" {}% ", pct);
        let ind_area = Rect::new(
            layout[1].right().saturating_sub(indicator.len() as u16 + 1),
            layout[1].y,
            indicator.len() as u16,
            1,
        );
        frame.render_widget(
            Paragraph::new(indicator).style(Style::default().fg(Color::DarkGray)),
            ind_area,
        );
    }

    // Status bar
    render_status_bar(
        frame,
        layout[2],
        &[
            ("j/k", "Scroll"),
            ("d/u", "Half page"),
            ("o", "Open"),
            ("a", "Add to List"),
            ("r", "Run"),
            ("s", "Submit"),
            ("c", "Commit & Push"),
            ("b/Esc", "Back"),
            ("?", "Help"),
        ],
    );
}

fn render_detail_title(frame: &mut Frame, area: Rect, state: &DetailState) {
    let d = &state.detail;
    let diff_color = match d.difficulty.as_str() {
        "Easy" => Color::Green,
        "Medium" => Color::Yellow,
        "Hard" => Color::Red,
        _ => Color::White,
    };

    let mut title_spans = vec![
        Span::styled(
            format!(" {}. {} ", d.frontend_question_id, d.title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[{}]", d.difficulty),
            Style::default()
                .fg(diff_color)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    match d.status.as_deref() {
        Some("ac") => title_spans.push(Span::styled(
            " \u{2714} Solved",
            Style::default().fg(Color::Green),
        )),
        Some("notac") => title_spans.push(Span::styled(
            " \u{25cf} Attempted",
            Style::default().fg(Color::Yellow),
        )),
        _ => {}
    }

    let title_line = Line::from(title_spans);

    let tags: Vec<Span> = d
        .topic_tags
        .iter()
        .enumerate()
        .flat_map(|(i, t)| {
            let mut spans = vec![Span::styled(
                format!(" {} ", t.name),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::DarkGray),
            )];
            if i < d.topic_tags.len() - 1 {
                spans.push(Span::raw(" "));
            }
            spans
        })
        .collect();

    let mut tags_line_spans = vec![Span::styled(" ", Style::default())];
    tags_line_spans.extend(tags);

    let title_block = Paragraph::new(vec![title_line, Line::from(tags_line_spans)])
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

    frame.render_widget(title_block, area);
}
