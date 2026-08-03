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

/// `favoritesLists.allFavorites` (LeetCode's older batch query) only ever
/// returns lists the current user created -- it does not include lists
/// they've saved/collected from other users (leetcode.com's "Saved by
/// me" section), no matter what fields are requested, and its nested
/// `questions` field returns stale/phantom data for the built-in
/// "Favorite" list (observed: 52 questions from what looks like a legacy
/// star-bookmark system, when the actual list is empty on leetcode.com
/// itself). `myCreatedFavoriteList`/`myCollectedFavoriteList` are the
/// pair leetcode.com's own problem-list sidebar uses to populate "My
/// Lists" and "Saved by me" respectively; `idHash: slug` aliases the
/// field back to the name the rest of this app expects. Problems are
/// still always fetched per-list via `FAVORITE_QUESTION_LIST_QUERY`
/// (the same one the website uses) since neither field here returns
/// nested questions.
pub const FAVORITES_LIST_QUERY: &str = r#"
query favoritesList {
  myCreatedFavoriteList {
    favorites {
      idHash: slug
      name
      isPublicFavorite
    }
  }
  myCollectedFavoriteList {
    favorites {
      idHash: slug
      name
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
