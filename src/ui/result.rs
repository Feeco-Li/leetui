use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::api::types::CheckResponse;

use super::status_bar::render_status_bar;

#[derive(Debug, Clone, Copy)]
pub enum ResultKind {
    Run,
    Submit,
}

#[derive(Debug, Clone)]
pub struct ResultData {
    pub status_msg: String,
    pub status_code: i32,
    pub total_correct: Option<i32>,
    pub total_testcases: Option<i32>,
    pub runtime: Option<String>,
    pub memory: Option<String>,
    pub code_output: Option<Vec<String>>,
    pub expected_output: Option<String>,
    pub last_testcase: Option<String>,
    pub compile_error: Option<String>,
    pub runtime_error: Option<String>,
    pub runtime_percentile: Option<f64>,
    pub memory_percentile: Option<f64>,
    pub stdout: Option<Vec<String>>,
    /// One entry per sample testcase (plus a trailing empty padding entry
    /// LeetCode always appends) -- unlike `code_output`/`stdout` above,
    /// which are flattened across all cases. Only meaningful for Run.
    pub per_case_output: Option<Vec<String>>,
    pub per_case_expected: Option<Vec<String>>,
    pub per_case_stdout: Option<Vec<String>>,
    /// One char per testcase ('1' pass / '0' fail).
    pub compare_result: Option<String>,
}

impl ResultData {
    pub fn from_check(resp: &CheckResponse) -> Self {
        let stdout = resp
            .std_output_list
            .as_ref()
            .map(|v| {
                v.iter()
                    .map(|s| s.trim_end_matches('\n').to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty());

        // LeetCode's Run/interpret endpoint reports status_code 10
        // ("Accepted") as long as the code executes without a runtime
        // error, even when the output doesn't match -- `correct_answer`
        // is the field that actually reflects pass/fail there. Submit
        // responses already set status_code correctly, so this is a
        // no-op for them.
        let (status_code, status_msg) = if resp.correct_answer == Some(false)
            && resp.status_code == Some(10)
        {
            (11, "Wrong Answer".to_string())
        } else {
            (
                resp.status_code.unwrap_or(-1),
                resp.status_msg.clone().unwrap_or_default(),
            )
        };

        Self {
            status_msg,
            status_code,
            total_correct: resp.total_correct,
            total_testcases: resp.total_testcases,
            runtime: resp.status_runtime.clone(),
            memory: resp.status_memory.clone(),
            code_output: resp.code_answer.clone().or(resp.code_output.clone()),
            expected_output: resp.expected_output.clone().or_else(|| {
                resp.expected_code_answer.as_ref().map(|v| v.join("\n"))
            }),
            last_testcase: resp.last_testcase.clone(),
            compile_error: resp.full_compile_error.clone().or(resp.compile_error.clone()),
            runtime_error: resp.full_runtime_error.clone().or(resp.runtime_error.clone()),
            runtime_percentile: resp.runtime_percentile,
            memory_percentile: resp.memory_percentile,
            stdout,
            per_case_output: resp.code_answer.clone().or(resp.code_output.clone()),
            per_case_expected: resp.expected_code_answer.clone(),
            per_case_stdout: resp.std_output_list.clone(),
            compare_result: resp.compare_result.clone(),
        }
    }
}

/// The raw sample testcase inputs, in the same order they were sent to
/// Run/Submit -- this is what `code_answer`/`expected_code_answer`/
/// `std_output_list` line up against index-for-index.
fn sample_testcases(detail: &crate::api::types::QuestionDetail) -> Vec<String> {
    detail
        .example_testcase_list
        .as_ref()
        .filter(|v| !v.is_empty())
        .cloned()
        .or_else(|| detail.sample_test_case.clone().map(|s| vec![s]))
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub enum ResultStatus {
    Pending,
    Success(ResultData),
    Error(String),
}

pub struct ResultState {
    pub kind: ResultKind,
    pub status: ResultStatus,
    pub problem_title: String,
    pub scroll_offset: u16,
    pub spinner_frame: usize,
    pub content_lines: Vec<Line<'static>>,
    pub content_height: u16,
    pub detail: crate::api::types::QuestionDetail,
}

impl ResultState {
    pub fn new(kind: ResultKind, problem_title: String, detail: crate::api::types::QuestionDetail) -> Self {
        Self {
            kind,
            status: ResultStatus::Pending,
            problem_title,
            scroll_offset: 0,
            spinner_frame: 0,
            content_lines: Vec::new(),
            content_height: 0,
            detail,
        }
    }

    pub fn set_result(&mut self, data: ResultData) {
        let testcases = sample_testcases(&self.detail);
        self.content_lines = build_result_lines(&data, self.kind, &testcases);
        self.status = ResultStatus::Success(data);
    }

    pub fn set_error(&mut self, msg: String) {
        self.content_lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  Error: {msg}"),
                Style::default().fg(Color::Red),
            )),
        ];
        self.status = ResultStatus::Error(msg);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ResultAction {
        match key.code {
            KeyCode::Char('b') | KeyCode::Char('q') | KeyCode::Esc => ResultAction::Back,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ResultAction::Quit
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll(1);
                ResultAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll(-1);
                ResultAction::None
            }
            _ => ResultAction::None,
        }
    }

