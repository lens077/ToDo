//! Validation of agent-produced alignment verdicts.
//!
//! The prompt states rules; this is where they are actually enforced. Every case
//! below is a way a confident wrong answer could otherwise reach the user.

use todo_core::{parse, review_alignment, Verdict};

const DOC: &str = "---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n\n- [ ] 加限流 <!--t:aaaaaa-->\n- [ ] 写文档 <!--t:bbbbbb-->\n- [ ] 加缓存 <!--t:cccccc-->\n";

fn review(body: &str) -> todo_core::AlignmentReview {
    let document = parse(DOC).unwrap();
    review_alignment(&document, body).unwrap()
}

#[test]
fn a_done_verdict_without_a_citation_is_rejected() {
    // The single most dangerous output: a confident completion claim with nothing
    // behind it. It must never reach the confirmation screen.
    let review = review(
        r#"{"verdicts":[{"task_id":"aaaaaa","verdict":"done","confidence":"high","reason":"做完了","citations":[]}]}"#,
    );

    assert!(review.accepted.is_empty());
    assert_eq!(review.rejected.len(), 1);
    assert!(review.rejected[0].reason.contains("citation"));
}

#[test]
fn a_done_verdict_with_a_citation_is_accepted() {
    let review = review(
        r#"{"verdicts":[{"task_id":"aaaaaa","verdict":"done","confidence":"high","reason":"已实现","citations":["src/rate_limit.rs:12"]}]}"#,
    );

    assert_eq!(review.accepted.len(), 1);
    assert_eq!(review.accepted[0].verdict, Verdict::Done);
    assert!(review.rejected.is_empty());
}

#[test]
fn unknown_and_not_started_need_no_citation() {
    // `unknown` has to be cheap to produce, or the model is pushed into guessing.
    let review = review(
        r#"{"verdicts":[
            {"task_id":"aaaaaa","verdict":"unknown","reason":"看不出来"},
            {"task_id":"bbbbbb","verdict":"not_started","reason":"没找到"}
        ]}"#,
    );

    assert_eq!(review.accepted.len(), 2);
    assert!(review.rejected.is_empty());
}

#[test]
fn a_hallucinated_task_id_is_rejected() {
    let review =
        review(r#"{"verdicts":[{"task_id":"zzzzzz","verdict":"done","citations":["a.rs:1"]}]}"#);

    assert!(review.accepted.is_empty());
    assert_eq!(review.rejected.len(), 1);
    assert!(review.rejected[0].reason.contains("unknown task id"));
}

#[test]
fn a_malformed_citation_is_rejected() {
    let review = review(
        r#"{"verdicts":[{"task_id":"aaaaaa","verdict":"done","citations":["整个项目都实现了"]}]}"#,
    );

    assert!(review.accepted.is_empty());
    assert!(review.rejected[0].reason.contains("path:line"));
}

#[test]
fn duplicate_verdicts_for_one_task_are_rejected() {
    let review = review(
        r#"{"verdicts":[
            {"task_id":"aaaaaa","verdict":"done","citations":["a.rs:1"]},
            {"task_id":"aaaaaa","verdict":"not_started"}
        ]}"#,
    );

    assert_eq!(review.accepted.len(), 1);
    assert_eq!(review.rejected.len(), 1);
    assert!(review.rejected[0].reason.contains("duplicate"));
}

#[test]
fn tasks_the_agent_skipped_are_reported_as_missing() {
    // Coverage has to be provable. Silent omission is how a scan misses things.
    let review = review(r#"{"verdicts":[{"task_id":"aaaaaa","verdict":"unknown"}]}"#);

    assert_eq!(review.missing, vec!["bbbbbb", "cccccc"]);
}

#[test]
fn a_partial_percent_outside_one_to_ninety_nine_is_rejected() {
    let review = review(
        r#"{"verdicts":[{"task_id":"aaaaaa","verdict":"partial","percent":100,"citations":["a.rs:1"]}]}"#,
    );

    assert!(review.accepted.is_empty());
    assert!(review.rejected[0].reason.contains("1 and 99"));
}

#[test]
fn a_response_wrapped_in_a_fenced_block_is_still_parsed() {
    // Models routinely wrap JSON in ```json fences despite instructions.
    let review = review(
        "好的，这是结果：\n\n```json\n{\"verdicts\":[{\"task_id\":\"aaaaaa\",\"verdict\":\"unknown\"}]}\n```\n",
    );

    assert_eq!(review.accepted.len(), 1);
}

#[test]
fn actionable_puts_broken_evidence_before_completion_claims() {
    // Broken evidence means work believed done has been undone; it outranks a new
    // completion claim for a reviewer's attention.
    let review = review(
        r#"{"verdicts":[
            {"task_id":"aaaaaa","verdict":"done","citations":["a.rs:1"]},
            {"task_id":"bbbbbb","verdict":"evidence_broken","reason":"commit 已被 revert"},
            {"task_id":"cccccc","verdict":"unknown"}
        ]}"#,
    );

    let actionable = review.actionable();
    assert_eq!(actionable.len(), 2, "unknown is not actionable");
    assert_eq!(actionable[0].verdict, Verdict::EvidenceBroken);
    assert_eq!(actionable[1].verdict, Verdict::Done);
}

#[test]
fn a_response_that_is_not_json_fails_loudly() {
    let document = parse(DOC).unwrap();
    assert!(review_alignment(&document, "我看了一下，感觉都做完了").is_err());
}

#[test]
fn the_prompt_states_the_rules_the_validator_enforces() {
    // If the prompt and the validator drift apart, the model is set up to fail on
    // rules it was never told about.
    let prompt = todo_core::ALIGNMENT_PROMPT;
    assert!(prompt.contains("unknown"));
    assert!(prompt.contains("path:line"));
    assert!(prompt.contains("todo.alignment.v1"));
    for verdict in [
        "done",
        "partial",
        "not_started",
        "evidence_broken",
        "stale",
        "unknown",
    ] {
        assert!(prompt.contains(verdict), "prompt must document {verdict}");
    }
}
