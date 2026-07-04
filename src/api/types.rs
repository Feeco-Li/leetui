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
    pub expected_output: Option<String>,
    pub last_testcase: Option<String>,
    pub total_correct: Option<i32>,
    pub total_testcases: Option<i32>,
    pub status_runtime: Option<String>,
    pub status_memory: Option<String>,
    pub compile_error: Option<String>,
    pub full_compile_error: Option<String>,
    pub correct_answer: Option<bool>,
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