    fn scroll(&mut self, delta: i32) {
        let new_offset = self.scroll_offset as i32 + delta;
        self.scroll_offset = new_offset.max(0) as u16;
    }
}

pub enum ResultAction {
    None,
    Back,
    Quit,
}

pub fn render_result(frame: &mut Frame, area: Rect, state: &mut ResultState) {
    let layout = Layout::vertical([
        Constraint::Length(3), // title bar
        Constraint::Min(3),   // content
        Constraint::Length(1), // status bar
    ])
    .split(area);

    // Title bar
    let kind_label = match state.kind {
        ResultKind::Run => "Run (sample cases)",
        ResultKind::Submit => "Submit (all cases)",
    };
    let title_line = Line::from(vec![
        Span::styled(
            format!(" {kind_label} Result "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            &state.problem_title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let title_block = Paragraph::new(vec![title_line])
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(title_block, layout[0]);

    // Content area
    state.content_height = layout[1].height;

    if matches!(state.status, ResultStatus::Pending) {
        let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let s = spinner[state.spinner_frame % spinner.len()];
        let elapsed = state.spinner_frame / 10; // 100ms tick rate
        let kind_verb = match state.kind {
            ResultKind::Run => "Running",
            ResultKind::Submit => "Submitting",
        };
        let loading = Paragraph::new(format!("\n  {s} {kind_verb}... ({elapsed}s)"))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, layout[1]);
    } else {
        let total_lines = state.content_lines.len() as u16;
        let max_scroll = total_lines.saturating_sub(state.content_height);
        if state.scroll_offset > max_scroll {
            state.scroll_offset = max_scroll;
        }

        let content = Paragraph::new(state.content_lines.clone())
            .block(Block::default().borders(Borders::NONE))
            .wrap(Wrap { trim: false })
            .scroll((state.scroll_offset, 0));

        frame.render_widget(content, layout[1]);
    }

    // Status bar
    render_status_bar(
        frame,
        layout[2],
        &[
            ("j/k", "Scroll"),
            ("b/q/Esc", "Back"),
            ("Ctrl+C", "Quit"),
            ("?", "Help"),
        ],
    );
}

fn build_result_lines(data: &ResultData, kind: ResultKind, testcases: &[String]) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(""));

    // Status code 10 = Accepted, 11 = Wrong Answer, 12 = MLE, 13 = Output Limit,
    // 14 = TLE, 15 = Runtime Error, 20 = Compile Error
    let (icon, color) = match data.status_code {
        10 => ("✔", Color::Green),
        20 => ("✘", Color::Red),
        14 => ("⏱", Color::Yellow),
        15 => ("!", Color::Red),
        _ => ("✘", Color::Red),
    };

    lines.push(Line::from(Span::styled(
        format!("  {icon} {}", data.status_msg),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Passed count
    if let (Some(correct), Some(total)) = (data.total_correct, data.total_testcases) {
        lines.push(Line::from(vec![
            Span::styled("  Passed: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{correct} / {total}"),
                Style::default().fg(if correct == total { Color::Green } else { Color::Yellow }),
            ),
        ]));
    }

    // Runtime & memory, with percentile when LeetCode provides one
    // (Submit-only, and only once enough accepted submissions exist to
    // compare against).
    if let Some(ref rt) = data.runtime {
        let suffix = percentile_suffix(data.runtime_percentile);
        lines.push(Line::from(vec![
            Span::styled("  Runtime: ", Style::default().fg(Color::White)),
            Span::styled(format!("{rt}{suffix}"), Style::default().fg(Color::Cyan)),
        ]));
    }
    if let Some(ref mem) = data.memory {
        let suffix = percentile_suffix(data.memory_percentile);
        lines.push(Line::from(vec![
            Span::styled("  Memory: ", Style::default().fg(Color::White)),
            Span::styled(format!("{mem}{suffix}"), Style::default().fg(Color::Cyan)),
        ]));
    }

    // Compile error
    if let Some(ref err) = data.compile_error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Compile Error:",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        for line in err.lines() {
            lines.push(Line::from(Span::styled(
                format!    ("  {line}"),
                Style::default().fg(Color::Red),
            )));
        }
    }

    // Runtime error (e.g. an uncaught exception) -- previously silently
    // dropped since `CheckResponse` didn't even deserialize this field.
    if let Some(ref err) = data.runtime_error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Runtime Error:",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        for line in err.lines() {
            lines.push(Line::from(Span::styled(
                format!("    {line}"),
                Style::default().fg(Color::Red),
            )));
        }
    }

    if matches!(kind, ResultKind::Run) {
        lines.extend(build_testcase_breakdown(data, testcases));
    } else {
        // Submit only ever reports the first failing case (not every
        // sample case like Run), but rendered with the same visual
        // language: pass/fail marker, input, stdout, output, expected.
        lines.extend(build_submit_case_block(data));
    }

    lines
}

