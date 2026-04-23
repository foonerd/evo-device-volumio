//! MPD-domain types.
//!
//! Narrow, concrete types the MPD connection layer speaks in. These
//! are not Volumio-shaped; they are MPD-domain facts the warden will
//! later project into whatever the steward's contract requires.
//!
//! All types are `pub(crate)` because they are implementation detail
//! of the plugin; the admission surface in `lib.rs` does not expose
//! them.

use std::time::Duration;

/// MPD playback state, as reported by the `status` command's
/// `state:` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PlayState {
    /// Actively playing a song.
    Playing,
    /// Paused mid-song.
    Paused,
    /// Stopped (nothing playing; position not retained).
    Stopped,
}

/// MPD protocol version, parsed from the welcome banner
/// (`OK MPD <major>.<minor>.<patch>`).
///
/// Comparable and orderable so later phases can gate feature use on
/// minimum protocol versions (for example, `partition` support arrived
/// in 0.22, `readpicture` in 0.22, `albumart` in 0.21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MpdVersion {
    /// Major version number.
    pub(crate) major: u32,
    /// Minor version number.
    pub(crate) minor: u32,
    /// Patch version number.
    pub(crate) patch: u32,
}

impl MpdVersion {
    /// Construct a version with the three components.
    pub(crate) fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }
}

impl std::fmt::Display for MpdVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Narrow view of MPD's `status` response.
///
/// Only the fields the playback warden needs today. Additional fields
/// MPD reports (xfade, mixrampdb, audio, etc.) are intentionally
/// dropped rather than surfaced: the connection layer's surface grows
/// by explicit opt-in, not by accumulating every tag MPD emits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MpdStatus {
    /// Playback state (always present in MPD responses).
    pub(crate) state: PlayState,
    /// Zero-based position of the current song within the queue.
    /// `None` when the queue is empty or nothing is selected.
    pub(crate) song_position: Option<u32>,
    /// Elapsed time within the current song. `None` when the player
    /// is stopped, or when MPD does not report it (some sources omit
    /// elapsed on initial response; this is treated as unknown, not
    /// zero).
    pub(crate) elapsed: Option<Duration>,
    /// Total duration of the current song. `None` when MPD does not
    /// report it (streams, some CD rips).
    pub(crate) duration: Option<Duration>,
    /// Volume level, 0-100. `None` when MPD reports -1 (no mixer
    /// configured) or when the field is absent.
    pub(crate) volume: Option<u8>,
}

/// Narrow view of MPD's `currentsong` response.
///
/// Only the fields the playback warden needs today. A richer shape
/// (composer, date, track number, disc number, etc.) lives as a
/// future extension when Phase 3.4's subject assertion demands it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MpdSong {
    /// MPD-relative file path (e.g. `INTERNAL/Artist/Album/track.flac`).
    /// Always present when `currentsong` returns a non-empty response.
    pub(crate) file_path: String,
    /// Track title tag, if present.
    pub(crate) title: Option<String>,
    /// Artist tag, if present (prefers Artist over AlbumArtist; the
    /// warden's subject-assertion logic in Phase 3.4 may walk both).
    pub(crate) artist: Option<String>,
    /// Album tag, if present.
    pub(crate) album: Option<String>,
    /// Track duration from the `duration:` field (MPD 0.21+) or
    /// `Time:` (older).
    pub(crate) duration: Option<Duration>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_displays_dotted_triple() {
        let v = MpdVersion::new(0, 23, 5);
        assert_eq!(format!("{}", v), "0.23.5");
    }

    #[test]
    fn versions_order_by_component() {
        let a = MpdVersion::new(0, 22, 0);
        let b = MpdVersion::new(0, 23, 0);
        let c = MpdVersion::new(0, 23, 1);
        assert!(a < b);
        assert!(b < c);
        assert_eq!(b, MpdVersion::new(0, 23, 0));
    }
}
