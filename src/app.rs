use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::path::PathBuf;
use std::process::Command;
use tokio::sync::mpsc;

use crate::api::client::LeetCodeClient;
use crate::api::types::{CheckResponse, FavoriteList, ProblemSummary, QuestionDetail, UserStats};
use crate::config::Config;
use crate::event::{Event, EventHandler};
use crate::scaffold;
use crate::ui::detail::{self, DetailAction, DetailState};
use crate::ui::home::{self, HomeAction, HomeState};
use crate::ui::lists::{self, ListsAction, ListsState};
use crate::ui::result::{self, ResultAction, ResultData, ResultKind, ResultState};
use crate::ui::setup::{self, SetupAction, SetupState};

pub enum Screen {
    Setup(SetupState),
    Home(HomeState),
    Detail(DetailState),
    Result(ResultState),
    Lists(ListsState),
}

pub enum ApiResult {
    ProblemBatch {
        problems: Vec<ProblemSummary>,
        total: i32,
        done: bool,
    },
    Detail(Result<QuestionDetail>),
    RunResult(Result<CheckResponse>),
    SubmitResult(Result<CheckResponse>),
    UserStats(Option<UserStats>),
    SearchResult(Result<(Vec<ProblemSummary>, i32)>),
    ProblemFetchError(String),
    Favorites(Result<Vec<FavoriteList>>),
    ListMutation(Result<()>, String), // (result, success_message)
    PopupFavorites(Result<Vec<FavoriteList>>),
    ListQuestions(String, Result<Vec<crate::api::types::FavoriteQuestion>>), // (id_hash, result)
}

pub struct AddToListPopup {
    pub lists: Vec<FavoriteList>,
    pub selected: usize,
    pub question_id: String,
    pub loading: bool,
}

pub struct App {
    pub screen: Screen,
    pub config: Option<Config>,
    pub should_quit: bool,
    pub error_overlay: Option<String>,
    pub success_message: Option<(String, u8)>, // (message, ticks remaining)
    pub help_overlay: bool,
    pub login_prompt: bool,
    pub login_waiting: bool,
    pub last_opened_dir: Option<PathBuf>,
    pub add_to_list_popup: Option<AddToListPopup>,
    saved_home: Option<HomeState>,
    saved_lists: Option<ListsState>,
    saved_run_result: Option<ResultState>,
    api_client: LeetCodeClient,
    api_tx: mpsc::UnboundedSender<ApiResult>,
    api_rx: mpsc::UnboundedReceiver<ApiResult>,
}

impl App {
    pub fn new(config: Option<Config>) -> Result<Self> {
        let (api_tx, api_rx) = mpsc::unbounded_channel();
        let api_client = LeetCodeClient::new(
            config.as_ref().and_then(|c| c.leetcode_session.as_deref()),
            config.as_ref().and_then(|c| c.csrf_token.as_deref()),
            config.as_ref().and_then(|c| c.cf_clearance.as_deref()),
            config.as_ref().and_then(|c| c.user_agent.as_deref()),
        )?;

        let login_prompt = config.as_ref().is_some_and(|c| !c.is_authenticated());

        let screen = if config.is_some() {
            Screen::Home(HomeState::new())
        } else {
            Screen::Setup(SetupState::new())
        };

        Ok(Self {
            screen,
            config,
            should_quit: false,
            error_overlay: None,
            success_message: None,
            help_overlay: false,
            login_prompt,
            login_waiting: false,
            last_opened_dir: None,
            add_to_list_popup: None,
            saved_home: None,
            saved_lists: None,
            saved_run_result: None,
            api_client,
            api_tx,
            api_rx,
        })
    }

