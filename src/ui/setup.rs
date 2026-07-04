use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::status_bar::render_status_bar;

const FIELD_COUNT: usize = 6;
const FIELD_LABELS: [&str; FIELD_COUNT] = [
    "Workspace Directory",
    "Language",
    "Editor",
    "LeetCode Session Cookie",
    "CSRF Token",
    "Solutions Repo URL",
];
const FIELD_DEFAULTS: [&str; FIELD_COUNT] = ["~/leetcode", "rust", "vim", "", "", ""];
const FIELD_HINTS: [&str; FIELD_COUNT] = [
    "Directory where problem projects will be created",
    "Default language for code snippets (rust, python3, cpp, java, ...)",
    "Editor command to open files (vim, nvim, code, ...)",
    "(Optional) LEETCODE_SESSION cookie value for authentication",
    "(Optional) csrftoken cookie value for authentication",
    "(Optional) git URL to clone your solutions from if the workspace dir doesn't exist yet",
];

pub struct SetupState {
    pub fields: [String; FIELD_COUNT],
    pub active_field: usize,
    pub is_editing: bool,
    pub authenticated: bool,
}

impl SetupState {
    pub fn new() -> Self {
        Self {
            fields: [
                FIELD_DEFAULTS[0].to_string(),
                FIELD_DEFAULTS[1].to_string(),
                FIELD_DEFAULTS[2].to_string(),
                FIELD_DEFAULTS[3].to_string(),
                FIELD_DEFAULTS[4].to_string(),
                FIELD_DEFAULTS[5].to_string(),
            ],
            active_field: 0,
            is_editing: false,
            authenticated: false,
        }
    }

    pub fn from_config(config: &crate::config::Config) -> Self {
        Self {
            fields: [
                config.workspace_dir.clone(),
                config.language.clone(),
                config.editor.clone(),
                config.leetcode_session.clone().unwrap_or_default(),
                config.csrf_token.clone().unwrap_or_default(),
                config.solutions_repo_url.clone().unwrap_or_default(),
            ],
            active_field: 3,
            is_editing: true,
            authenticated: config.is_authenticated(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SetupAction {
        // Ctrl+L for browser login
        if key.code == KeyCode::Char('l') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return SetupAction::BrowserLogin;
        }

        match key.code {
            KeyCode::Tab | KeyCode::Down => {
                self.active_field = (self.active_field + 1) % FIELD_COUNT;
                SetupAction::None
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.active_field = (self.active_field + FIELD_COUNT - 1) % FIELD_COUNT;
                SetupAction::None
            }
            KeyCode::Char(c) => {
                self.fields[self.active_field].push(c);
                SetupAction::None
            }
            KeyCode::Backspace => {
                self.fields[self.active_field].pop();
                SetupAction::None
            }
            KeyCode::Enter => SetupAction::Submit,
            KeyCode::Esc => {
                if self.is_editing {
                    SetupAction::Cancel
                } else {
                    SetupAction::Quit
                }
            }
            _ => SetupAction::None,
        }
    }
}

pub enum SetupAction {
    None,
    Submit,
    Cancel,
    Quit,
    BrowserLogin,
}

pub fn render_setup(frame: &mut Frame, state: &SetupState) {
    let area = frame.area();

    let form_width = 70u16.min(area.width.saturating_sub(4));
    let form_height = 27u16.min(area.height.saturating_sub(2));
    let form_area = centered_rect(form_width, form_height, area);

    let block = Block::default()
        .title(" LeetCode CLI \u{2014} Setup ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    frame.render_widget(Clear, form_area);
    frame.render_widget(block, form_area);

    let inner = form_area.inner(Margin::new(2, 1));

    let layout = Layout::vertical([
        Constraint::Length(1), // welcome text
        Constraint::Length(1), // spacer
        Constraint::Length(3), // field 0
        Constraint::Length(3), // field 1
        Constraint::Length(3), // field 2
        Constraint::Length(3), // field 3
        Constraint::Length(3), // field 4
        Constraint::Length(3), // field 5
        Constraint::Length(1), // auth status
        Constraint::Length(1), // spacer
        Constraint::Length(1), // status bar
    ])
    .split(inner);

    let welcome = Paragraph::new("Configure your LeetCode CLI settings:")
        .style(Style::default().fg(Color::White));
    frame.render_widget(welcome, layout[0]);

    for i in 0..FIELD_COUNT {
        render_field(frame, layout[i + 2], i, state);
    }

    // Auth status line
    let auth_line = if state.authenticated {
        Line::from(Span::styled(
            "\u{25cf} Authenticated",
            Style::default().fg(Color::Green),
        ))
    } else {
        Line::from(vec![
            Span::styled(
                "\u{25cb} Not authenticated",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "  (Ctrl+L: auto-login from browser)",
                Style::default().fg(Color::DarkGray),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(auth_line), layout[8]);

    let esc_label = if state.is_editing { "Back" } else { "Quit" };
    render_status_bar(
        frame,
        layout[10],
        &[
            ("Tab/\u{2193}", "Next"),
            ("Shift+Tab/\u{2191}", "Prev"),
            ("Ctrl+L", "Auto-login"),
            ("Enter", "Save"),
            ("Esc", esc_label),
        ],
    );
}

fn render_field(frame: &mut Frame, area: Rect, index: usize, state: &SetupState) {
    let is_active = state.active_field == index;
    let label_style = if is_active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let value = &state.fields[index];
    let cursor = if is_active { "\u{258e}" } else { "" };

    let layout = Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let label = Line::from(vec![
        Span::styled(FIELD_LABELS[index], label_style),
        Span::styled(format!("  {}", FIELD_HINTS[index]), Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(label), layout[0]);

    let input_style = if is_active {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Gray)
    };

    // Mask session/csrf values with dots for security
    let display_value = if (index == 3 || index == 4) && !value.is_empty() {
        let shown = value.len().min(4);
        format!("{}{}",
            &value[..shown],
            "\u{2022}".repeat(value.len().saturating_sub(shown))
        )
    } else {
        value.clone()
    };

    let input = Line::from(vec![
        Span::styled(format!(" {display_value}"), input_style),
        Span::styled(cursor, Style::default().fg(Color::Cyan)),
    ]);
    let input_block = Paragraph::new(input).style(
        Style::default().bg(if is_active {
            Color::DarkGray
        } else {
            Color::Black
        }),
    );
    frame.render_widget(input_block, layout[1]);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}