fn percentile_suffix(percentile: Option<f64>) -> String {
    match percentile {
        Some(p) if p > 0.0 => format!(" (beats {p:.1}%)"),
        _ => String::new(),
    }
}

/// Submit's equivalent of `build_testcase_breakdown`, but for the single
/// failing case it reports (or nothing, if the submission was accepted).
fn build_submit_case_block(data: &ResultData) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    let has_content =
        data.last_testcase.is_some() || data.expected_output.is_some() || data.code_output.is_some();
    if !has_content {
        return lines;
    }

    let passed = data.status_code == 10;
    let (icon, color) = if passed {
        ("\u{2714}", Color::Green)
    } else {
        ("\u{2718}", Color::Red)
    };

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  Testcase {icon}"),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )));

    if let Some(ref input) = data.last_testcase {
        lines.push(Line::from(Span::styled(
            "    Input:",
            Style::default().fg(Color::White),
        )));
        for line in input.lines() {
            lines.push(Line::from(Span::styled(
                format!("      {line}"),
                Style::default().fg(Color::Gray),
            )));
        }
    }

    if let Some(ref stdout) = data.stdout {
        if !stdout.is_empty() {
            lines.push(Line::from(Span::styled(
                "    Stdout:",
                Style::default().fg(Color::White),
            )));
            for entry in stdout {
                for line in entry.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("      {line}"),
                        Style::default().fg(Color::Gray),
                    )));
                }
            }
        }
    }

    if let Some(ref output) = data.code_output {
        let output_color = if passed { Color::White } else { Color::Red };
        lines.push(Line::from(Span::styled(
            "    Output:",
            Style::default().fg(output_color),
        )));
        for line in output {
            lines.push(Line::from(Span::styled(
                format!("      {line}"),
                Style::default().fg(output_color),
            )));
        }
    }

    if !passed {
        if let Some(ref expected) = data.expected_output {
            lines.push(Line::from(Span::styled(
                "    Expected:",
                Style::default().fg(Color::Green),
            )));
            for line in expected.lines() {
                lines.push(Line::from(Span::styled(
                    format!("      {line}"),
                    Style::default().fg(Color::Green),
                )));
            }
        }
    }

    lines
}

