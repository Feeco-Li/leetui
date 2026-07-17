use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::api::types::QuestionDetail;

pub fn scaffold_python(workspace: &PathBuf, detail: &QuestionDetail) -> Result<PathBuf> {
    let dir_name = format!("{}-{}", detail.frontend_question_id, detail.title_slug);
    let project_dir = workspace.join(&dir_name);
    let solution_py = project_dir.join("solution.py");

    // Idempotent: skip if already exists
    if solution_py.exists() {
        return Ok(solution_py);
    }

    std::fs::create_dir_all(&project_dir)
        .with_context(|| format!("Failed to create dir {}", project_dir.display()))?;

    let mut src = String::new();

    // Problem description as comments
    src.push_str(&format!("# {}: {}\n", detail.frontend_question_id, detail.title));
    src.push_str(&format!("# Difficulty: {}\n", detail.difficulty));
    src.push_str(&format!(
        "# https://leetcode.com/problems/{}/\n",
        detail.title_slug
    ));
    src.push_str("#\n");

    // Add description as comments
    if let Some(ref html) = detail.content {
        let text = html2text::from_read(html.as_bytes(), 80).unwrap_or_default();
        for line in text.lines().take(50) {
            src.push_str(&format!("# {}\n", line));
        }
    }

    src.push('\n');

    // Code snippet
    let snippet = detail
        .code_snippets
        .as_ref()
        .and_then(|snippets| snippets.iter().find(|s| s.lang_slug == "python3"))
        .map(|s| s.code.as_str())
        .unwrap_or("# No Python snippet available for this problem\n");

    src.push_str(snippet);
    src.push('\n');

    std::fs::write(&solution_py, src)
        .with_context(|| format!("Failed to write {}", solution_py.display()))?;

    Ok(solution_py)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::CodeSnippet;

    fn sample_detail() -> QuestionDetail {
        QuestionDetail {
            question_id: "1".into(),
            frontend_question_id: "1".into(),
            title: "Two Sum".into(),
            title_slug: "two-sum".into(),
            difficulty: "Easy".into(),
            content: Some("<p>Given an array...</p>".into()),
            is_paid_only: false,
            topic_tags: vec![],
            code_snippets: Some(vec![CodeSnippet {
                lang: "Python3".into(),
                lang_slug: "python3".into(),
                code: "class Solution:\n    def twoSum(self, nums, target):\n        pass".into(),
            }]),
            example_testcase_list: None,
            sample_test_case: None,
            hints: vec![],
            status: None,
        }
    }

    #[test]
    fn scaffolds_solution_py_with_snippet() {
        let tmp = std::env::temp_dir().join(format!("leetui-scaffold-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        let path = scaffold_python(&tmp, &sample_detail()).unwrap();
        assert_eq!(path, tmp.join("1-two-sum/solution.py"));

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# 1: Two Sum"));
        assert!(content.contains("class Solution:"));
        assert!(content.contains("def twoSum"));
        assert!(!content.contains("unittest"));
        assert!(!content.contains("if __name__ == \"__main__\":"));

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn is_idempotent() {
        let tmp = std::env::temp_dir().join(format!("leetui-scaffold-test-idem-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        let path1 = scaffold_python(&tmp, &sample_detail()).unwrap();
        std::fs::write(&path1, "# user edits preserved\n").unwrap();

        let path2 = scaffold_python(&tmp, &sample_detail()).unwrap();
        let content = std::fs::read_to_string(&path2).unwrap();
        assert_eq!(content, "# user edits preserved\n");

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
