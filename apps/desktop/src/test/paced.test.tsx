// Spec: specs/013-output-pacing/spec.md

/**
 * AC-2: chunks render in `index` order and the caret disappears on `is_last`.
 *
 * The pacing itself is asserted in Rust (`crates/butler-core/tests/pacing.rs`),
 * which is the point of §1 putting the policy in core: there is no timing to
 * test here, because the component has none. What is testable here is that
 * the overlay renders what it was sent, in the order it was sent, and stops
 * claiming the answer is still arriving when it is not.
 */

import { render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it } from "vitest";

import { AnswerPanel } from "../components/AnswerPanel";
import { PacedAnswer } from "../components/PacedAnswer";
import type { UiEvent } from "../ipc/client";
import { apply, dismiss, resetForTest, state } from "../state/runtime";

import "../styles/base.css";

/** One `AnswerChunk`, spelled once. */
function chunk(
  index: number,
  text: string,
  is_last = false,
  request = 7,
): UiEvent {
  return { type: "answer-chunk", request, index, text, is_last };
}

/** The rendered text with the whitespace the DOM would collapse collapsed. */
function visible(container: HTMLElement): string {
  return (container.textContent ?? "").replace(/\s+/g, " ").trim();
}

describe("AC-2: the paced answer", () => {
  beforeEach(() => {
    resetForTest();
  });

  it("renders chunks in index order", () => {
    apply({ type: "answer-started", request: 7 });
    const { container } = render(() => <PacedAnswer />);

    apply(chunk(0, "Capture exclusion is"));
    apply(chunk(1, "a compositor feature,"));
    apply(chunk(2, "not encryption.", true));

    expect(visible(container)).toBe(
      "Capture exclusion is a compositor feature, not encryption.",
    );
    expect(container.querySelectorAll(".chunk")).toHaveLength(3);
  });

  it("buffers a chunk that arrives before the one in front of it", () => {
    apply({ type: "answer-started", request: 7 });
    const { container } = render(() => <PacedAnswer />);

    apply(chunk(0, "one"));
    apply(chunk(2, "three"));
    // Index 2 must not be rendered yet: the reader would see "one three".
    expect(visible(container)).toBe("one");
    expect(state.chunks).toEqual(["one"]);

    apply(chunk(1, "two"));
    // The gap is filled, so both waiting chunks land at once, still in order.
    expect(visible(container)).toBe("one two three");
    expect(state.chunks).toEqual(["one", "two", "three"]);
  });

  it("drops the caret when the last chunk lands", () => {
    apply({ type: "answer-started", request: 7 });
    const { container } = render(() => <PacedAnswer />);

    apply(chunk(0, "still"));
    expect(container.querySelector(".caret")).not.toBeNull();

    apply(chunk(1, "arriving"));
    expect(container.querySelector(".caret")).not.toBeNull();

    apply(chunk(2, "done.", true));
    expect(container.querySelector(".caret")).toBeNull();
  });

  it("keeps the caret through AnswerDone, which the reader has not caught up to", () => {
    apply({ type: "answer-started", request: 7 });
    const { container } = render(() => <PacedAnswer />);

    apply(chunk(0, "the provider stopped"));
    apply({ type: "answer-done", request: 7, stop: "end-turn" });

    // §3.2: the pacer keeps releasing after the stream ends, so `AnswerDone`
    // is not the end of rendering. A caret that vanished here would say the
    // answer was complete while words were still arriving.
    expect(state.phase).toBe("done");
    expect(container.querySelector(".caret")).not.toBeNull();

    apply(chunk(1, "and the pacer drained.", true));
    expect(container.querySelector(".caret")).toBeNull();
  });

  it("ignores a chunk belonging to a superseded request", () => {
    apply({ type: "answer-started", request: 7 });
    const { container } = render(() => <PacedAnswer />);

    apply(chunk(0, "the current answer"));
    apply(chunk(1, "from the old one", false, 6));

    expect(visible(container)).toBe("the current answer");
    expect(state.chunks).toEqual(["the current answer"]);
  });

  it("starts clean on the next answer", () => {
    apply({ type: "answer-started", request: 7 });
    const { container } = render(() => <PacedAnswer />);
    apply(chunk(0, "first answer.", true));
    expect(visible(container)).toBe("first answer.");

    apply({ type: "answer-started", request: 8 });
    expect(visible(container)).toBe("");
    expect(state.answerComplete).toBe(false);

    // The new answer restarts at index 0; the old indices are forgotten.
    apply(chunk(0, "second answer.", true, 8));
    expect(visible(container)).toBe("second answer.");
  });

  it("forgets a buffered chunk when the answer is dismissed", () => {
    apply({ type: "answer-started", request: 7 });
    apply(chunk(0, "one"));
    apply(chunk(2, "three"));

    dismiss();
    expect(state.chunks).toEqual([]);
    expect(state.answer).toBe("");

    // The stranded index 2 must not resurface against the next answer.
    apply({ type: "answer-started", request: 9 });
    apply(chunk(0, "fresh.", true, 9));
    expect(state.chunks).toEqual(["fresh."]);
  });

  it("shows the panel's placeholder until the first chunk arrives", () => {
    const { container } = render(() => <AnswerPanel />);
    expect(visible(container)).toBe("nothing to add");

    apply({ type: "answer-started", request: 7 });
    expect(visible(container)).toBe("Thinking");

    apply(chunk(0, "here it is.", true));
    expect(visible(container)).toBe("here it is.");
  });

  it("fades each chunk in under spec 012's 150 ms ceiling", () => {
    apply({ type: "answer-started", request: 7 });
    const { container } = render(() => <PacedAnswer />);
    apply(chunk(0, "faded"));

    const first = container.querySelector(".chunk");
    expect(first).not.toBeNull();
    const duration = getComputedStyle(first as Element).animationDuration;
    expect(duration).toBe("120ms");
  });
});