/// One block per sample testcase: pass/fail marker, input, this run's
/// stdout, actual output, and (if it didn't pass) the expected output.
fn build_testcase_breakdown(data: &ResultData, testcases: &[String]) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    for (i, testcase) in testcases.iter().enumerate() {
        let passed = data
            .compare_result
            .as_ref()
            .and_then(|c| c.as_bytes().get(i))
            .map(|b| *b == b'1');
        let (icon, color) = match passed {
            Some(true) => ("\u{2714}", Color::Green),
            Some(false) => ("\u{2718}", Color::Red),
            None => ("\u{25cb}", Color::DarkGray),
        };

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  Testcase {} {icon}", i + 1),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));

        lines.push(Line::from(Span::styled(
            "    Input:",
            Style::default().fg(Color::White),
        )));
        for line in testcase.lines() {
            lines.push(Line::from(Span::styled(
                format!("      {line}"),
                Style::default().fg(Color::Gray),
            )));
        }

        if let Some(stdout) = data
            .per_case_stdout
            .as_ref()
            .and_then(|v| v.get(i))
            .map(|s| s.trim_end_matches('\n'))
            .filter(|s| !s.is_empty())
        {
            lines.push(Line::from(Span::styled(
                "    Stdout:",
                Style::default().fg(Color::White),
            )));
            for line in stdout.lines() {
                lines.push(Line::from(Span::styled(
                    format!("      {line}"),
                    Style::default().fg(Color::Gray),
                )));
            }
        }

        if let Some(output) = data.per_case_output.as_ref().and_then(|v| v.get(i)) {
            let output_color = if passed == Some(false) { Color::Red } else { Color::White };
            lines.push(Line::from(Span::styled(
                "    Output:",
                Style::default().fg(output_color),
            )));
            for line in output.lines() {
                lines.push(Line::from(Span::styled(
                    format!("      {line}"),
                    Style::default().fg(output_color),
                )));
            }
        }

        if passed != Some(true) {
            if let Some(expected) = data.per_case_expected.as_ref().and_then(|v| v.get(i)) {
                lines.push(Line::from(Span::styled(
                    "    Expected:",
                    Style::default().fg(Color::Green),
                )));
                for line in expected.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("      {line}"),
                        Style::default().fg(Color::Green),
                    )));
                }
            }
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_check_surfaces_print_output_as_stdout() {
        let body = r#"{
            "status_code": 10,
            "status_runtime": "0 ms",
            "code_answer": ["[0,1]", ""],
            "std_output_list": ["DEBUG_PRINT_MARKER [2, 7, 11, 15] 9\n", ""],
            "total_correct": 1,
            "total_testcases": 1,
            "state": "SUCCESS"
        }"#;
        let resp: CheckResponse = serde_json::from_str(body).unwrap();
        let data = ResultData::from_check(&resp);

        assert_eq!(
            data.stdout,
            Some(vec!["DEBUG_PRINT_MARKER [2, 7, 11, 15] 9".to_string()])
        );
        // The return value shown as "Output:" must stay the code_answer,
        // not be shadowed by stdout.
        assert_eq!(data.code_output, Some(vec!["[0,1]".to_string(), String::new()]));
    }

    #[test]
    fn from_check_omits_stdout_when_absent() {
        let body = r#"{
            "status_code": 10,
            "code_answer": ["[0,1]", ""],
            "std_output_list": ["", ""],
            "state": "SUCCESS"
        }"#;
        let resp: CheckResponse = serde_json::from_str(body).unwrap();
        let data = ResultData::from_check(&resp);

        assert_eq!(data.stdout, None);
    }

    #[test]
    fn from_check_overrides_misleading_accepted_status_on_run() {
        // Captured from a real `interpret_solution` + `check` round trip:
        // LeetCode reports status_code 10 / "Accepted" even though
        // correct_answer is false and no testcases actually passed.
        let body = r#"{
            "status_code": 10,
            "status_msg": "Accepted",
            "code_answer": ["[1,1,2]", "[1,1,2,3]", ""],
            "expected_code_answer": ["[1,2]", "[1,2,3]", ""],
            "correct_answer": false,
            "total_correct": 0,
            "total_testcases": 2,
            "state": "SUCCESS"
        }"#;
        let resp: CheckResponse = serde_json::from_str(body).unwrap();
        let data = ResultData::from_check(&resp);

        assert_eq!(data.status_code, 11);
        assert_eq!(data.status_msg, "Wrong Answer");
    }

    #[test]
    fn from_check_keeps_accepted_status_when_correct() {
        let body = r#"{
            "status_code": 10,
            "status_msg": "Accepted",
            "correct_answer": true,
            "total_correct": 2,
            "total_testcases": 2,
            "state": "SUCCESS"
        }"#;
        let resp: CheckResponse = serde_json::from_str(body).unwrap();
        let data = ResultData::from_check(&resp);

        assert_eq!(data.status_code, 10);
        assert_eq!(data.status_msg, "Accepted");
    }

    #[test]
    fn testcase_breakdown_pairs_each_case_with_its_own_io() {
        // Same real round trip as above: 2 sample cases, both failed.
        let body = r#"{
            "status_code": 10,
            "code_answer": ["[1,1,2]", "[1,1,2,3]", ""],
            "expected_code_answer": ["[1,2]", "[1,2,3]", ""],
            "std_output_list": ["1\n", "1\n2\n", ""],
            "compare_result": "00",
            "correct_answer": false,
            "total_correct": 0,
            "total_testcases": 2,
            "state": "SUCCESS"
        }"#;
        let resp: CheckResponse = serde_json::from_str(body).unwrap();
        let data = ResultData::from_check(&resp);
        let testcases = vec!["[1,1,2]".to_string(), "[1,1,2,3,3]".to_string()];

        let lines = build_testcase_breakdown(&data, &testcases);
        let rendered: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect();

        assert!(rendered.iter().any(|l| l.contains("Testcase 1")));
        assert!(rendered.iter().any(|l| l.contains("Testcase 2")));
        // Each case's own input, not the other case's.
        let tc1_pos = rendered.iter().position(|l| l.contains("Testcase 1")).unwrap();
        let tc2_pos = rendered.iter().position(|l| l.contains("Testcase 2")).unwrap();
        assert!(rendered[tc1_pos..tc2_pos].iter().any(|l| l.contains("[1,1,2]") && !l.contains("3,3")));
        assert!(rendered[tc2_pos..].iter().any(|l| l.contains("[1,1,2,3,3]")));
        // Case 2's stdout is "1\n2\n" -- two separate print()s -- not
        // flattened together with case 1's.
        assert!(rendered[tc2_pos..].iter().any(|l| l.trim() == "1"));
        assert!(rendered[tc2_pos..].iter().any(|l| l.trim() == "2"));
    }

    fn render(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect()
    }

    #[test]
    fn submit_case_block_shows_failing_case_like_run_breakdown() {
        // Matches the real Submit response the user pasted: Wrong Answer,
        // 0/168, single failing testcase, no runtime/memory (N/A).
        let body = r#"{
            "status_code": 11,
            "status_msg": "Wrong Answer",
            "last_testcase": "[1,1,2]",
            "expected_output": "[1,2]",
            "code_output": ["[1,1,2]"],
            "total_correct": 0,
            "total_testcases": 168,
            "state": "SUCCESS"
        }"#;
        let resp: CheckResponse = serde_json::from_str(body).unwrap();
        let data = ResultData::from_check(&resp);
        let lines = build_submit_case_block(&data);
        let rendered = render(&lines);

        assert!(rendered.iter().any(|l| l.contains("Testcase") && l.contains('\u{2718}')));
        assert!(rendered.iter().any(|l| l.trim() == "[1,1,2]"));
        assert!(rendered.iter().any(|l| l.contains("Expected:")));
    }

    #[test]
    fn submit_case_block_empty_when_accepted() {
        let body = r#"{
            "status_code": 10,
            "status_msg": "Accepted",
            "total_correct": 168,
            "total_testcases": 168,
            "state": "SUCCESS"
        }"#;
        let resp: CheckResponse = serde_json::from_str(body).unwrap();
        let data = ResultData::from_check(&resp);
        assert!(build_submit_case_block(&data).is_empty());
    }

    #[test]
    fn runtime_error_surfaces_in_result_lines() {
        // Captured from a real interpret_solution round trip that raised
        // ZeroDivisionError -- previously this field wasn't deserialized
        // at all, so nothing but "Runtime Error" showed.
        let body = r#"{
            "status_code": 15,
            "status_msg": "Runtime Error",
            "runtime_error": "Line 3: ZeroDivisionError: division by zero",
            "full_runtime_error": "ZeroDivisionError: division by zero\nLine 3 in deleteDuplicates",
            "state": "SUCCESS"
        }"#;
        let resp: CheckResponse = serde_json::from_str(body).unwrap();
        let data = ResultData::from_check(&resp);

        assert_eq!(
            data.runtime_error.as_deref(),
            Some("ZeroDivisionError: division by zero\nLine 3 in deleteDuplicates")
        );

        let lines = build_result_lines(&data, ResultKind::Run, &[]);
        let rendered = render(&lines);
        assert!(rendered.iter().any(|l| l.contains("Runtime Error:")));
        assert!(rendered.iter().any(|l| l.contains("ZeroDivisionError")));
    }

    #[test]
    fn percentile_shown_when_present_omitted_when_zero() {
        assert_eq!(percentile_suffix(Some(42.5)), " (beats 42.5%)");
        assert_eq!(percentile_suffix(Some(0.0)), "");
        assert_eq!(percentile_suffix(None), "");
    }
}
