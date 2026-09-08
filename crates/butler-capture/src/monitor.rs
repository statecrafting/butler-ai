// Spec: specs/006-screen-capture/spec.md

//! Monitor identity and enumeration (spec 006 §3.1, §3.4).

/// A monitor, identified stably for the process's lifetime.
///
/// Derived from the platform's own monitor handle, **not** from enumeration
/// order (§3.1). Order changes when a display is plugged in, and a user who
/// pinned "the second monitor" would silently start watching a different
/// screen, which is exactly the class of surprise this product cannot afford.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonitorId(pub u32);

/// A rectangle in desktop coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// What the source knows about one monitor (§3.1).
#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    /// Its stable id.
    pub id: MonitorId,
    /// The name the OS reports. Settings (014) pin a monitor by this.
    pub name: String,
    /// Where it sits on the desktop.
    pub bounds: Bounds,
    /// Its scale factor.
    pub scale: f32,
    /// Whether the OS calls it primary.
    pub is_primary: bool,
}
