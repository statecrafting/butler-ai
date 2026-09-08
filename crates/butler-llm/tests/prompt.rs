// Spec: specs/010-assistant-inference/spec.md

//! FR-002 and AC-3: the request body is byte-stable, and the prompt is a
//! fixture because **a prompt change is a spec change**.
//!
//! The golden files under `tests/fixtures/prompts/` are the review surface.
//! A diff there in a pull request is the signal that the assistant's
//! instructions changed, which is not something to discover from behaviour.

use butler_core::machine::RequestId;
use butler_core::redaction::{RedactionPolicy, redact};
use butler_core::settings::{AnswerStyle, Effort};
use butler_llm::{InferenceRequest, PromptConfig, build};

/// A fixed request, so the goldens are reproducible.
fn request(screen: &str, note: Option<&str>, prior: Option<&str>) -> InferenceRequest {
    InferenceRequest {
        request: RequestId(7),
        screen_text: redact(screen, &RedactionPolicy::default()),
        prior_answer: prior.map(ToOwned::to_owned),
        user_note: note.map(ToOwned::to_owned),
        effort: Effort::Medium,
        answer_style: AnswerStyle::Short,
        max_output_tokens: 1024,
    }
}

/// Compare against a golden, or write it when regenerating.
fn golden(name: &str, actual: &str) {
    let path = format!("tests/fixtures/prompts/{name}");
    if std::env::var_os("BUTLER_REGENERATE_PROMPTS").is_some() {
        std::fs::write(&path, actual).expect("write the golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("golden {name} missing ({e}). Regenerate with BUTLER_REGENERATE_PROMPTS=1")
    });
    assert_eq!(
        actual, expected,
        "the prompt changed. AC-3: a prompt change is a spec change, so update \
         specs/010-assistant-inference/spec.md in the same pull request, then \
         regenerate with BUTLER_REGENERATE_PROMPTS=1"
    );
}

/// FR-002. The body for a fixed request is byte-stable and carries no field
/// §3.2 does not name.
#[test]
fn fr_002_the_request_body_is_byte_stable() {
    let req = request(
        "fn main() {\n    println!(\"hello\")\n}\n\nerror: expected `;`, found `}`",
        Some("why?"),
        None,
    );
    let prompt = build(&req, PromptConfig::default());
    let body = butler_llm::anthropic::request_body("claude-opus-5", &req, &prompt);

    let rendered = serde_json::to_string_pretty(&body).expect("serialize");
    golden("request_body.json", &rendered);

    // §3.2 lists the fields. Anything else here would reach the provider
    // without a spec change, and three of them are 400s on current models.
    let object = body.as_object().expect("an object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "fallbacks",
            "max_tokens",
            "messages",
            "model",
            "output_config",
            "stream",
            "system"
        ]
    );

    // The three that are hard failures on current models.
    for banned in ["temperature", "top_p", "top_k", "thinking"] {
        assert!(
            !object.contains_key(banned),
            "`{banned}` must not be sent: it is a 400 on the default model"
        );
    }
}

/// §3.2: the server-side fallback is opt-in per model, and its scalar form
/// pairs with one specific beta header.
#[test]
fn fallbacks_are_sent_only_for_the_model_that_takes_them() {
    let req = request("some screen", None, None);
    let prompt = build(&req, PromptConfig::default());

    let default_model = butler_llm::anthropic::request_body("claude-opus-5", &req, &prompt);
    assert_eq!(
        default_model.get("fallbacks").and_then(|v| v.as_str()),
        Some("default")
    );

    let other = butler_llm::anthropic::request_body("claude-haiku-4-5", &req, &prompt);
    assert!(
        other.get("fallbacks").is_none(),
        "a model the spec does not name must not carry the parameter"
    );
}

/// AC-3. The assembled prompt is a golden.
#[test]
fn ac_003_the_prompt_is_a_reviewed_fixture() {
    let req = request(
        "Subject: quarterly numbers\n\nCan you summarise the attached figures?",
        None,
        None,
    );
    let prompt = build(&req, PromptConfig::default());
    golden("system.txt", &prompt.system);
    golden("user_plain.txt", &prompt.user);
}

#[test]
fn ac_003_a_note_and_a_prior_answer_are_placed_where_the_spec_says() {
    let req = request(
        "a screen with a question on it",
        Some("in French"),
        Some("An earlier answer."),
    );
    let prompt = build(&req, PromptConfig::default());

    golden("user_with_note.txt", &prompt.user);
    assert_eq!(prompt.prior_answer.as_deref(), Some("An earlier answer."));

    // §3.3: the note is the user speaking, the screen is not. It sits outside
    // the block, so text on the screen cannot masquerade as the user.
    let screen_end = prompt.user.find("</screen>").expect("a closed block");
    let note_at = prompt.user.find("in French").expect("the note");
    assert!(
        note_at > screen_end,
        "the note must sit outside the screen block"
    );
}

/// §3.3's trimming, and the reason it keeps the beginning: a screen's question
/// is far more often at the top than at the bottom of a long document.
#[test]
fn the_context_budget_trims_from_the_end() {
    let long = "A".repeat(20_000);
    let req = request(&long, None, None);
    let prompt = build(&req, PromptConfig::default());

    let inside = prompt
        .user
        .trim_start_matches("<screen>\n")
        .trim_end_matches("\n</screen>");
    assert_eq!(
        inside.chars().count(),
        PromptConfig::default().max_screen_chars
    );
    assert!(inside.starts_with("AAAA"));
}

/// The screen is always delimited, even when it is empty: the model must
/// never have to guess where untrusted content starts.
#[test]
fn the_screen_block_is_always_delimited() {
    let req = request("", None, None);
    let prompt = build(&req, PromptConfig::default());
    assert!(prompt.user.starts_with("<screen>\n"));
    assert!(prompt.user.ends_with("\n</screen>"));
}
