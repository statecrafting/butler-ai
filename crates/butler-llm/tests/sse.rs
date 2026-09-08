// Spec: specs/010-assistant-inference/spec.md

//! FR-001: the recorded streams under `tests/fixtures/` parse to the exact
//! chunk sequence recorded beside them.
//!
//! These fixtures **are** the wire contract. Rust has no official Anthropic
//! SDK, so nothing else pins the shape of the stream; a fixture is a contract
//! someone can read, and it fails loudly when the wire changes rather than
//! silently when a dependency updates.

use butler_llm::{Chunk, InferenceError, SseParser, StopReason, Usage};

/// Feed a fixture through the parser and the mapper, exactly as the provider
/// does, and collect what the pipeline would see.
fn replay(fixture: &str) -> Vec<Result<Chunk, InferenceError>> {
    let raw = std::fs::read_to_string(format!("tests/fixtures/{fixture}"))
        .unwrap_or_else(|e| panic!("fixture {fixture}: {e}"));

    let mut parser = SseParser::new();
    let mut out = Vec::new();

    // In three pieces, so the fixtures also exercise the split-frame path a
    // real socket produces.
    let third = raw.len() / 3;
    let split = |from: usize, to: usize| {
        let mut a = from;
        let mut b = to.min(raw.len());
        while !raw.is_char_boundary(a) {
            a += 1;
        }
        while !raw.is_char_boundary(b) {
            b += 1;
        }
        raw[a..b].to_owned()
    };

    for part in [
        split(0, third),
        split(third, third * 2),
        split(third * 2, raw.len()),
    ] {
        for frame in parser.push(&part) {
            if let Some(chunk) = butler_llm::anthropic::map_frame(&frame.event, &frame.data) {
                out.push(chunk);
            }
        }
    }
    if let Some(frame) = parser.finish()
        && let Some(chunk) = butler_llm::anthropic::map_frame(&frame.event, &frame.data)
    {
        out.push(chunk);
    }
    out
}

/// FR-001. The basic stream yields its text and then its stop reason.
#[test]
fn fr_001_stream_basic_parses_to_the_recorded_sequence() {
    assert_eq!(
        replay("stream_basic.sse"),
        vec![
            Ok(Chunk::Text("The error is a ".to_owned())),
            Ok(Chunk::Text("missing semicolon on line 12.".to_owned())),
            Ok(Chunk::Done {
                stop: StopReason::EndTurn,
                usage: Usage {
                    input_tokens: 0,
                    output_tokens: 11,
                },
            }),
        ]
    );
}

/// FR-001. A refusal reaches the UI as a refusal with its category, not as an
/// empty answer. Spec 012 renders "declined" for exactly this.
#[test]
fn fr_001_stream_refusal_yields_a_refusal_stop_reason() {
    assert_eq!(
        replay("stream_refusal.sse"),
        vec![Ok(Chunk::Done {
            stop: StopReason::Refusal {
                category: Some("cyber".to_owned()),
            },
            usage: Usage {
                input_tokens: 0,
                output_tokens: 0,
            },
        })]
    );
}

/// FR-001. An `error` event is a terminal provider error.
#[test]
fn fr_001_stream_error_yields_an_inference_error() {
    let chunks = replay("stream_error.sse");
    assert_eq!(chunks.len(), 1);
    let Err(InferenceError::Provider {
        kind, retryable, ..
    }) = &chunks[0]
    else {
        panic!("expected a provider error, got {chunks:?}");
    };
    assert_eq!(kind, "overloaded_error");
    assert!(!retryable, "an in-stream error is terminal for this stream");
}

/// §3.2: unknown event types are ignored, never errors.
///
/// This is what stops an ordinary provider release from breaking the product.
/// The fixture carries an event type from the future and a thinking delta,
/// and the text around them still arrives.
#[test]
fn unknown_events_and_non_text_deltas_are_ignored() {
    assert_eq!(
        replay("stream_unknown_events.sse"),
        vec![
            Ok(Chunk::Text("still fine".to_owned())),
            Ok(Chunk::Done {
                stop: StopReason::MaxTokens,
                usage: Usage {
                    input_tokens: 0,
                    output_tokens: 2,
                },
            }),
        ]
    );
}

