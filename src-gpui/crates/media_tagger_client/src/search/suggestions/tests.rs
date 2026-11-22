//! `SuggestionClient.test.ts`, with the worker replaced by the effects of the model.

use std::sync::Arc;

use super::{
    EditorId, SuggestionClient, SuggestionContext, SuggestionEffect, SuggestionEvent,
    SuggestionFailure, SuggestionInput, SuggestionRequest,
};
use crate::{
    model::Model,
    search::{active_token::read_active_token, matcher::TagSuggestion},
};

fn request(text: &str, retry: u64) -> SuggestionRequest {
    SuggestionRequest {
        context: SuggestionContext {
            token: read_active_token(text, text.len()),
            ..SuggestionContext::default()
        },
        retry,
    }
}

fn vocabulary(tags: &[&str]) -> Arc<Vec<String>> {
    Arc::new(tags.iter().map(|tag| (*tag).to_owned()).collect())
}

fn search(
    client: &mut SuggestionClient,
    editor: EditorId,
    tags: &Arc<Vec<String>>,
    text: &str,
) -> Vec<SuggestionEffect> {
    client.handle(SuggestionInput::Search {
        editor,
        vocabulary: Arc::clone(tags),
        request: request(text, 0),
    })
}

fn job(effects: &[SuggestionEffect]) -> u64 {
    match effects {
        [SuggestionEffect::Search { job, .. }] => *job,
        other => panic!("expected one search, got {other:?}"),
    }
}

fn answer(client: &mut SuggestionClient, job: u64, value: &str) -> Vec<SuggestionEffect> {
    client.handle(SuggestionInput::Answered {
        job,
        result: Ok(Ok(vec![TagSuggestion {
            value: value.into(),
            ranges: Vec::new(),
        }])),
    })
}

fn settled(
    client: &mut SuggestionClient,
) -> Vec<(EditorId, Result<Vec<String>, SuggestionFailure>)> {
    client
        .take_events()
        .into_iter()
        .map(|SuggestionEvent::Settled { editor, result, .. }| {
            (
                editor,
                result.map(|items| items.iter().map(|item| item.value.clone()).collect()),
            )
        })
        .collect()
}

#[test]
fn one_search_runs_and_queued_requests_coalesce_per_editor() {
    let mut client = SuggestionClient::new();
    let (one, two) = (EditorId::next(), EditorId::next());
    let tags = vocabulary(&["cat", "car"]);
    let first = job(&search(&mut client, one, &tags, "ca"));
    assert!(search(&mut client, two, &tags, "c").is_empty());
    assert!(search(&mut client, two, &tags, "car").is_empty());
    assert_eq!(
        client.queued(),
        1,
        "the newer request of two replaced its older one"
    );

    let next = job(&answer(&mut client, first, "cat"));
    assert_eq!(settled(&mut client), [(one, Ok(vec!["cat".to_owned()]))]);
    assert!(answer(&mut client, next, "car").is_empty());
    assert_eq!(settled(&mut client), [(two, Ok(vec!["car".to_owned()]))]);
    assert_eq!(client.indexed_versions(), 1, "one vocabulary, one index");
}

#[test]
fn a_newer_request_stops_the_running_one_and_drops_its_answer() {
    let mut client = SuggestionClient::new();
    let editor = EditorId::next();
    let tags = vocabulary(&["cat"]);
    let effects = search(&mut client, editor, &tags, "c");
    let SuggestionEffect::Search {
        job: first, cancel, ..
    } = &effects[0];
    assert!(search(&mut client, editor, &tags, "ca").is_empty());
    assert!(cancel.is_cancelled());
    let second = job(&answer(&mut client, *first, "obsolete"));
    assert!(settled(&mut client).is_empty());
    answer(&mut client, second, "cat");
    assert_eq!(settled(&mut client), [(editor, Ok(vec!["cat".to_owned()]))]);
}

#[test]
fn old_vocabulary_requests_are_dropped_and_a_failure_lasts_until_retry() {
    let mut client = SuggestionClient::new();
    let editor = EditorId::next();
    let other = EditorId::next();
    let old = vocabulary(&["old"]);
    let new = vocabulary(&["cat"]);
    let first = job(&search(&mut client, editor, &old, "o"));
    assert!(search(&mut client, other, &new, "c").is_empty());
    assert_eq!(client.version(), 2);
    let second = job(&answer(&mut client, first, "old"));
    assert!(
        settled(&mut client).is_empty(),
        "an answer for an old vocabulary is dropped"
    );

    assert!(client
        .handle(SuggestionInput::Answered {
            job: second,
            result: Err(SuggestionFailure),
        })
        .is_empty());
    assert_eq!(settled(&mut client), [(other, Err(SuggestionFailure))]);
    assert!(search(&mut client, other, &new, "ca").is_empty());
    assert_eq!(settled(&mut client), [(other, Err(SuggestionFailure))]);

    client.handle(SuggestionInput::Retry);
    let retried = job(&search(&mut client, other, &new, "ca"));
    answer(&mut client, retried, "cat");
    assert_eq!(settled(&mut client), [(other, Ok(vec!["cat".to_owned()]))]);
}

#[test]
fn disposal_settles_nothing_and_takes_no_new_request() {
    let mut client = SuggestionClient::new();
    let (one, two) = (EditorId::next(), EditorId::next());
    let tags = vocabulary(&["cat"]);
    let first = job(&search(&mut client, one, &tags, "c"));
    search(&mut client, two, &tags, "c");
    client.handle(SuggestionInput::Dispose);
    assert!(answer(&mut client, first, "late").is_empty());
    assert!(search(&mut client, one, &tags, "ca").is_empty());
    assert!(settled(&mut client).is_empty());
    assert!(!client.is_searching());
}

#[test]
fn the_same_vocabulary_is_not_indexed_again() {
    let mut client = SuggestionClient::new();
    let editor = EditorId::next();
    let tags = vocabulary(&["cat", "dog"]);
    for _ in 0..20 {
        let id = job(&search(&mut client, editor, &tags, "ca"));
        answer(&mut client, id, "cat");
    }
    assert_eq!(client.indexed_versions(), 1);
    let id = job(&search(
        &mut client,
        editor,
        &vocabulary(&["cat", "new"]),
        "ca",
    ));
    answer(&mut client, id, "cat");
    assert_eq!(client.indexed_versions(), 2);
}

#[test]
fn events_are_taken_per_editor() {
    let mut client = SuggestionClient::new();
    let (one, two) = (EditorId::next(), EditorId::next());
    let tags = vocabulary(&["cat"]);
    let first = job(&search(&mut client, one, &tags, "c"));
    search(&mut client, two, &tags, "c");
    let second = job(&answer(&mut client, first, "cat"));
    answer(&mut client, second, "cat");
    assert_eq!(client.take_events_for(two).len(), 1);
    assert_eq!(client.take_events_for(two).len(), 0);
    assert_eq!(client.take_events().len(), 1);
}
