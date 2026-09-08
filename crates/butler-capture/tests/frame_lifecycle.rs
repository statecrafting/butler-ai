// Spec: specs/006-screen-capture/spec.md

//! The lifecycle rules that make `Frame` the first link in the privacy chain
//! (spec 006 §3.2, spec 015).
//!
//! These run on **every** target, Linux included, because they are about the
//! type rather than about a screen (§2, AC-1).
//!
//! # FR-004's allocator hook
//!
//! "Dropping a `Frame` zeroes its buffer" cannot be asserted by reading the
//! memory after the drop: that is a use-after-free, and a test that does it
//! is testing the allocator's reuse policy rather than our `Drop`. The hook
//! below inspects the block **during** deallocation, while it is still
//! validly ours, which is the only point at which the question has an answer.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use butler_capture::{CropError, Frame, MonitorId, Rect};

// ------------------------------------------------------- FR-004: the hook

/// The size of block to watch. Set to the frame's buffer size before the drop.
static WATCH_SIZE: AtomicUsize = AtomicUsize::new(0);
/// Set when a watched block was freed while still holding a non-zero byte.
static SAW_NONZERO: AtomicBool = AtomicBool::new(false);
/// Set when a watched block was freed at all, so the test cannot pass by the
/// hook never firing.
static SAW_WATCHED: AtomicBool = AtomicBool::new(false);

/// A pass-through allocator that inspects one size of block on free.
struct Watcher;

// SAFETY: every method forwards to `System`, which is a valid allocator, with
// the same arguments and no aliasing of its own. The added read in `dealloc`
// happens before the block is handed back, while the pointer is still valid
// and owned by the caller, and only reads.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Watcher {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let watch = WATCH_SIZE.load(Ordering::Relaxed);
        if watch != 0 && layout.size() == watch {
            SAW_WATCHED.store(true, Ordering::Relaxed);
            let block = unsafe { std::slice::from_raw_parts(ptr, layout.size()) };
            if block.iter().any(|byte| *byte != 0) {
                SAW_NONZERO.store(true, Ordering::Relaxed);
            }
        }
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Watcher = Watcher;

// ------------------------------------------------------------------ helpers

/// A frame full of a recognizable non-zero pattern.
///
/// Built through the crate's own test constructor, because `Frame::new` is
/// `pub(crate)`: nothing outside this crate can make pixels into a `Frame`,
/// which is itself part of §3.2.
fn frame(width: u32, height: u32) -> Frame {
    butler_capture::test_support::frame_from_pattern(MonitorId(1), width, height, 2.0, 0xAB)
}

// ------------------------------------------------------------------- tests

/// FR-004. The buffer is zero at the moment it is freed.
#[test]
fn fr_004_dropping_a_frame_zeroes_its_buffer() {
    // A size no other allocation in this test binary is likely to share, so
    // the hook watches this frame and nothing else.
    let (width, height) = (37, 41);
    let bytes = (width as usize) * (height as usize) * 4;

    let f = frame(width, height);
    assert!(
        f.as_rgba().iter().any(|b| *b != 0),
        "the fixture must start non-zero, or this test proves nothing"
    );

    SAW_NONZERO.store(false, Ordering::Relaxed);
    SAW_WATCHED.store(false, Ordering::Relaxed);
    WATCH_SIZE.store(bytes, Ordering::Relaxed);

    drop(f);

    WATCH_SIZE.store(0, Ordering::Relaxed);

    assert!(
        SAW_WATCHED.load(Ordering::Relaxed),
        "the hook never saw the frame's buffer freed, so it proved nothing"
    );
    assert!(
        !SAW_NONZERO.load(Ordering::Relaxed),
        "the frame's buffer still held screen content when it was freed"
    );
}

/// A frame that is *not* dropped keeps its pixels: the zeroing is a drop
/// effect, not something that happens to the buffer at construction.
#[test]
fn a_live_frame_keeps_its_pixels() {
    let f = frame(8, 8);
    assert!(f.as_rgba().iter().all(|b| *b == 0xAB));
    assert_eq!(f.len(), 8 * 8 * 4);
    assert!(!f.is_empty());
}

/// §3.2: `Debug` prints dimensions and never content (spec 015 §3.5).
#[test]
fn debug_output_carries_no_pixels() {
    let f = frame(4, 4);
    let rendered = format!("{f:?}");

    assert!(rendered.contains("width"), "{rendered}");
    assert!(rendered.contains("bytes"), "{rendered}");
    assert!(
        !rendered.contains("171") && !rendered.to_lowercase().contains("ab, ab"),
        "a pixel value reached Debug output: {rendered}"
    );
}

/// §3.2: a view borrows a rectangle and cannot leave the frame.
#[test]
fn a_crop_is_bounded_by_its_frame() {
    let f = frame(10, 10);

    let view = f
        .crop(Rect {
            x: 2,
            y: 3,
            width: 4,
            height: 5,
        })
        .expect("an in-bounds crop");
    assert_eq!(view.rows().count(), 5);
    for row in view.rows() {
        assert_eq!(row.len(), 4 * 4, "each row is width * 4 bytes of RGBA8");
    }

    // `FrameView` is not `PartialEq` (it borrows a screen; comparing two of
    // them is not a thing this crate should make easy), so the error is
    // matched rather than compared.
    assert_eq!(
        f.crop(Rect {
            x: 8,
            y: 0,
            width: 4,
            height: 1
        })
        .unwrap_err(),
        CropError::OutOfBounds
    );
    assert_eq!(
        f.crop(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 1
        })
        .unwrap_err(),
        CropError::Empty
    );
}

/// §3.2: `captured_at` is monotonic, so a frame cannot say *when* in
/// wall-clock terms the user's screen looked like this.
#[test]
fn captured_at_is_monotonic_and_not_a_wall_clock() {
    let first = frame(2, 2);
    let second = frame(2, 2);
    assert!(second.captured_at >= first.captured_at);

    // `Instant` has no conversion to a date. Asserting the type is the
    // assertion: a `SystemTime` here would compile and would be the defect.
    let _: std::time::Instant = first.captured_at;
}
