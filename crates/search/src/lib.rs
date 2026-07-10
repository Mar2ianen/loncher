#![forbid(unsafe_code)]

//! Application search backed by the high-level `nucleo` matcher.

use std::sync::{Arc, Mutex};

use loncher_applications::ApplicationEntry;
use nucleo::{
    Config, Nucleo, Utf32String,
    pattern::{CaseMatching, Normalization},
};
use thiserror::Error;

pub const DEFAULT_RESULT_LIMIT: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub application: ApplicationEntry,
    pub score: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub generation: u64,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResponse {
    pub generation: u64,
    pub query: String,
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("search worker did not produce a snapshot")]
    NoSnapshot,
    #[error("search matcher is unavailable")]
    MatcherUnavailable,
}

#[derive(Clone)]
struct Candidate {
    application: ApplicationEntry,
    fields: Vec<String>,
}

#[derive(Clone)]
pub struct SearchService {
    applications: Arc<Vec<ApplicationEntry>>,
    result_limit: usize,
    matcher: Arc<Mutex<Nucleo<Candidate>>>,
}

impl SearchService {
    pub fn new(applications: Vec<ApplicationEntry>, result_limit: usize) -> Self {
        let mut applications = applications;
        applications.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then(left.desktop_id.cmp(&right.desktop_id))
        });
        let applications = Arc::new(applications);
        let matcher: Nucleo<Candidate> = Nucleo::new(Config::DEFAULT, Arc::new(|| {}), Some(1), 1);
        let injector = matcher.injector();
        for application in applications.iter().cloned() {
            let fields = vec![
                application.name.clone(),
                application.generic_name.clone().unwrap_or_default(),
                application.keywords.join(" "),
                application.desktop_id.clone(),
            ];
            injector.push(Candidate { application, fields }, |candidate, columns| {
                columns[0] = Utf32String::from(candidate.fields.join(" "));
            });
        }
        drop(injector);
        Self {
            applications,
            result_limit: result_limit.max(1),
            matcher: Arc::new(Mutex::new(matcher)),
        }
    }

    pub fn applications(&self) -> &[ApplicationEntry] {
        self.applications.as_ref()
    }

    pub fn search(&self, request: SearchRequest) -> Result<SearchResponse, SearchError> {
        if request.query.trim().is_empty() {
            let results = self
                .applications
                .iter()
                .take(self.result_limit)
                .cloned()
                .map(|application| SearchResult { application, score: None })
                .collect();
            return Ok(SearchResponse {
                generation: request.generation,
                query: request.query,
                results,
            });
        }

        let mut matcher = self.matcher.lock().map_err(|_| SearchError::MatcherUnavailable)?;
        matcher.pattern.reparse(
            0,
            &request.query,
            CaseMatching::Smart,
            Normalization::Smart,
            false,
        );
        while matcher.tick(10).running {}
        let snapshot = matcher.snapshot();
        let results = snapshot
            .matched_items(..)
            .take(self.result_limit)
            .map(|item| SearchResult { application: item.data.application.clone(), score: None })
            .collect();
        Ok(SearchResponse { generation: request.generation, query: request.query, results })
    }

    pub async fn search_async(
        &self,
        request: SearchRequest,
    ) -> Result<SearchResponse, SearchError> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || service.search(request))
            .await
            .map_err(|_| SearchError::NoSnapshot)?
    }

    pub fn accept_if_current(
        current_generation: u64,
        response: SearchResponse,
    ) -> Option<SearchResponse> {
        (response.generation == current_generation).then_some(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn app(
        id: &str,
        name: &str,
        generic_name: Option<&str>,
        keywords: &[&str],
    ) -> ApplicationEntry {
        ApplicationEntry {
            desktop_id: id.into(),
            name: name.into(),
            generic_name: generic_name.map(str::to_owned),
            keywords: keywords.iter().map(|value| (*value).into()).collect(),
            icon: None,
            desktop_path: PathBuf::from(format!("/tmp/{id}.desktop")),
            working_directory: None,
            exec: vec!["true".into()],
            actions: Vec::new(),
            terminal: false,
            dbus_activatable: false,
        }
    }

    #[test]
    fn cyrillic_and_latin_queries_match_indexed_fields() {
        let service = SearchService::new(
            vec![
                app("zed.desktop", "Zed", Some("Editor"), &["code"]),
                app("org.demo.desktop", "Редактор", Some("Editor"), &["код"]),
            ],
            12,
        );
        assert_eq!(
            service.search(SearchRequest { generation: 1, query: "zed".into() }).unwrap().results
                [0]
            .application
            .desktop_id,
            "zed.desktop"
        );
        assert_eq!(
            service.search(SearchRequest { generation: 2, query: "ред".into() }).unwrap().results
                [0]
            .application
            .desktop_id,
            "org.demo.desktop"
        );
    }

    #[test]
    fn empty_query_is_alphabetical_and_bounded() {
        let service = SearchService::new(
            vec![
                app("z.desktop", "Zed", None, &[]),
                app("a.desktop", "Alpha", None, &[]),
                app("m.desktop", "Middle", None, &[]),
            ],
            2,
        );
        let response =
            service.search(SearchRequest { generation: 7, query: String::new() }).unwrap();
        assert_eq!(
            response
                .results
                .iter()
                .map(|result| result.application.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "Middle"]
        );
    }

    #[test]
    fn stale_response_is_rejected() {
        let service = SearchService::new(vec![app("demo.desktop", "Demo", None, &[])], 12);
        let response =
            service.search(SearchRequest { generation: 3, query: "demo".into() }).unwrap();
        assert!(SearchService::accept_if_current(4, response).is_none());
    }
}
