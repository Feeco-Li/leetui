use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct GraphQLResponse<T> {
    pub data: Option<T>,
}

// Problem list types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemListData {
    pub problemset_question_list: Option<ProblemsetQuestionList>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemsetQuestionList {
    pub total: i32,
    pub questions: Vec<ProblemSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemSummary {
    pub frontend_question_id: String,
    pub title: String,
    pub title_slug: String,
    pub difficulty: String,
    pub status: Option<String>,
    pub ac_rate: f64,
    pub is_paid_only: bool,
    pub topic_tags: Vec<TopicTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicTag {
    pub name: String,
    pub slug: String,
}

// Problem detail types
#[derive(Debug, Deserialize)]
pub struct QuestionDetailData {
    pub question: Option<QuestionDetail>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionDetail {
    pub question_id: String,
    pub frontend_question_id: String,
    pub title: String,
    pub title_slug: String,
    pub difficulty: String,
    pub content: Option<String>,
    pub is_paid_only: bool,
    pub topic_tags: Vec<TopicTag>,
    pub code_snippets: Option<Vec<CodeSnippet>>,
    pub example_testcase_list: Option<Vec<String>>,
    pub sample_test_case: Option<String>,
    pub hints: Vec<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeSnippet {
    pub lang: String,
    pub lang_slug: String,
    pub code: String,
}

// Run/submit response types
#[derive(Debug, Deserialize)]
pub struct InterpretResponse {
    pub interpret_id: Option<String>,
    pub interpret_expected_id: Option<String>,
    pub test_case: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitResponse {
    pub submission_id: Option<u64>,
    pub error: Option<String>,
}

/// LeetCode's "Run" check response sends `code_output` as an array (one entry
/// per sample case), but the "Submit" check response sends it as a single
/// string. Accept either shape.
fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        Vec(Vec<String>),
        String(String),
    }

    Ok(match Option::<StringOrVec>::deserialize(deserializer)? {
        Some(StringOrVec::Vec(v)) => Some(v),
        Some(StringOrVec::String(s)) if s.is_empty() => None,
        Some(StringOrVec::String(s)) => Some(s.lines().map(String::from).collect()),
        None => None,
    })
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct CheckResponse {
    pub state: String,
    pub status_msg: Option<String>,
    pub status_code: Option<i32>,
    pub code_answer: Option<Vec<String>>,
    pub expected_code_answer: Option<Vec<String>>,
    #[serde(deserialize_with = "deserialize_string_or_vec")]
    pub code_output: Option<Vec<String>>,
    /// stdout captured from `print()`/debug output during execution, one
    /// entry per test case (distinct from `code_output`, which holds the
    /// return-value comparison, not printed output).
    #[serde(deserialize_with = "deserialize_string_or_vec")]
    pub std_output_list: Option<Vec<String>>,
    pub expected_output: Option<String>,
    pub last_testcase: Option<String>,
    pub total_correct: Option<i32>,
    pub total_testcases: Option<i32>,
    pub status_runtime: Option<String>,
    pub status_memory: Option<String>,
    pub compile_error: Option<String>,
    pub full_compile_error: Option<String>,
    /// Short/full runtime error text (e.g. an uncaught exception traceback).
    /// Previously not deserialized at all, so a crash showed no detail
    /// beyond the bare "Runtime Error" status.
    pub runtime_error: Option<String>,
    pub full_runtime_error: Option<String>,
    pub correct_answer: Option<bool>,
    /// One char per sample testcase ('1' pass / '0' fail), only present on
    /// Run/interpret responses.
    pub compare_result: Option<String>,
    /// Only populated for accepted Submit responses, comparing against
    /// other accepted submissions.
    pub runtime_percentile: Option<f64>,
    pub memory_percentile: Option<f64>,
}

// User status types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStatusData {
    pub user_status: Option<UserStatus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStatus {
    pub is_signed_in: bool,
    pub username: Option<String>,
}

// User profile types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfileData {
    pub matched_user: Option<MatchedUser>,
    pub all_questions_count: Option<Vec<DifficultyCount>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedUser {
    pub submit_stats: Option<SubmitStats>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitStats {
    pub ac_submission_num: Vec<DifficultyCount>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DifficultyCount {
    pub difficulty: String,
    pub count: i32,
}

// Favorites list types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesListData {
    pub favorites_lists: Option<FavoritesLists>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesLists {
    pub all_favorites: Vec<FavoriteList>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteList {
    pub id_hash: String,
    pub name: String,
    pub description: Option<String>,
    pub view_count: i32,
    pub creator: String,
    pub is_watched: bool,
    pub is_public_favorite: bool,
    pub questions: Vec<FavoriteQuestion>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteQuestion {
    pub question_id: String,
    pub status: Option<String>,
    pub title: String,
    pub title_slug: String,
}

// Per-list question fetch (fallback for custom lists, see
// `FAVORITE_QUESTION_LIST_QUERY`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteQuestionListData {
    pub favorite_question_list: Option<FavoriteQuestionListResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteQuestionListResult {
    pub questions: Vec<FavoriteQuestionNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteQuestionNode {
    pub question_frontend_id: String,
    pub title: String,
    pub title_slug: String,
    /// "SOLVED" / "TO_DO" (uppercase enum), unlike the lowercase "ac" /
    /// "notac" `FavoriteQuestion::status` elsewhere -- normalized via
    /// `normalize_favorite_status`.
    pub status: Option<String>,
}

/// Maps this query's uppercase status enum ("SOLVED" / "TO_DO") to the
/// lowercase "ac" / "notac" convention `FavoriteQuestion::status` and the UI
/// already use elsewhere.
pub fn normalize_favorite_status(status: Option<&str>) -> Option<String> {
    match status {
        Some("SOLVED") => Some("ac".to_string()),
        Some("ATTEMPTED") => Some("notac".to_string()),
        _ => None,
    }
}

// Aggregated user stats
#[derive(Debug, Clone)]
pub struct UserStats {
    pub username: String,
    pub easy_solved: i32,
    pub easy_total: i32,
    pub medium_solved: i32,
    pub medium_total: i32,
    pub hard_solved: i32,
    pub hard_total: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from a real `interpret_solution` + `check` round trip against
    // leetcode.com for Two Sum with a `print()` call in the solution.
    #[test]
    fn parses_std_output_list_from_run_response() {
        let body = r#"{
            "status_code": 10,
            "lang": "python3",
            "run_success": true,
            "status_runtime": "0 ms",
            "code_answer": ["[0,1]", ""],
            "code_output": ["DEBUG_PRINT_MARKER [2, 7, 11, 15] 9"],
            "std_output_list": ["DEBUG_PRINT_MARKER [2, 7, 11, 15] 9\n", ""],
            "status_memory": "19.2 MB",
            "total_correct": 1,
            "total_testcases": 1,
            "state": "SUCCESS"
        }"#;

        let resp: CheckResponse = serde_json::from_str(body).unwrap();
        assert_eq!(
            resp.std_output_list,
            Some(vec![
                "DEBUG_PRINT_MARKER [2, 7, 11, 15] 9\n".to_string(),
                String::new()
            ])
        );
    }
}
