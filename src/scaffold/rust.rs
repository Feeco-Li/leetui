use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::api::types::QuestionDetail;

pub fn scaffold_rust(workspace: &PathBuf, detail: &QuestionDetail) -> Result<PathBuf> {
    let dir_name = format!(
        "{}-{}",
        detail.frontend_question_id,
        detail.title_slug
    );
    // Cargo package names can't start with a digit, so prefix with "p"
    let pkg_name = format!("p{dir_name}");
    let project_dir = workspace.join(&dir_name);

    // Idempotent: skip if already exists
    if project_dir.join("Cargo.toml").exists() {
        return Ok(project_dir.join("src/main.rs"));
    }

    // Create project with cargo init
    std::fs::create_dir_all(&project_dir)
        .with_context(|| format!("Failed to create dir {}", project_dir.display()))?;

    let output = Command::new("cargo")
        .args(["init", "--name", &pkg_name, "--vcs", "none"])
        .current_dir(&project_dir)
        .output()
        .context("Failed to run cargo init")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo init failed: {}", stderr);
    }

    // Build the source file content
    let mut src = String::new();

    // Problem description as comments
    src.push_str(&format!("// {}: {}\n", detail.frontend_question_id, detail.title));
    src.push_str(&format!("// Difficulty: {}\n", detail.difficulty));
    src.push_str(&format!(
        "// https://leetcode.com/problems/{}/\n",
        detail.title_slug
    ));
    src.push_str("//\n");

    // Add description as comments
    if let Some(ref html) = detail.content {
        let text = html2text::from_read(html.as_bytes(), 80)
            .unwrap_or_default();
        for line in text.lines().take(50) {
            src.push_str(&format!("// {}\n", line));
        }
    }

    src.push('\n');

    // Code snippet
    let snippet = detail
        .code_snippets
        .as_ref()
        .and_then(|snippets| snippets.iter().find(|s| s.lang_slug == "rust"))
        .map(|s| s.code.as_str())
        .unwrap_or("// No Rust snippet available for this problem\n");

    // Add `struct Solution;` for LSP if snippet uses `impl Solution` but doesn't define the struct
    if snippet.contains("impl Solution") && !snippet.contains("struct Solution") {
        src.push_str("struct Solution;\n\n");
    }

    src.push_str(snippet);
    src.push('\n');

    // Main function with test stub
    src.push_str("\nfn main() {\n");
    src.push_str("    println!(\"Run with: cargo test\");\n");
    src.push_str("}\n");
    src.push_str("\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n");
    src.push_str("    #[test]\n    fn test_solution() {\n");
    src.push_str("        // TODO: add test cases\n");
    src.push_str("    }\n}\n");

    let main_rs = project_dir.join("src/main.rs");
    std::fs::write(&main_rs, src)
        .with_context(|| format!("Failed to write {}", main_rs.display()))?;

    Ok(main_rs)
}
