use anyhow::{Context, Result, bail};
use reqwest::{Client, RequestBuilder, cookie::Jar};
use serde_json::json;
use std::sync::Arc;

use super::queries::{FAVORITES_LIST_QUERY, GLOBAL_DATA_QUERY, PROBLEM_LIST_QUERY, QUESTION_DETAIL_QUERY, USER_PROFILE_QUERY};
use super::types::*;

const LEETCODE_GRAPHQL: &str = "https://leetcode.com/graphql";
const LEETCODE_RUN: &str = "https://leetcode.com/problems/{slug}/interpret_solution/";
const LEETCODE_SUBMIT: &str = "https://leetcode.com/problems/{slug}/submit/";
const LEETCODE_CHECK: &str = "https://leetcode.com/submissions/detail/{id}/check/";
const LEETCODE_LIST_API: &str = "https://leetcode.com/list/api/";
const LEETCODE_LIST_QUESTIONS_API: &str = "https://leetcode.com/list/api/questions";

#[cfg(target_os = "macos")]
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
#[cfg(target_os = "windows")]
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

#[derive(Clone)]
pub struct LeetCodeClient {
    client: Client,
    csrf_token: Option<String>,
}

impl LeetCodeClient {
    pub fn new(
        session: Option<&str>,
        csrf: Option<&str>,
        cf_clearance: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<Self> {
        let jar = Arc::new(Jar::default());
        let url = "https://leetcode.com".parse().unwrap();

        if let Some(session) = session {
            if !session.is_empty() {
                jar.add_cookie_str(&format!("LEETCODE_SESSION={session}"), &url);
            }
        }
        if let Some(csrf) = csrf {
            if !csrf.is_empty() {
                jar.add_cookie_str(&format!("csrftoken={csrf}"), &url);
            }
        }
        if let Some(cf_clearance) = cf_clearance {
            if !cf_clearance.is_empty() {
                jar.add_cookie_str(&format!("cf_clearance={cf_clearance}"), &url);
            }
        }

        let client = Client::builder()
            .cookie_provider(jar)
            .user_agent(user_agent.filter(|ua| !ua.is_empty()).unwrap_or(DEFAULT_USER_AGENT))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            csrf_token: csrf.map(String::from),
        })
    }

    fn auth_request(&self, builder: RequestBuilder) -> RequestBuilder {
        let builder = builder
            .header("Content-Type", "application/json")
            .header("Origin", "https://leetcode.com")
            .header("Referer", "https://leetcode.com");
        if let Some(ref token) = self.csrf_token {
            builder.header("x-csrftoken", token)
        } else {
            builder
        }
    }

    pub async fn fetch_problems(
        &self,
        limit: i32,
        skip: i32,
        difficulty: Option<&str>,
        search_keywords: Option<&str>,
    ) -> Result<(Vec<ProblemSummary>, i32)> {
        let mut filters = json!({});
        if let Some(diff) = difficulty {
            filters["difficulty"] = json!(diff);
        }
        if let Some(kw) = search_keywords {
            filters["searchKeywords"] = json!(kw);
        }

        let body = json!({
            "query": PROBLEM_LIST_QUERY,
            "variables": {
                "categorySlug": "all-code-essentials",
                "limit": limit,
                "skip": skip,
                "filters": filters,
            }
        });

        let resp = self
            .auth_request(self.client.post(LEETCODE_GRAPHQL))
            .json(&body)
            .send()
            .await
            .context("Failed to send problem list request")?;

        let data: GraphQLResponse<ProblemListData> = resp
            .json()
            .await
            .context("Failed to parse problem list response")?;

        let list = data
            .data
            .and_then(|d| d.problemset_question_list)
            .context("No problem list data in response")?;

        Ok((list.questions, list.total))
    }

    pub async fn fetch_problem_detail(&self, slug: &str) -> Result<QuestionDetail> {
        let body = json!({
            "query": QUESTION_DETAIL_QUERY,
            "variables": {
                "titleSlug": slug,
            }
        });

        let resp = self
            .auth_request(self.client.post(LEETCODE_GRAPHQL))
            .header("Referer", format!("https://leetcode.com/problems/{slug}/"))
            .json(&body)
            .send()
            .await
            .context("Failed to send problem detail request")?;

        let data: GraphQLResponse<QuestionDetailData> = resp
            .json()
            .await
            .context("Failed to parse problem detail response")?;

        data.data
            .and_then(|d| d.question)
            .context("No question data in response")
    }

    pub async fn run_code(
        &self,
        slug: &str,
        question_id: &str,
        lang: &str,
        typed_code: &str,
        data_input: &str,
    ) -> Result<String> {
        let url = LEETCODE_RUN.replace("{slug}", slug);
        let body = json!({
            "lang": lang,
            "question_id": question_id,
            "typed_code": typed_code,
            "data_input": data_input,
        });

        let resp = self
            .auth_request(self.client.post(&url))
            .header("Referer", format!("https://leetcode.com/problems/{slug}/"))
            .json(&body)
            .send()
            .await
            .context("Failed to send run request")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("LeetCode returned HTTP {status}: {body}");
        }

        let data: InterpretResponse = resp
            .json()
            .await
            .context("Failed to parse run response")?;

        if let Some(err) = data.error {
            bail!("LeetCode: {err}");
        }

        data.interpret_id.context("No interpret_id in response")
    }

    pub async fn submit_code(
        &self,
        slug: &str,
        question_id: &str,
        lang: &str,
        typed_code: &str,
    ) -> Result<String> {
        let url = LEETCODE_SUBMIT.replace("{slug}", slug);
        let body = json!({
            "lang": lang,
            "question_id": question_id,
            "typed_code": typed_code,
        });

        let resp = self
            .auth_request(self.client.post(&url))
            .header("Referer", format!("https://leetcode.com/problems/{slug}/"))
            .json(&body)
            .send()
            .await
            .context("Failed to send submit request")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("LeetCode returned HTTP {status}: {body}");
        }

        let data: SubmitResponse = resp
            .json()
            .await
            .context("Failed to parse submit response")?;

        if let Some(err) = data.error {
            bail!("LeetCode: {err}");
        }

        data.submission_id
            .map(|id| id.to_string())
            .context("No submission_id in response")
    }

    pub async fn check_result(&self, id: &str) -> Result<CheckResponse> {
        let url = LEETCODE_CHECK.replace("{id}", id);

        let resp = self
            .auth_request(self.client.get(&url))
            .header("Referer", "https://leetcode.com")
            .send()
            .await
            .context("Failed to send check request")?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .context("Failed to read check response body")?;

        if !status.is_success() {
            bail!("LeetCode returned HTTP {status} while checking result: {body}");
        }

        let data: CheckResponse = serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse check response: {body}"))?;

        Ok(data)
    }

    pub async fn poll_result(&self, id: &str) -> Result<CheckResponse> {
        let mut attempts = 0u32;
        loop {
            let result = self.check_result(id).await?;
            if result.state == "SUCCESS" {
                return Ok(result);
            }

            attempts += 1;
            if attempts > 30 {
                anyhow::bail!("Timed out waiting for result");
            }

            let delay = if attempts <= 3 { 1000 } else { 2000 };
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
    }

    pub async fn fetch_username(&self) -> Option<String> {
        let body = json!({
            "query": GLOBAL_DATA_QUERY,
            "variables": {}
        });

        let resp = self
            .auth_request(self.client.post(LEETCODE_GRAPHQL))
            .json(&body)
            .send()
            .await
            .ok()?;

        let data: GraphQLResponse<UserStatusData> = resp.json().await.ok()?;
        let status = data.data?.user_status?;
        if status.is_signed_in {
            status.username
        } else {
            None
        }
    }

    pub async fn fetch_user_stats(&self, username: &str) -> Result<UserStats> {
        let body = json!({
            "query": USER_PROFILE_QUERY,
            "variables": { "username": username }
        });

        let resp = self
            .auth_request(self.client.post(LEETCODE_GRAPHQL))
            .json(&body)
            .send()
            .await
            .context("Failed to send user profile request")?;

        let data: GraphQLResponse<UserProfileData> = resp
            .json()
            .await
            .context("Failed to parse user profile response")?;

        let profile = data.data.context("No profile data")?;

        let solved = profile
            .matched_user
            .and_then(|u| u.submit_stats)
            .map(|s| s.ac_submission_num)
            .unwrap_or_default();

        let totals = profile.all_questions_count.unwrap_or_default();

        fn find_count(items: &[DifficultyCount], diff: &str) -> i32 {
            items.iter().find(|d| d.difficulty == diff).map(|d| d.count).unwrap_or(0)
        }

        Ok(UserStats {
            username: username.to_string(),
            easy_solved: find_count(&solved, "Easy"),
            easy_total: find_count(&totals, "Easy"),
            medium_solved: find_count(&solved, "Medium"),
            medium_total: find_count(&totals, "Medium"),
            hard_solved: find_count(&solved, "Hard"),
            hard_total: find_count(&totals, "Hard"),
        })
    }

    pub async fn fetch_favorites(&self) -> Result<Vec<FavoriteList>> {
        let body = json!({
            "query": FAVORITES_LIST_QUERY,
            "variables": {}
        });

        let resp = self
            .auth_request(self.client.post(LEETCODE_GRAPHQL))
            .json(&body)
            .send()
            .await
            .context("Failed to fetch favorites")?;

        let data: GraphQLResponse<FavoritesListData> = resp
            .json()
            .await
            .context("Failed to parse favorites response")?;

        let lists = data
            .data
            .and_then(|d| d.favorites_lists)
            .map(|f| f.all_favorites)
            .unwrap_or_default();

        Ok(lists)
    }

    pub async fn create_favorite_list(&self, name: &str) -> Result<()> {
        let resp = self
            .auth_request(self.client.post(LEETCODE_LIST_API))
            .json(&json!({ "name": name }))
            .send()
            .await
            .context("Failed to create list")?;

        let status = resp.status();
        if !status.is_success() {
            bail!("Failed to create list: HTTP {status}");
        }
        Ok(())
    }

    pub async fn delete_favorite_list(&self, id_hash: &str) -> Result<()> {
        let url = format!("{}{}", LEETCODE_LIST_API, id_hash);
        let resp = self
            .auth_request(self.client.delete(&url))
            .send()
            .await
            .context("Failed to delete list")?;

        let status = resp.status();
        if !status.is_success() {
            bail!("Failed to delete list: HTTP {status}");
        }
        Ok(())
    }

    pub async fn add_to_favorite(&self, id_hash: &str, question_id: &str) -> Result<()> {
        let resp = self
            .auth_request(self.client.post(LEETCODE_LIST_QUESTIONS_API))
            .json(&json!({
                "favorite_id_hash": id_hash,
                "question_id": question_id,
            }))
            .send()
            .await
            .context("Failed to add to list")?;

        let status = resp.status();
        if !status.is_success() {
            bail!("Failed to add problem to list: HTTP {status}");
        }
        Ok(())
    }

    pub async fn remove_from_favorite(&self, id_hash: &str, question_id: &str) -> Result<()> {
        let url = format!("{}/{}/{}", LEETCODE_LIST_QUESTIONS_API, id_hash, question_id);
        let resp = self
            .auth_request(self.client.delete(&url))
            .send()
            .await
            .context("Failed to remove from list")?;

        let status = resp.status();
        if !status.is_success() {
            bail!("Failed to remove problem from list: HTTP {status}");
        }
        Ok(())
    }
}