    pub async fn run(
        &mut self,
        terminal: &mut ratatui::DefaultTerminal,
        events: &mut EventHandler,
    ) -> Result<()> {
        if matches!(self.screen, Screen::Home(_)) {
            self.start_fetch_problems();
            self.start_fetch_user_stats();
            self.pull_workspace_on_startup();
        }

        loop {
            terminal.draw(|f| self.render(f))?;

            if self.should_quit {
                break;
            }

            tokio::select! {
                event = events.next() => {
                    match event? {
                        Event::Key(key) => self.handle_key(key, terminal, events)?,
                        Event::Tick => self.handle_tick(),
                        Event::Resize(_, _) => {}
                    }
                }
                Some(api_result) = self.api_rx.recv() => {
                    self.handle_api_result(api_result);
                }
            }
        }

        Ok(())
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        match &mut self.screen {
            Screen::Setup(state) => setup::render_setup(frame, state),
            Screen::Home(state) => home::render_home(frame, area, state),
            Screen::Detail(state) => detail::render_detail(frame, area, state),
            Screen::Result(state) => result::render_result(frame, area, state),
            Screen::Lists(state) => lists::render_lists(frame, area, state),
        }

        // Login waiting overlay (browser redirect)
        if self.login_waiting {
            let overlay_width = 56u16.min(area.width.saturating_sub(4));
            let overlay_height = 7u16.min(area.height.saturating_sub(4));
            let x = area.x + (area.width.saturating_sub(overlay_width)) / 2;
            let y = area.y + (area.height.saturating_sub(overlay_height)) / 2;
            let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

            frame.render_widget(Clear, overlay_area);
            let prompt = Paragraph::new("\nOpened LeetCode login in your browser.\nAfter logging in, press Enter to retry.\n\n Esc: Cancel")
                .block(
                    Block::default()
                        .title(" Browser Login ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .style(Style::default().fg(Color::White))
                .wrap(Wrap { trim: true });
            frame.render_widget(prompt, overlay_area);
        }

        // Login prompt overlay
        if self.login_prompt {
            let overlay_width = 52u16.min(area.width.saturating_sub(4));
            let overlay_height = 7u16.min(area.height.saturating_sub(4));
            let x = area.x + (area.width.saturating_sub(overlay_width)) / 2;
            let y = area.y + (area.height.saturating_sub(overlay_height)) / 2;
            let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

            frame.render_widget(Clear, overlay_area);
            let prompt = Paragraph::new("\nLogin to LeetCode?\nExtracts cookies from your browser.\n\n (Y) Yes  (N) No  (S) Settings")
                .block(
                    Block::default()
                        .title(" Login ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .style(Style::default().fg(Color::White))
                .wrap(Wrap { trim: true });
            frame.render_widget(prompt, overlay_area);
        }

        // Add-to-list popup overlay
        if let Some(ref popup) = self.add_to_list_popup {
            let overlay_width = 44u16.min(area.width.saturating_sub(4));
            let overlay_height = (popup.lists.len() as u16 + 4)
                .min(16)
                .max(5)
                .min(area.height.saturating_sub(4));
            let x = area.x + (area.width.saturating_sub(overlay_width)) / 2;
            let y = area.y + (area.height.saturating_sub(overlay_height)) / 2;
            let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

            frame.render_widget(Clear, overlay_area);

            if popup.loading {
                let spinner = [
                    "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}",
                    "\u{2826}", "\u{2827}", "\u{2807}", "\u{280f}",
                ];
                let s = spinner[0];
                let p = Paragraph::new(format!("\n {s} Loading lists..."))
                    .block(
                        Block::default()
                            .title(" Add to List ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Cyan)),
                    )
                    .style(Style::default().fg(Color::Yellow));
                frame.render_widget(p, overlay_area);
            } else if popup.lists.is_empty() {
                let p = Paragraph::new(
                    "\n No lists found.\n Create one from Lists (L) first.\n\n Esc: Close",
                )
                .block(
                    Block::default()
                        .title(" Add to List ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .style(Style::default().fg(Color::White))
                .wrap(Wrap { trim: true });
                frame.render_widget(p, overlay_area);
            } else {
                let inner_area = Rect::new(
                    overlay_area.x + 1,
                    overlay_area.y + 1,
                    overlay_area.width.saturating_sub(2),
                    overlay_area.height.saturating_sub(2),
                );

                let block = Block::default()
                    .title(" Add to List ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan));
                frame.render_widget(block, overlay_area);

                let visible_height = inner_area.height as usize;
                let items: Vec<Line> = popup
                    .lists
                    .iter()
                    .enumerate()
                    .map(|(i, list)| {
                        let selected = i == popup.selected;
                        let prefix = if selected { "\u{25b8} " } else { "  " };
                        let style = if selected {
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        Line::from(Span::styled(
                            format!("{prefix}{} ({})", list.name, list.questions.len()),
                            style,
                        ))
                    })
                    .collect();

                // Scroll if needed
                let scroll_offset = if popup.selected >= visible_height {
                    popup.selected - visible_height + 1
                } else {
                    0
                };

                let p = Paragraph::new(items).scroll((scroll_offset as u16, 0));
                frame.render_widget(p, inner_area);
            }
        }

        // Success toast (bottom center)
        if let Some((ref msg, _)) = self.success_message {
            let text = format!(" \u{2714} {msg} ");
            let w = (text.len() as u16 + 2).min(area.width.saturating_sub(4));
            let x = area.x + (area.width.saturating_sub(w)) / 2;
            let y = area.bottom().saturating_sub(3);
            let toast_area = Rect::new(x, y, w, 1);
            frame.render_widget(Clear, toast_area);
            frame.render_widget(
                Paragraph::new(text).style(Style::default().fg(Color::Black).bg(Color::Green)),
                toast_area,
            );
        }

        // Error overlay
        if let Some(ref msg) = self.error_overlay {
            let overlay_width = 50u16.min(area.width.saturating_sub(4));
            let overlay_height = 8u16.min(area.height.saturating_sub(4));
            let x = area.x + (area.width.saturating_sub(overlay_width)) / 2;
            let y = area.y + (area.height.saturating_sub(overlay_height)) / 2;
            let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

            frame.render_widget(Clear, overlay_area);
            let error_block = Paragraph::new(format!("\n{msg}\n\nPress Esc to dismiss"))
                .block(
                    Block::default()
                        .title(" Error ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Red)),
                )
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: true });
            frame.render_widget(error_block, overlay_area);
        }

        // Help overlay
        if self.help_overlay {
            let help_text = match &self.screen {
                Screen::Home(state) => {
                    if state.search_mode {
                        vec![
                            ("Enter", "Apply search / open selected"),
                            ("Esc", "Cancel search"),
                            ("\u{2191}/\u{2193}", "Navigate results"),
                            ("Backspace", "Delete char (empty exits)"),
                        ]
                    } else if state.filter.open {
                        vec![
                            ("j/k", "Navigate filters"),
                            ("Space", "Toggle filter"),
                            ("Esc/Enter/f", "Close filter"),
                        ]
                    } else {
                        vec![
                            ("j/k/\u{2191}/\u{2193}", "Navigate problems"),
                            ("g/G", "Jump to top / bottom"),
                            ("Enter", "View problem detail"),
                            ("o", "Scaffold & open in editor"),
                            ("a", "Add to list"),
                            ("/", "Search"),
                            ("f", "Filter by difficulty"),
                            ("L", "Browse lists"),
                            ("S", "Settings"),
                            ("q", "Quit"),
                        ]
                    }
                }
                Screen::Detail(_) => vec![
                    ("j/k/\u{2191}/\u{2193}", "Scroll"),
                    ("d/u", "Half page down / up"),
                    ("o", "Scaffold & open in editor"),
                    ("a", "Add to list"),
                    ("r", "Run code"),
                    ("b/q/Esc", "Back to list"),
                ],
                Screen::Result(state) => match state.kind {
                    ResultKind::Run => vec![
                        ("j/k/\u{2191}/\u{2193}", "Scroll"),
                        ("s", "Submit code"),
                        ("b/q/Esc", "Back to problem"),
                    ],
                    ResultKind::Submit => vec![
                        ("j/k/\u{2191}/\u{2193}", "Scroll"),
                        ("c", "Commit & push solution"),
                        ("b/q/Esc", "Back to run"),
                    ],
                },
                Screen::Lists(_) => vec![
                    ("j/k/\u{2191}/\u{2193}", "Navigate"),
                    ("l", "Expand/collapse list"),
                    ("Enter/o", "Open problem / toggle list"),
                    ("n", "Create new list"),
                    ("d", "Delete list / remove problem"),
                    ("Esc/q", "Back to home"),
                ],
                Screen::Setup(_) => vec![
                    ("Tab/\u{2193}", "Next field"),
                    ("Shift+Tab/\u{2191}", "Previous field"),
                    ("Ctrl+L", "Auto-login from browser"),
                    ("Enter", "Save settings"),
                    ("Esc", "Cancel"),
                ],
            };

            let max_key_len = help_text.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
            let lines: Vec<Line> = help_text
                .iter()
                .map(|(key, desc)| {
                    Line::from(vec![
                        Span::styled(
                            format!("  {:>width$}", key, width = max_key_len),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!("  {desc}"), Style::default().fg(Color::White)),
                    ])
                })
                .collect();

            let overlay_height = (lines.len() as u16 + 4).min(area.height.saturating_sub(4));
            let overlay_width = 48u16.min(area.width.saturating_sub(4));
            let x = area.x + (area.width.saturating_sub(overlay_width)) / 2;
            let y = area.y + (area.height.saturating_sub(overlay_height)) / 2;
            let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

            frame.render_widget(Clear, overlay_area);
            let help_block = Paragraph::new(lines)
                .block(
                    Block::default()
                        .title(" Keybindings ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .style(Style::default().fg(Color::White));
            frame.render_widget(help_block, overlay_area);
        }
    }

    fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        terminal: &mut ratatui::DefaultTerminal,
        events: &EventHandler,
    ) -> Result<()> {
        // Global quit: Ctrl+C always exits
        if key.code == KeyCode::Char('c')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
        {
            self.should_quit = true;
            return Ok(());
        }

        // Toggle help overlay
        if key.code == KeyCode::Char('?')
            && !self.login_prompt
            && !self.login_waiting
            && self.error_overlay.is_none()
            && self.add_to_list_popup.is_none()
        {
            self.help_overlay = !self.help_overlay;
            return Ok(());
        }

        // Handle login waiting (browser redirect)
        if self.login_waiting {
            match key.code {
                KeyCode::Enter => {
                    self.retry_browser_login();
                }
                KeyCode::Esc => {
                    self.login_waiting = false;
                }
                _ => {}
            }
            return Ok(());
        }

        // Handle login prompt
        if self.login_prompt {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.login_prompt = false;
                    self.browser_login();
                    self.start_fetch_user_stats();
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.login_prompt = false;
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    self.login_prompt = false;
                    let setup_state = match &self.config {
                        Some(c) => SetupState::from_config(c),
                        None => SetupState::new(),
                    };
                    self.screen = Screen::Setup(setup_state);
                }
                _ => {}
            }
            return Ok(());
        }

        // Dismiss help overlay on any key
        if self.help_overlay {
            self.help_overlay = false;
            return Ok(());
        }

        // Dismiss success message on any key
        if self.success_message.is_some() {
            self.success_message = None;
        }

        // Dismiss error overlay on Esc or q
        if self.error_overlay.is_some() {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.error_overlay = None,
                _ => {}
            }
            return Ok(());
        }

        // Handle add-to-list popup
        if let Some(ref mut popup) = self.add_to_list_popup {
            match key.code {
                KeyCode::Esc => {
                    self.add_to_list_popup = None;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if !popup.lists.is_empty() {
                        popup.selected = (popup.selected + 1) % popup.lists.len();
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if !popup.lists.is_empty() {
                        popup.selected =
                            (popup.selected + popup.lists.len() - 1) % popup.lists.len();
                    }
                }
                KeyCode::Enter => {
                    if let Some(list) = popup.lists.get(popup.selected) {
                        let id_hash = list.id_hash.clone();
                        let list_name = list.name.clone();
                        let question_id = popup.question_id.clone();
                        self.add_to_list_popup = None;
                        self.start_add_to_list(&id_hash, &question_id, &list_name);
                    }
                }
                _ => {}
            }
            return Ok(());
        }

        // Handle setup keys separately to avoid borrow conflicts with do_browser_login
        let setup_action = if let Screen::Setup(ref mut state) = self.screen {
            Some(state.handle_key(key))
        } else {
            None
        };

        if let Some(action) = setup_action {
            match action {
                SetupAction::Submit => {
                    if let Screen::Setup(ref state) = self.screen {
                        let session = if state.fields[3].is_empty() {
                            None
                        } else {
                            Some(state.fields[3].clone())
                        };
                        let csrf = if state.fields[4].is_empty() {
                            None
                        } else {
                            Some(state.fields[4].clone())
                        };
                        let solutions_repo_url = if state.fields[5].is_empty() {
                            None
                        } else {
                            Some(state.fields[5].clone())
                        };
                        let config = Config {
                            workspace_dir: state.fields[0].clone(),
                            language: state.fields[1].clone(),
                            editor: state.fields[2].clone(),
                            leetcode_session: session,
                            csrf_token: csrf,
                            cf_clearance: self
                                .config
                                .as_ref()
                                .and_then(|c| c.cf_clearance.clone()),
                            user_agent: self.config.as_ref().and_then(|c| c.user_agent.clone()),
                            solutions_repo_url,
                        };
                        if let Err(e) = config.save() {
                            self.error_overlay = Some(format!("Failed to save config: {e}"));
                        } else {
                            if let Ok(client) = LeetCodeClient::new(
                                config.leetcode_session.as_deref(),
                                config.csrf_token.as_deref(),
                                config.cf_clearance.as_deref(),
                                config.user_agent.as_deref(),
                            ) {
                                self.api_client = client;
                            }
                            self.config = Some(config);
                            self.screen = Screen::Home(HomeState::new());
                            self.start_fetch_problems();
                            self.start_fetch_user_stats();
                        }
                    }
                }
                SetupAction::Cancel => {
                    self.restore_home();
                }
                SetupAction::BrowserLogin => {
                    self.browser_login();
                    if let Screen::Setup(ref mut s) = self.screen {
                        if let Some(ref config) = self.config {
                            s.fields[3] = config.leetcode_session.clone().unwrap_or_default();
                            s.fields[4] = config.csrf_token.clone().unwrap_or_default();
                            s.authenticated = config.is_authenticated();
                        }
                    }
                }
                SetupAction::Quit => self.should_quit = true,
                SetupAction::None => {}
            }
            return Ok(());
        }

        match &mut self.screen {
            Screen::Home(state) => match state.handle_key(key) {
                HomeAction::Quit => self.should_quit = true,
                HomeAction::OpenDetail(slug) => {
                    self.start_fetch_detail(&slug);
                }
                HomeAction::Scaffold(slug) => {
                    self.start_fetch_detail_for_scaffold(&slug, terminal)?;
                }
                HomeAction::SearchFetch(query) => {
                    self.start_search_fetch(&query);
                }
                HomeAction::Lists => {
                    // Save home state and switch to lists
                    let old = std::mem::replace(&mut self.screen, Screen::Lists(ListsState::new()));
                    if let Screen::Home(home) = old {
                        self.saved_home = Some(home);
                    }
                    self.start_fetch_favorites();
                }
                HomeAction::AddToList(question_id) => {
                    self.open_add_to_list_popup(question_id);
                }
                HomeAction::Settings => {
                    let setup_state = match &self.config {
                        Some(c) => SetupState::from_config(c),
                        None => SetupState::new(),
                    };
                    self.screen = Screen::Setup(setup_state);
                }
                HomeAction::None => {}
            },
            Screen::Detail(state) => {
                let action = state.handle_key(key);
                match action {
                    DetailAction::Back => {
                        if let Some(lists) = self.saved_lists.take() {
                            self.screen = Screen::Lists(lists);
                        } else {
                            self.restore_home();
                        }
                    }
                    DetailAction::Quit => self.should_quit = true,
                    DetailAction::Scaffold(_) => {
                        let detail = if let Screen::Detail(s) = &self.screen {
                            s.detail.clone()
                        } else {
                            unreachable!()
                        };
                        self.do_scaffold_and_edit(&detail, terminal, events)?;
                    }
                    DetailAction::RunCode => {
                        let detail = if let Screen::Detail(s) = &self.screen {
                            s.detail.clone()
                        } else {
                            unreachable!()
                        };
                        self.start_run_code(&detail);
                    }
                    DetailAction::AddToList(question_id) => {
                        self.open_add_to_list_popup(question_id);
                    }
                    DetailAction::None => {}
                }
            }
            Screen::Result(state) => match state.handle_key(key) {
                ResultAction::Back => {
                    let (kind, detail) = if let Screen::Result(s) = &self.screen {
                        (s.kind, s.detail.clone())
                    } else {
                        unreachable!()
                    };
                    if matches!(kind, ResultKind::Submit) {
                        if let Some(run_result) = self.saved_run_result.take() {
                            self.screen = Screen::Result(run_result);
                        } else {
                            self.screen = Screen::Detail(DetailState::new(detail));
                        }
                    } else {
                        self.screen = Screen::Detail(DetailState::new(detail));
                    }
                }
                ResultAction::Quit => self.should_quit = true,
                ResultAction::SubmitCode => {
                    let (problem_title, detail) = if let Screen::Result(s) = &self.screen {
                        (s.problem_title.clone(), s.detail.clone())
                    } else {
                        unreachable!()
                    };
                    // Stash the current Run result so "back" from the
                    // Submit screen we're about to show returns to it,
                    // rather than skipping straight to the problem detail.
                    let old = std::mem::replace(
                        &mut self.screen,
                        Screen::Result(ResultState::new(
                            ResultKind::Submit,
                            problem_title,
                            detail.clone(),
                        )),
                    );
                    if let Screen::Result(run_result) = old {
                        self.saved_run_result = Some(run_result);
                    }
                    self.start_submit_code(&detail);
                }
                ResultAction::Commit => {
                    let detail = if let Screen::Result(s) = &self.screen {
                        s.detail.clone()
                    } else {
                        unreachable!()
                    };
                    self.do_git_commit(&detail, terminal, events)?;
                }
                ResultAction::None => {}
            },
            Screen::Lists(state) => {
                let action = state.handle_key(key);
                match action {
                    ListsAction::Back => {
                        self.restore_home();
                    }
                    ListsAction::OpenDetail(slug) => {
                        self.start_fetch_detail(&slug);
                    }
                    ListsAction::CreateList(name) => {
                        self.start_create_list(&name);
                    }
                    ListsAction::DeleteList(id_hash) => {
                        self.start_delete_list(&id_hash);
                    }
                    ListsAction::RemoveProblem {
                        id_hash,
                        question_id,
                    } => {
                        self.start_remove_from_list(&id_hash, &question_id);
                    }
                    ListsAction::FetchListQuestions(id_hash) => {
                        self.start_fetch_list_questions(&id_hash);
                    }
                    ListsAction::None => {}
                }
            }
            Screen::Setup(_) => {} // handled above
        }

        Ok(())
    }

    fn handle_tick(&mut self) {
        // Auto-dismiss success messages
        if let Some((_, ref mut ticks)) = self.success_message {
            if *ticks == 0 {
                self.success_message = None;
            } else {
                *ticks -= 1;
            }
        }

        match &mut self.screen {
            Screen::Home(state) => {
                state.spinner_frame = state.spinner_frame.wrapping_add(1);
            }
            Screen::Result(state) => {
                state.spinner_frame = state.spinner_frame.wrapping_add(1);
            }
            Screen::Lists(state) => {
                state.spinner_frame = state.spinner_frame.wrapping_add(1);
            }
            _ => {}
        }
    }

    fn handle_api_result(&mut self, result: ApiResult) {
        match result {
            ApiResult::ProblemBatch {
                problems,
                total,
                done,
            } => {
                // Resolve target: active Home screen or saved_home
                let state = if let Screen::Home(ref mut s) = self.screen {
                    Some(s)
                } else {
                    self.saved_home.as_mut()
                };
                if let Some(state) = state {
                    state.loading_buffer.extend(problems);
                    state.total_problems = total;
                    if done {
                        state.loading = false;
                        state.problems = std::mem::take(&mut state.loading_buffer);
                        state.rebuild_filter();
                        let problems = state.problems.clone();
                        tokio::spawn(async move {
                            save_problems_cache(&problems);
                        });
                    } else if state.problems.is_empty() {
                        // No cache — show what we have so far
                        state.problems = state.loading_buffer.clone();
                        state.rebuild_filter();
                    }
                    state.error_message = None;
                }
            }
            ApiResult::ProblemFetchError(e) => {
                let state = if let Screen::Home(ref mut s) = self.screen {
                    Some(s)
                } else {
                    self.saved_home.as_mut()
                };
                if let Some(state) = state {
                    state.loading = false;
                    state.error_message = Some(e);
                }
            }
            ApiResult::Detail(Ok(detail)) => {
                // Save current screen state before switching to detail
                let old =
                    std::mem::replace(&mut self.screen, Screen::Detail(DetailState::new(detail)));
                match old {
                    Screen::Home(home) => self.saved_home = Some(home),
                    Screen::Lists(lists) => self.saved_lists = Some(lists),
                    _ => {}
                }
            }
            ApiResult::Detail(Err(e)) => {
                self.error_overlay = Some(format!("Failed to load problem: {e}"));
            }
            ApiResult::RunResult(res) | ApiResult::SubmitResult(res) => {
                if let Screen::Result(ref mut state) = self.screen {
                    match res {
                        Ok(resp) => state.set_result(ResultData::from_check(&resp)),
                        Err(e) => state.set_error(format!("{e}")),
                    }
                }
            }
            ApiResult::UserStats(stats) => {
                let state = if let Screen::Home(ref mut s) = self.screen {
                    Some(s)
                } else {
                    self.saved_home.as_mut()
                };
                if let Some(state) = state {
                    state.user_stats = stats;
                }
            }
            ApiResult::SearchResult(Ok((problems, _))) => {
                if let Some(p) = problems.first() {
                    self.start_fetch_detail(&p.title_slug.clone());
                } else {
                    self.error_overlay = Some("Problem not found.".to_string());
                }
            }
            ApiResult::SearchResult(Err(e)) => {
                self.error_overlay = Some(format!("Search failed: {e}"));
            }
            ApiResult::Favorites(Ok(lists)) => {
                let mut needs_question_fetch = Vec::new();
                if let Screen::Lists(ref mut state) = self.screen {
                    state.lists = lists;
                    state.loading = false;
                    state.error_message = None;
                    state.clamp_cursor();
                    // The batch query never includes each list's problems
                    // (see FAVORITES_LIST_QUERY) -- every list's real
                    // "Problems" count has to be backfilled with a
                    // per-list fetch.
                    needs_question_fetch = state
                        .lists
                        .iter()
                        .enumerate()
                        .filter(|(i, l)| l.questions.is_empty() && !state.loading_questions.contains(i))
                        .map(|(i, l)| (i, l.id_hash.clone()))
                        .collect();
                    for (i, _) in &needs_question_fetch {
                        state.loading_questions.insert(*i);
                    }
                }
                for (_, id_hash) in needs_question_fetch {
                    self.start_fetch_list_questions(&id_hash);
                }
            }
            ApiResult::Favorites(Err(e)) => {
                if let Screen::Lists(ref mut state) = self.screen {
                    state.loading = false;
                    state.error_message = Some(format!("{e}"));
                }
            }
            ApiResult::ListMutation(Ok(()), msg) => {
                self.success_message = Some((msg, 12)); // ~2 seconds at 5 ticks/sec
                if matches!(self.screen, Screen::Lists(_)) {
                    self.start_fetch_favorites();
                }
            }
            ApiResult::ListMutation(Err(e), _) => {
                self.error_overlay = Some(format!("{e}"));
            }
            ApiResult::PopupFavorites(Ok(lists)) => {
                if let Some(ref mut popup) = self.add_to_list_popup {
                    popup.lists = lists;
                    popup.loading = false;
                }
            }
            ApiResult::PopupFavorites(Err(e)) => {
                self.add_to_list_popup = None;
                self.error_overlay = Some(format!("Failed to load lists: {e}"));
            }
            ApiResult::ListQuestions(id_hash, Ok(questions)) => {
                if let Screen::Lists(ref mut state) = self.screen {
                    if let Some(idx) = state.lists.iter().position(|l| l.id_hash == id_hash) {
                        state.lists[idx].questions = questions;
                        state.loading_questions.remove(&idx);
                        state.question_errors.remove(&idx);
                    }
                }
            }
            ApiResult::ListQuestions(id_hash, Err(e)) => {
                if let Screen::Lists(ref mut state) = self.screen {
                    if let Some(idx) = state.lists.iter().position(|l| l.id_hash == id_hash) {
                        if state.loading_questions.remove(&idx) {
                            state.question_errors.insert(idx, format!("{e}"));
                        }
                    }
                }
            }
        }
    }

    fn restore_home(&mut self) {
        if let Some(home) = self.saved_home.take() {
            self.screen = Screen::Home(home);
        } else {
            self.screen = Screen::Home(HomeState::new());
            self.start_fetch_problems();
        }
    }

    fn start_fetch_problems(&mut self) {
        if let Screen::Home(ref mut state) = self.screen {
            state.loading = true;
            state.error_message = None;

            // Load cached problems for instant display
            if let Some(cached) = load_cached_problems() {
                state.total_problems = cached.len() as i32;
                state.problems = cached;
                state.rebuild_filter();
            } else {
                state.problems.clear();
                state.filtered_indices.clear();
                state.total_problems = 0;
            }

            let client = self.api_client.clone();
            let tx = self.api_tx.clone();
            const BATCH: i32 = 100;

            tokio::spawn(async move {
                let mut skip: i32 = 0;
                loop {
                    let result = client.fetch_problems(BATCH, skip, None, None).await;
                    match result {
                        Ok((batch, total)) => {
                            let done = (batch.len() as i32) < BATCH
                                || skip + (batch.len() as i32) >= total;
                            let _ = tx.send(ApiResult::ProblemBatch {
                                problems: batch,
                                total,
                                done,
                            });
                            if done {
                                break;
                            }
                            skip += BATCH;
                        }
                        Err(e) => {
                            let _ = tx.send(ApiResult::ProblemFetchError(format!("{e}")));
                            break;
                        }
                    }
                }
            });
        }
    }

    fn start_search_fetch(&self, query: &str) {
        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        let query = query.to_string();

        tokio::spawn(async move {
            let result = client.fetch_problems(1, 0, None, Some(&query)).await;
            let _ = tx.send(ApiResult::SearchResult(result));
        });
    }

    fn start_fetch_favorites(&self) {
        let client = self.api_client.clone();
        let tx = self.api_tx.clone();

        tokio::spawn(async move {
            let result = client.fetch_favorites().await;
            let _ = tx.send(ApiResult::Favorites(result));
        });
    }

    fn start_fetch_list_questions(&self, id_hash: &str) {
        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        let id_hash = id_hash.to_string();

        tokio::spawn(async move {
            let result = client.fetch_favorite_questions(&id_hash).await;
            let _ = tx.send(ApiResult::ListQuestions(id_hash, result));
        });
    }

    fn start_create_list(&self, name: &str) {
        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        let name = name.to_string();

        tokio::spawn(async move {
            let msg = format!("List \"{}\" created", name);
            let result = client.create_favorite_list(&name).await;
            let _ = tx.send(ApiResult::ListMutation(result, msg));
        });
    }

    fn start_delete_list(&self, id_hash: &str) {
        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        let id_hash = id_hash.to_string();

        tokio::spawn(async move {
            let result = client.delete_favorite_list(&id_hash).await;
            let _ = tx.send(ApiResult::ListMutation(result, "List deleted".into()));
        });
    }

    fn start_remove_from_list(&self, id_hash: &str, question_id: &str) {
        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        let id_hash = id_hash.to_string();
        let question_id = question_id.to_string();

        tokio::spawn(async move {
            let result = client.remove_from_favorite(&id_hash, &question_id).await;
            let _ = tx.send(ApiResult::ListMutation(result, "Removed from list".into()));
        });
    }

    fn open_add_to_list_popup(&mut self, question_id: String) {
        self.add_to_list_popup = Some(AddToListPopup {
            lists: Vec::new(),
            selected: 0,
            question_id,
            loading: true,
        });

        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        tokio::spawn(async move {
            let result = client.fetch_favorites().await;
            let _ = tx.send(ApiResult::PopupFavorites(result));
        });
    }

    fn start_add_to_list(&self, id_hash: &str, question_id: &str, list_name: &str) {
        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        let id_hash = id_hash.to_string();
        let question_id = question_id.to_string();
        let msg = format!("Added to \"{}\"", list_name);

        tokio::spawn(async move {
            let result = client.add_to_favorite(&id_hash, &question_id).await;
            let _ = tx.send(ApiResult::ListMutation(result, msg));
        });
    }

    fn start_fetch_user_stats(&self) {
        let client = self.api_client.clone();
        let tx = self.api_tx.clone();

        tokio::spawn(async move {
            let username = client.fetch_username().await;
            let stats = match username {
                Some(name) => client.fetch_user_stats(&name).await.ok(),
                None => None,
            };
            let _ = tx.send(ApiResult::UserStats(stats));
        });
    }

    fn start_fetch_detail(&self, slug: &str) {
        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        let slug = slug.to_string();

        tokio::spawn(async move {
            let result = client.fetch_problem_detail(&slug).await;
            let _ = tx.send(ApiResult::Detail(result));
        });
    }

    fn start_fetch_detail_for_scaffold(
        &mut self,
        slug: &str,
        _terminal: &mut ratatui::DefaultTerminal,
    ) -> Result<()> {
        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        let slug = slug.to_string();

        tokio::spawn(async move {
            let result = client.fetch_problem_detail(&slug).await;
            let _ = tx.send(ApiResult::Detail(result));
        });
        Ok(())
    }

    fn read_user_code(&self, detail: &QuestionDetail) -> Result<String> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No config loaded"))?;
        let workspace = config.expanded_workspace();
        let dir_name = format!("{}-{}", detail.frontend_question_id, detail.title_slug);
        let file_path = match config.language.as_str() {
            "rust" => workspace.join(&dir_name).join("src").join("main.rs"),
            "python3" | "python" => workspace.join(&dir_name).join("solution.py"),
            "cpp" | "c++" => workspace.join(&dir_name).join("solution.cpp"),
            "java" => workspace.join(&dir_name).join("Solution.java"),
            "javascript" => workspace.join(&dir_name).join("solution.js"),
            "typescript" => workspace.join(&dir_name).join("solution.ts"),
            "go" | "golang" => workspace.join(&dir_name).join("solution.go"),
            _ => workspace.join(&dir_name).join("src").join("main.rs"),
        };

        let content = std::fs::read_to_string(&file_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to read code from {}: {e}\nScaffold the problem first with 'o'",
                file_path.display()
            )
        })?;

        if config.language.eq_ignore_ascii_case("rust") {
            return extract_rust_solution(&content);
        }

        Ok(content)
    }

    fn lang_slug(&self) -> &str {
        let config = self.config.as_ref();
        match config.map(|c| c.language.as_str()) {
            Some("rust") => "rust",
            Some("python3") => "python3",
            Some("python") => "python3",
            Some("cpp" | "c++") => "cpp",
            Some("java") => "java",
            Some("javascript") => "javascript",
            Some("typescript") => "typescript",
            Some("go" | "golang") => "golang",
            _ => "rust",
        }
    }

    fn start_run_code(&mut self, detail: &QuestionDetail) {
        let config = match &self.config {
            Some(c) => c,
            None => {
                self.error_overlay = Some("No config loaded".to_string());
                return;
            }
        };

        if !config.is_authenticated() {
            self.error_overlay = Some("Authentication required.\nPress S for settings, or use Ctrl+L in settings for auto-login.".to_string());
            return;
        }

        let code = match self.read_user_code(detail) {
            Ok(c) => c,
            Err(e) => {
                self.error_overlay = Some(format!("{e}"));
                return;
            }
        };

        // Get test input from example testcases
        let data_input = detail
            .example_testcase_list
            .as_ref()
            .and_then(|v| {
                if v.is_empty() {
                    None
                } else {
                    Some(v.join("\n"))
                }
            })
            .or_else(|| detail.sample_test_case.clone())
            .unwrap_or_default();

        let title = format!("{}. {}", detail.frontend_question_id, detail.title);
        self.screen = Screen::Result(ResultState::new(ResultKind::Run, title, detail.clone()));

        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        let slug = detail.title_slug.clone();
        let question_id = detail.question_id.clone();
        let lang = self.lang_slug().to_string();

        tokio::spawn(async move {
            let result = async {
                let interpret_id = client
                    .run_code(&slug, &question_id, &lang, &code, &data_input)
                    .await?;
                client.poll_result(&interpret_id).await
            }
            .await;
            let _ = tx.send(ApiResult::RunResult(result));
        });
    }

    fn start_submit_code(&mut self, detail: &QuestionDetail) {
        let config = match &self.config {
            Some(c) => c,
            None => {
                self.error_overlay = Some("No config loaded".to_string());
                return;
            }
        };

        if !config.is_authenticated() {
            self.error_overlay = Some("Authentication required.\nPress S for settings, or use Ctrl+L in settings for auto-login.".to_string());
            return;
        }

        let code = match self.read_user_code(detail) {
            Ok(c) => c,
            Err(e) => {
                self.error_overlay = Some(format!("{e}"));
                return;
            }
        };

        let title = format!("{}. {}", detail.frontend_question_id, detail.title);
        self.screen = Screen::Result(ResultState::new(ResultKind::Submit, title, detail.clone()));

        let client = self.api_client.clone();
        let tx = self.api_tx.clone();
        let slug = detail.title_slug.clone();
        let question_id = detail.question_id.clone();
        let lang = self.lang_slug().to_string();

        tokio::spawn(async move {
            let result = async {
                let submission_id = client
                    .submit_code(&slug, &question_id, &lang, &code)
                    .await?;
                client.poll_result(&submission_id).await
            }
            .await;
            let _ = tx.send(ApiResult::SubmitResult(result));
        });
    }

    fn do_scaffold_and_edit(
        &mut self,
        detail: &QuestionDetail,
        terminal: &mut ratatui::DefaultTerminal,
        events: &EventHandler,
    ) -> Result<()> {
        let config = match &self.config {
            Some(c) => c.clone(),
            None => {
                self.error_overlay = Some("No config loaded".to_string());
                return Ok(());
            }
        };

        let workspace = config.expanded_workspace();

        if !workspace.exists() {
            let repo_url = config
                .solutions_repo_url
                .as_deref()
                .filter(|u| !u.is_empty());

            if let Some(url) = repo_url {
                if let Some(parent) = workspace.parent() {
                    std::fs::create_dir_all(parent).ok();
                }

                let clone_status = Command::new("git")
                    .args(["clone", url])
                    .arg(&workspace)
                    .status();

                match clone_status {
                    Ok(s) if s.success() => {}
                    Ok(s) => {
                        self.error_overlay = Some(format!(
                            "Failed to clone solutions repo '{url}' (status: {s})"
                        ));
                        return Ok(());
                    }
                    Err(e) => {
                        self.error_overlay = Some(format!("Failed to run 'git clone': {e}"));
                        return Ok(());
                    }
                }
            } else {
                std::fs::create_dir_all(&workspace).ok();
            }
        }

        match scaffold::scaffold_problem(&workspace, detail, &config.language) {
            Ok(file_path) => {
                let project_dir = file_path
                    .parent()
                    .and_then(|p| p.parent())
                    .unwrap_or(&workspace);
                self.last_opened_dir = Some(project_dir.to_path_buf());

                // Pause event reader so editor gets exclusive stdin access
                events.pause();
                ratatui::restore();

                let status = Command::new(&config.editor)
                    .arg(&file_path)
                    .current_dir(project_dir)
                    .status();

                *terminal = ratatui::init();
                events.resume();

                match status {
                    Ok(s) if s.success() => {}
                    Ok(s) => {
                        self.error_overlay = Some(format!("Editor exited with status: {}", s));
                    }
                    Err(e) => {
                        self.error_overlay = Some(format!(
                            "Failed to launch editor '{}': {}",
                            config.editor, e
                        ));
                    }
                }
            }
            Err(e) => {
                self.error_overlay = Some(format!("Scaffold failed: {e}"));
            }
        }

        Ok(())
    }

    fn pull_workspace_on_startup(&mut self) {
        let Some(config) = &self.config else {
            return;
        };
        let workspace = config.expanded_workspace();

        if !workspace.join(".git").exists() {
            return;
        }

        let pull_output = Command::new("git")
            .args(["pull", "origin", "main"])
            .current_dir(&workspace)
            .output();

        match pull_output {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                self.error_overlay = Some(format!("Failed to pull latest solutions: {stderr}"));
            }
            Err(e) => {
                self.error_overlay = Some(format!("Failed to run 'git pull': {e}"));
            }
        }
    }

    fn do_git_commit(
        &mut self,
        detail: &QuestionDetail,
        terminal: &mut ratatui::DefaultTerminal,
        events: &EventHandler,
    ) -> Result<()> {
        let config = match &self.config {
            Some(c) => c.clone(),
            None => {
                self.error_overlay = Some("No config loaded".to_string());
                return Ok(());
            }
        };

        let workspace = config.expanded_workspace();
        let dir_name = format!("{}-{}", detail.frontend_question_id, detail.title_slug);
        let project_dir = workspace.join(&dir_name);

        if !project_dir.exists() {
            self.error_overlay =
                Some("No scaffolded solution for this problem yet — press 'o' first.".to_string());
            return Ok(());
        }

        let current_branch = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&workspace)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        if current_branch != "main" {
            let checkout_output = Command::new("git")
                .args(["checkout", "main"])
                .current_dir(&workspace)
                .output();

            match checkout_output {
                Ok(o) if o.status.success() => {}
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    self.error_overlay =
                        Some(format!("Failed to switch to branch 'main': {stderr}"));
                    return Ok(());
                }
                Err(e) => {
                    self.error_overlay = Some(format!("Failed to run 'git checkout': {e}"));
                    return Ok(());
                }
            }
        }

        let pull_output = Command::new("git")
            .args(["pull", "origin", "main"])
            .current_dir(&workspace)
            .output();

        match pull_output {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                self.error_overlay = Some(format!("'git pull' failed: {stderr}"));
                return Ok(());
            }
            Err(e) => {
                self.error_overlay = Some(format!("Failed to run 'git pull': {e}"));
                return Ok(());
            }
        }

        let add_status = Command::new("git")
            .args(["add", "--"])
            .arg(&project_dir)
            .current_dir(&workspace)
            .status();

        match add_status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                self.error_overlay = Some(format!(
                    "'git add' failed (status: {s}). Is {} a git repository?",
                    workspace.display()
                ));
                return Ok(());
            }
            Err(e) => {
                self.error_overlay = Some(format!("Failed to run 'git add': {e}"));
                return Ok(());
            }
        }

        let commit_title = format!("{}. {}", detail.frontend_question_id, detail.title);

        // Pause event reader so the editor gets exclusive stdin access
        events.pause();
        ratatui::restore();

        let status = Command::new("git")
            .args(["commit", "--edit", "-m", &commit_title])
            .current_dir(&workspace)
            .status();

        *terminal = ratatui::init();
        events.resume();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                self.error_overlay = Some(format!(
                    "'git commit' exited with status: {s} (there may be nothing staged to commit)"
                ));
                return Ok(());
            }
            Err(e) => {
                self.error_overlay = Some(format!("Failed to run 'git commit': {e}"));
                return Ok(());
            }
        }

        let push_output = Command::new("git")
            .args(["push", "origin", "main"])
            .current_dir(&workspace)
            .output();

        match push_output {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                self.error_overlay = Some(format!("Committed, but 'git push' failed: {stderr}"));
            }
            Err(e) => {
                self.error_overlay =
                    Some(format!("Committed, but failed to run 'git push': {e}"));
            }
        }

        Ok(())
    }

    fn browser_login(&mut self) {
        let domains = vec!["leetcode.com".to_string()];
        let cookies = match rookie::load(Some(domains)) {
            Ok(c) => c,
            Err(_) => {
                let _ = Command::new("open")
                    .arg("https://leetcode.com/accounts/login/")
                    .spawn();
                self.login_waiting = true;
                return;
            }
        };

        let session = cookies
            .iter()
            .find(|c| c.name == "LEETCODE_SESSION")
            .map(|c| c.value.clone());
        let csrf = cookies
            .iter()
            .find(|c| c.name == "csrftoken")
            .map(|c| c.value.clone());
        let cf_clearance = cookies
            .iter()
            .find(|c| c.name == "cf_clearance")
            .map(|c| c.value.clone());

        if session.is_none() || csrf.is_none() {
            // No cookies found — open browser and wait for retry
            let _ = Command::new("open")
                .arg("https://leetcode.com/accounts/login/")
                .spawn();
            self.login_waiting = true;
            return;
        }

        self.apply_login_cookies(session, csrf, cf_clearance);
    }

    fn retry_browser_login(&mut self) {
        self.login_waiting = false;

        let domains = vec!["leetcode.com".to_string()];
        let cookies = match rookie::load(Some(domains)) {
            Ok(c) => c,
            Err(e) => {
                self.error_overlay = Some(format!(
                    "Still can't extract cookies: {e}\n\nMake sure you logged into leetcode.com,\nthen press Enter to retry."
                ));
                self.login_waiting = true;
                return;
            }
        };

        let session = cookies
            .iter()
            .find(|c| c.name == "LEETCODE_SESSION")
            .map(|c| c.value.clone());
        let csrf = cookies
            .iter()
            .find(|c| c.name == "csrftoken")
            .map(|c| c.value.clone());
        let cf_clearance = cookies
            .iter()
            .find(|c| c.name == "cf_clearance")
            .map(|c| c.value.clone());

        if session.is_none() || csrf.is_none() {
            self.error_overlay = Some(
                "Could not find LeetCode cookies.\n\nLog into leetcode.com in your browser,\nthen press Enter to retry.".to_string()
            );
            self.login_waiting = true;
            return;
        }

        self.apply_login_cookies(session, csrf, cf_clearance);
    }

    fn apply_login_cookies(
        &mut self,
        session: Option<String>,
        csrf: Option<String>,
        cf_clearance: Option<String>,
    ) {
        // Update config
        if let Some(ref mut config) = self.config {
            config.leetcode_session = session.clone();
            config.csrf_token = csrf.clone();
            config.cf_clearance = cf_clearance.clone();
            if let Err(e) = config.save() {
                self.error_overlay = Some(format!("Cookies found but failed to save config: {e}"));
                return;
            }
        }

        let user_agent = self.config.as_ref().and_then(|c| c.user_agent.clone());

        // Recreate client with new credentials
        match LeetCodeClient::new(
            session.as_deref(),
            csrf.as_deref(),
            cf_clearance.as_deref(),
            user_agent.as_deref(),
        ) {
            Ok(client) => {
                self.api_client = client;
                self.start_fetch_problems();
                self.start_fetch_user_stats();
            }
            Err(e) => {
                self.error_overlay = Some(format!("Failed to create client: {e}"));
            }
        }
    }
}