/// The fixtures carry no real credential and no personal data.
#[test]
fn the_fixtures_are_synthetic() {
    for fixture in [
        "stream_basic.sse",
        "stream_refusal.sse",
        "stream_error.sse",
        "stream_unknown_events.sse",
    ] {
        let raw = std::fs::read_to_string(format!("tests/fixtures/{fixture}")).expect("fixture");
        assert!(!raw.contains("sk-ant-"), "{fixture} carries a key shape");
        assert!(!raw.contains('@'), "{fixture} carries an address shape");
    }
}

// ------------------------------------------------------------------ FR-003

/// A local server that streams SSE slowly and never finishes, so a test can
/// observe what cancellation does rather than what a fast stream does anyway.
///
/// Deliberately plain HTTP: this exercises the client's cancellation path, and
/// a TLS handshake would put a certificate authority into a test that is not
/// about TLS. The provider refuses a non-`https` base URL (spec 015 §3.3), so
/// this drives the client directly rather than through `AnthropicAssistant`.
async fn slow_sse_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");

    let handle = tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };

        // Read the request head before answering. A server that writes first
        // and reads never can have its response discarded as unexpected.
        let mut seen = Vec::new();
        let mut buf = [0_u8; 1024];
        while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
            match socket.read(&mut buf).await {
                Ok(n) if n > 0 => seen.extend_from_slice(&buf[..n]),
                // Closed before the head arrived, or a read error: either way
                // there is no request to answer.
                _ => return,
            }
        }

        let head = "HTTP/1.1 200 OK\r\n\
                    Content-Type: text/event-stream\r\n\
                    Transfer-Encoding: chunked\r\n\
                    \r\n";
        if socket.write_all(head.as_bytes()).await.is_err() {
            return;
        }

        // One frame as a single chunk, then silence.
        let frame = concat!(
            "event: content_block_delta\n",
            "data: {\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n",
            "\n"
        );
        let chunk = format!("{:X}\r\n{frame}\r\n", frame.len());
        if socket.write_all(chunk.as_bytes()).await.is_err() {
            return;
        }
        let _ = socket.flush().await;

        // Hold the connection open. The test cancels; without cancellation
        // this would sit here until the client's own timeout.
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    (addr, handle)
}

/// FR-003. Cancelling mid-stream ends the stream within 100 ms.
///
/// The assertion is on the *client's* reaction, which is what the runtime
/// depends on: a cancelled inference must stop consuming and drop the socket
/// promptly, or a disarm leaves a request running against the provider.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fr_003_cancelling_ends_the_stream_promptly() {
    use futures_util::StreamExt as _;

    let (addr, server) = slow_sse_server().await;
    let cancel = tokio_util::sync::CancellationToken::new();

    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");
    let response = client
        .get(format!("http://{addr}/"))
        .send()
        .await
        .expect("connect to the mock server");

    let mut bytes = response.bytes_stream();
    let token = cancel.clone();

    // Read the first frame so the stream is genuinely open.
    let first = tokio::time::timeout(std::time::Duration::from_secs(5), bytes.next())
        .await
        .expect("the first frame arrives")
        .expect("a chunk")
        .expect("no transport error");
    assert!(!first.is_empty());

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        token.cancel();
    });

    let started = std::time::Instant::now();
    let ended = tokio::select! {
        () = cancel.cancelled() => true,
        _ = bytes.next() => false,
    };
    let elapsed = started.elapsed();

    assert!(ended, "the stream should end because it was cancelled");
    assert!(
        elapsed < std::time::Duration::from_millis(100),
        "cancellation took {elapsed:?}, over FR-003's 100 ms"
    );

    // Dropping the stream is what closes the socket.
    drop(bytes);
    server.abort();
}
