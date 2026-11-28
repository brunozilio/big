use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::Value;

use super::fixture::Answer;
use crate::error::ActionError;
use crate::http::{Http, HttpRequest, HttpResponse};

#[derive(Debug, Default)]
pub struct Mock {
    answers: Mutex<Vec<Answer>>,
    sent: Mutex<Vec<String>>,
    unanswered: Mutex<Vec<String>>,
}

fn body_of(body: &Value) -> Vec<u8> {
    match body {
        Value::Null => Vec::new(),
        Value::String(text) => text.clone().into_bytes(),
        other => other.to_string().into_bytes(),
    }
}

fn response_of(answer: &Answer) -> HttpResponse {
    let mut response = HttpResponse::new(answer.status, body_of(&answer.body));
    for (name, value) in &answer.headers {
        let value = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
        response = response.with_header(name, &value);
    }
    response
}

impl Mock {
    pub fn of(answers: &[Answer]) -> Mock {
        Mock { answers: Mutex::new(answers.to_vec()), ..Mock::default() }
    }

    pub fn failing(answers: &[Answer], status: u16) -> Mock {
        let failed = answers
            .iter()
            .map(|answer| Answer { status, body: Value::String("{}".into()), repeat: true, ..answer.clone() })
            .collect::<Vec<_>>();
        Mock::of(&failed)
    }

    pub fn sent(&self) -> Vec<String> {
        self.sent.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn unanswered(&self) -> Vec<String> {
        self.unanswered.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

#[async_trait]
impl Http for Mock {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ActionError> {
        let shown = format!("{} {}", request.method.as_str(), request.url);
        self.sent.lock().unwrap_or_else(PoisonError::into_inner).push(shown.clone());
        let mut answers = self.answers.lock().unwrap_or_else(PoisonError::into_inner);
        let found = answers.iter().position(|answer| {
            answer.method.eq_ignore_ascii_case(request.method.as_str()) && request.url.contains(&answer.url)
        });
        let Some(found) = found else {
            self.unanswered.lock().unwrap_or_else(PoisonError::into_inner).push(shown.clone());
            return Err(ActionError::permanent(format!("the conformance harness has no answer for {shown}")));
        };
        let response = response_of(&answers[found]);
        if !answers[found].repeat {
            answers.remove(found);
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::http::Method;

    fn answer(url: &str, repeat: bool) -> Answer {
        serde_json::from_value(json!({ "method": "get", "url": url, "body": { "a": 1 }, "repeat": repeat }))
            .unwrap_or_else(|error| panic!("{error}"))
    }

    #[tokio::test]
    async fn answers_once_in_order_and_records_what_it_could_not_answer() {
        let mock = Mock::of(&[answer("/a", false)]);
        let first = mock.send(HttpRequest::new(Method::Get, "https://h/a")).await.map(|r| r.text());
        assert_eq!(first, Ok("{\"a\":1}".to_owned()));
        assert!(mock.send(HttpRequest::new(Method::Get, "https://h/a")).await.is_err());
        assert_eq!(mock.unanswered(), ["GET https://h/a"]);
        assert_eq!(mock.sent().len(), 2);
    }

    #[tokio::test]
    async fn a_failing_mock_answers_every_route_with_the_status() {
        let mock = Mock::failing(&[answer("/a", false)], 503);
        for _ in 0..2 {
            let status = mock.send(HttpRequest::new(Method::Get, "https://h/a")).await.map(|r| r.status);
            assert_eq!(status, Ok(503));
        }
    }
}
