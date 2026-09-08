// Spec: specs/015-privacy-boundary/spec.md

//! FR-005: exactly one distinct destination host, and a proxy in the
//! environment is not it.
//!
//! §3.3 says the endpoint set is explicit. The risk it names is not a rogue
//! URL in the code, which review would catch; it is an endpoint **nobody
//! chose**: `HTTPS_PROXY` in the environment silently reroutes every request
//! through a host the user never configured and the UI never surfaced. On a
//! product whose whole promise is that the screen goes to one place, that is
//! the failure that matters.
//!
//! # How this is measured without a network
//!
//! Two loopback listeners, and counting. One stands in for the provider; the
//! spec's "denying egress proxy" is the other, which accepts a connection and
//! immediately drops it. The client is pointed at the first and the
//! environment is pointed at the second, and the test asserts which one was
//! contacted.
//!
//! The TLS handshake against the provider listener fails, and that is fine:
//! the question is which host the client opened a socket to, and it has
//! answered that by the time the handshake begins. Nothing here reaches the
//! internet, so the test is the same on a developer's laptop and on a CI
//! runner with no egress.
//!
//! # Why a child process
//!
//! `reqwest` reads the proxy environment when the client is built, so the
//! test has to set it before that happens. `std::env::set_var` is `unsafe` in
//! edition 2024 and spec 001 forbids `unsafe` in this crate outright, and a
//! process-wide mutation would race every other test in the binary anyway.
//! So the parent sets the environment on a **child** invocation of this same
//! test binary, which is safe, and the child runs the one ignored test below.

use std::io::Read as _;
use std::net::{SocketAddr, TcpListener};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use butler_core::machine::RequestId;
use butler_core::redaction::{RedactionPolicy, redact};
use butler_core::settings::{AnswerStyle, Effort};
use butler_llm::anthropic::AnthropicAssistant;
use butler_llm::assistant::{Assistant as _, InferenceRequest};
use butler_llm::secrets::Secret;

/// The child reads its provider endpoint from here; its presence is also what
/// tells the ignored test below that it is the child.
const BASE_URL: &str = "BUTLER_TEST_BASE_URL";

/// Accept connections forever, count them, and drop each one.
///
/// Dropping is the "denying" half: a real denying proxy refuses to forward,
/// and for this test's purpose refusing and forwarding are the same, because
/// the count has already gone up.
fn count_connections(listener: TcpListener) -> Arc<AtomicU32> {
    let count = Arc::new(AtomicU32::new(0));
    let seen = Arc::clone(&count);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            seen.fetch_add(1, Ordering::SeqCst);
            // Read once so the peer's write does not fail before it has
            // finished connecting, then drop.
            let mut scratch = [0_u8; 64];
            let _ = stream.read(&mut scratch);
        }
    });
    count
}

fn bind() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    (listener, addr)
}

/// FR-005. One destination, and the environment does not get a vote.
#[test]
fn fr_005_exactly_one_destination_host_and_the_proxy_is_not_it() {
    let (provider_listener, provider) = bind();
    let (proxy_listener, proxy) = bind();
    let provider_hits = count_connections(provider_listener);
    let proxy_hits = count_connections(proxy_listener);

    let proxy_url = format!("http://{proxy}");
    let status = Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "the_child_makes_one_request", "--ignored"])
        .env(BASE_URL, format!("https://{provider}"))
        // Every spelling `reqwest` honours. Missing one would make this test
        // pass because the variable was never read, not because it was
        // ignored on purpose.
        .env("HTTPS_PROXY", &proxy_url)
        .env("https_proxy", &proxy_url)
        .env("HTTP_PROXY", &proxy_url)
        .env("http_proxy", &proxy_url)
        .env("ALL_PROXY", &proxy_url)
        .env("all_proxy", &proxy_url)
        .status()
        .expect("spawn the child test");
    assert!(status.success(), "the child test failed: {status}");

    let reached_provider = provider_hits.load(Ordering::SeqCst);
    let reached_proxy = proxy_hits.load(Ordering::SeqCst);

    // Non-vacuity first. If the client contacted nothing at all, "it did not
    // contact the proxy" would be true for the wrong reason, which is exactly
    // the shape of a test that has stopped testing. Dropping `.no_proxy()`
    // from the client fails here rather than below, because the request went
    // somewhere else entirely, so the message has to say where.
    assert!(
        reached_provider >= 1,
        "the client opened no socket to the configured endpoint \
         (the proxy saw {reached_proxy}), so the assertion below would prove \
         nothing"
    );
    assert_eq!(
        reached_proxy, 0,
        "the client honoured a proxy from the environment: that is a second \
         destination the user never configured and the UI never surfaced \
         (spec 015 §3.3)"
    );
}

/// §3.3: the endpoint must be HTTPS. A proxy variable pointing at plain HTTP
/// is one way a destination becomes plaintext; a `base_url` is the other, and
/// the provider refuses it at construction rather than at request time.
#[test]
fn a_plaintext_endpoint_is_refused_before_any_request() {
    let refused = AnthropicAssistant::new(
        Secret::new("sk-ant-not-a-real-key".to_owned()),
        "claude-opus-5".to_owned(),
        Some("http://127.0.0.1:9".to_owned()),
    );
    assert!(refused.is_err(), "a plaintext endpoint was accepted");

    let accepted = AnthropicAssistant::new(
        Secret::new("sk-ant-not-a-real-key".to_owned()),
        "claude-opus-5".to_owned(),
        Some("https://127.0.0.1:9".to_owned()),
    );
    assert!(accepted.is_ok(), "an HTTPS endpoint was refused");
}

/// The child half of FR-005. Ignored, so an ordinary `cargo test` run does
/// not execute it; the parent above runs it by name with the environment set.
#[test]
#[ignore = "run by fr_005_exactly_one_destination_host_and_the_proxy_is_not_it"]
fn the_child_makes_one_request() {
    let Ok(base_url) = std::env::var(BASE_URL) else {
        panic!("{BASE_URL} is unset: this test is only meaningful as a child");
    };

    let assistant = AnthropicAssistant::new(
        Secret::new("sk-ant-not-a-real-key".to_owned()),
        "claude-opus-5".to_owned(),
        Some(base_url),
    )
    .expect("build the provider");

    let request = InferenceRequest {
        request: RequestId(1),
        screen_text: redact("nothing sensitive", &RedactionPolicy::default()),
        prior_answer: None,
        user_note: None,
        effort: Effort::Medium,
        answer_style: AnswerStyle::Short,
        max_output_tokens: 64,
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async {
        use futures_util::StreamExt as _;
        let cancel = tokio_util::sync::CancellationToken::new();
        let mut stream = assistant.stream(request, cancel);
        // It will fail: the listener speaks no TLS. The socket the client
        // opened to get there is the whole measurement.
        while stream.next().await.is_some() {}
    });
}
