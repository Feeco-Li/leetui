pub const PROBLEM_LIST_QUERY: &str = r#"
query problemsetQuestionList($categorySlug: String, $limit: Int, $skip: Int, $filters: QuestionListFilterInput) {
  problemsetQuestionList: questionList(
    categorySlug: $categorySlug
    limit: $limit
    skip: $skip
    filters: $filters
  ) {
    total: totalNum
    questions: data {
      frontendQuestionId: questionFrontendId
      title
      titleSlug
      difficulty
      status
      acRate
      isPaidOnly
      topicTags {
        name
        slug
      }
    }
  }
}
"#;

pub const QUESTION_DETAIL_QUERY: &str = r#"
query questionDetail($titleSlug: String!) {
  question(titleSlug: $titleSlug) {
    questionId
    frontendQuestionId: questionFrontendId
    title
    titleSlug
    difficulty
    content
    isPaidOnly
    topicTags {
      name
      slug
    }
    codeSnippets {
      lang
      langSlug
      code
    }
    exampleTestcaseList
    sampleTestCase
    hints
    status
  }
}
"#;

pub const GLOBAL_DATA_QUERY: &str = r#"
query {
  userStatus {
    isSignedIn
    username
  }
}
"#;

/// Deliberately does *not* request the nested `questions` field that
/// LeetCode's schema technically allows here: for the built-in "Favorite"
/// list it returns stale/phantom data (observed: 52 questions from what
/// looks like a legacy star-bookmark system, when the actual list is
/// empty on leetcode.com itself), and it's simply always empty for custom
/// lists. `FAVORITE_QUESTION_LIST_QUERY` below -- the same per-list query
/// the website's own problem-list page uses -- is the only reliable
/// source for a list's contents, so every list's problems are always
/// fetched that way instead.
pub const FAVORITES_LIST_QUERY: &str = r#"
query favoritesList {
  favoritesLists {
    allFavorites {
      idHash
      name
      description
      viewCount
      creator
      isWatched
      isPublicFavorite
    }
  }
}
"#;

pub const FAVORITE_QUESTION_LIST_QUERY: &str = r#"
query favoriteQuestionList($favoriteSlug: String!, $skip: Int, $limit: Int) {
  favoriteQuestionList(favoriteSlug: $favoriteSlug, skip: $skip, limit: $limit) {
    questions {
      questionFrontendId
      title
      titleSlug
      status
    }
    totalLength
  }
}
"#;

pub const USER_PROFILE_QUERY: &str = r#"
query getUserProfile($username: String!) {
  matchedUser(username: $username) {
    submitStats {
      acSubmissionNum {
        difficulty
        count
      }
    }
  }
  allQuestionsCount {
    difficulty
    count
  }
}
"#;