fn load_cached_problems() -> Option<Vec<ProblemSummary>> {
    let path = Config::cache_path();
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_problems_cache(problems: &[ProblemSummary]) {
    let path = Config::cache_path();
    if let Ok(data) = serde_json::to_string(problems) {
        let _ = std::fs::write(path, data);
    }
}

/// Extract the solution portion of a Rust file using tree-sitter.
///
/// Walks top-level AST nodes and keeps everything except:
/// - Leading line comments (problem description)
/// - `struct Solution;` (LSP shim we added)
/// - `fn main() { ... }`
/// - `#[cfg(test)] mod tests { ... }`
fn extract_rust_solution(content: &str) -> Result<String> {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter_rust::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| anyhow::anyhow!("Failed to set tree-sitter language: {e}"))?;

    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse Rust file"))?;

    let root = tree.root_node();
    let mut parts: Vec<&str> = Vec::new();
    let mut in_leading_comments = true;
    let mut skip_next = false;

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        // If the previous node was #[cfg(test)], skip this node (the mod item)
        if skip_next {
            skip_next = false;
            continue;
        }

        let kind = child.kind();
        let text = &content[child.byte_range()];

        // Skip leading line comments (problem description block)
        if in_leading_comments && kind == "line_comment" {
            continue;
        }
        if kind != "line_comment" {
            in_leading_comments = false;
        }

        // Skip empty `struct Solution` in any form: `struct Solution;`, `struct Solution {}`, etc.
        // These are LSP shims — LeetCode provides its own.
        if kind == "struct_item" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = &content[name_node.byte_range()];
                if name == "Solution" {
                    let has_fields = child.child_by_field_name("body").is_some_and(|body| {
                        let mut bc = body.walk();
                        body.children(&mut bc)
                            .any(|c| c.kind() == "field_declaration")
                    });
                    if !has_fields {
                        continue;
                    }
                }
            }
        }

        // Skip `fn main() { ... }`
        if kind == "function_item" {
            if let Some(name_node) = child.child_by_field_name("name") {
                if &content[name_node.byte_range()] == "main" {
                    continue;
                }
            }
        }

        // Skip `#[cfg(test)]` attribute and mark to skip the next item (mod tests)
        if kind == "attribute_item" && text.contains("cfg") && text.contains("test") {
            skip_next = true;
            continue;
        }

        parts.push(text);
    }

    let result = parts.join("\n").trim().to_string();
    if result.is_empty() {
        // Fallback: return original content if parsing produced nothing
        Ok(content.to_string())
    } else {
        Ok(result)
    }
}
