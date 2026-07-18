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
      questions {
        questionId
        status
        title
        titleSlug
      }
    }
  }
}
"#;

/// LeetCode's `favoritesLists` query above only returns non-empty nested
/// `questions` for the built-in "Favorite" list -- custom user-created lists
/// come back with an empty array regardless of their real contents. The
/// website itself fetches a custom list's problems separately via this
/// per-list query (keyed by the list's `idHash` as `favoriteSlug`), so we
/// have to do the same as a fallback when `questions` comes back empty.
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
