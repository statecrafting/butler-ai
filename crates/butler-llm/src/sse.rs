// Spec: specs/010-assistant-inference/spec.md

//! A pure Server-Sent Events frame parser (spec 010 §3.2).
//!
//! Pure and byte-oriented so the wire contract can be pinned by fixture tests
//! rather than by a live API call: `tests/sse.rs` feeds it the recorded
//! streams under `tests/fixtures/` and asserts the exact chunk sequence
//! (FR-001). A parser that needed a network to be tested would be a parser
//! nobody could test.
//!
//! The grammar is small and the spec is
//! <https://html.spec.whatwg.org/multipage/server-sent-events.html>: lines of
//! `field: value`, a blank line ends an event, and a leading colon is a
//! comment. Anthropic sends `event:` and `data:` only.

/// One dispatched event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// The `event:` field, or an empty string when the stream omits it.
    pub event: String,
    /// The `data:` field, with multi-line values joined by newlines.
    pub data: String,
}

/// An incremental SSE parser.
///
/// Incremental because a stream arrives in arbitrary chunks: a frame can be
/// split across two reads, and a parser that assumed otherwise would work in
/// every test and fail against a real socket.
#[derive(Debug, Default)]
pub struct SseParser {
    buffer: String,
    event: String,
    data: String,
    has_data: bool,
}

impl SseParser {
    /// A parser with nothing buffered.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed bytes, and take whatever complete frames they finished.
    pub fn push(&mut self, chunk: &str) -> Vec<Frame> {
        self.buffer.push_str(chunk);
        let mut frames = Vec::new();

        // Only whole lines are consumed; a trailing partial line stays in the
        // buffer for the next chunk.
        while let Some(index) = self.buffer.find('\n') {
            let line = self.buffer[..index].trim_end_matches('\r').to_owned();
            self.buffer.drain(..=index);
            if let Some(frame) = self.line(&line) {
                frames.push(frame);
            }
        }
        frames
    }

    /// Finish, emitting a frame if one was left un-terminated.
    ///
    /// A server that closes without a final blank line has still told us
    /// something, and discarding it would lose the last chunk of an answer.
    pub fn finish(&mut self) -> Option<Frame> {
        let rest = std::mem::take(&mut self.buffer);
        let mut last = None;
        for line in rest.split('\n') {
            if let Some(frame) = self.line(line.trim_end_matches('\r')) {
                last = Some(frame);
            }
        }
        // A frame dispatched while draining wins; otherwise flush whatever
        // fields are still accumulated. The first version discarded the
        // former and then found the state already cleared, so it returned
        // nothing and the last chunk of an answer was lost (D-5).
        last.or_else(|| self.dispatch())
    }

    /// Process one complete line. Returns a frame when the line ended one.
    fn line(&mut self, line: &str) -> Option<Frame> {
        if line.is_empty() {
            return self.dispatch();
        }
        // A leading colon is a comment, which servers use as a keep-alive.
        if line.starts_with(':') {
            return None;
        }

        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            // A line with no colon is a field with an empty value.
            None => (line, ""),
        };

        match field {
            "event" => value.clone_into(&mut self.event),
            "data" => {
                if self.has_data {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.has_data = true;
            }
            // `id` and `retry` are part of SSE but Anthropic sends neither,
            // and an unknown field is ignored by the spec rather than an
            // error. Forward compatibility is the point (§3.2).
            _ => {}
        }
        None
    }

    /// Emit the accumulated frame, if there is one.
    fn dispatch(&mut self) -> Option<Frame> {
        if !self.has_data && self.event.is_empty() {
            return None;
        }
        let frame = Frame {
            event: std::mem::take(&mut self.event),
            data: std::mem::take(&mut self.data),
        };
        self.has_data = false;
        Some(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, SseParser};

    #[test]
    fn a_simple_frame_parses() {
        let mut parser = SseParser::new();
        let frames = parser.push("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
        assert_eq!(
            frames,
            vec![Frame {
                event: "message_stop".to_owned(),
                data: "{\"type\":\"message_stop\"}".to_owned(),
            }]
        );
    }

    /// The property a real socket needs: a frame split across reads.
    #[test]
    fn a_frame_split_across_chunks_parses() {
        let mut parser = SseParser::new();
        assert!(parser.push("event: content_bl").is_empty());
        assert!(parser.push("ock_delta\ndata: {\"a\":").is_empty());
        let frames = parser.push("1}\n\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, "content_block_delta");
        assert_eq!(frames[0].data, "{\"a\":1}");
    }

    #[test]
    fn multi_line_data_is_joined_with_newlines() {
        let mut parser = SseParser::new();
        let frames = parser.push("event: e\ndata: one\ndata: two\n\n");
        assert_eq!(frames[0].data, "one\ntwo");
    }

    #[test]
    fn comments_and_crlf_are_handled() {
        let mut parser = SseParser::new();
        let frames = parser.push(": keep-alive\r\nevent: ping\r\ndata: {}\r\n\r\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, "ping");
    }

    #[test]
    fn several_frames_in_one_chunk_all_arrive() {
        let mut parser = SseParser::new();
        let frames = parser.push("event: a\ndata: 1\n\nevent: b\ndata: 2\n\n");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].event, "b");
    }

    /// A server that closes without a final blank line has still told us
    /// something; discarding it would lose the last chunk of an answer.
    #[test]
    fn an_unterminated_frame_is_emitted_on_finish() {
        let mut parser = SseParser::new();
        assert!(parser.push("event: last\ndata: 9\n").is_empty());
        assert_eq!(
            parser.finish(),
            Some(Frame {
                event: "last".to_owned(),
                data: "9".to_owned(),
            })
        );
        assert_eq!(parser.finish(), None, "finish is idempotent");
    }

    #[test]
    fn a_blank_line_with_nothing_buffered_emits_nothing() {
        let mut parser = SseParser::new();
        assert!(parser.push("\n\n\n").is_empty());
    }
}
