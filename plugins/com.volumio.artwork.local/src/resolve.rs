//! `artwork.resolve` JSON payload, sidecar file discovery, and embedded tags.
//!
//! Request `target` uses the same `scheme` / `value` shape as
//! [`evo_plugin_sdk::contract::ExternalAddressing`], with schemes aligned to
//! `com.volumio.playback.mpd` (`mpd-path`, `mpd-album`).

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::Serialize;

use crate::embedded;

/// `mpd-path` scheme: value is MPD's `file` (library-relative or absolute).
pub(crate) const SCHEME_MPD_PATH: &str = "mpd-path";
/// `mpd-album` scheme: value is `Artist|Album` (see MPD warden).
pub(crate) const SCHEME_MPD_ALBUM: &str = "mpd-album";

const COVER_FILE_NAMES: &[&str] = &[
    "cover.jpg",
    "folder.jpg",
    "front.jpg",
    "cover.png",
    "folder.png",
    "front.png",
];

/// Request body for `artwork.resolve` (JSON, UTF-8).
#[derive(Debug, Deserialize)]
pub(crate) struct ArtworkResolveRequest {
    /// Schema version; `1` is the only value accepted.
    pub(crate) v: u8,
    /// Which subject to resolve art for; mirrors external addressing.
    pub(crate) target: ResolveTarget,
}

/// Subject selector: must match a registered scheme from the playback warden.
#[derive(Debug, Deserialize)]
pub(crate) struct ResolveTarget {
    pub(crate) scheme: String,
    pub(crate) value: String,
}

/// JSON response (always serialised; business outcomes use `status`, not HTTP).
#[derive(Debug, Serialize)]
pub(crate) struct ArtworkResolveResponse {
    v: u8,
    status: ResponseStatus,
    /// Absolute path to an image file on this device, when `status` is
    /// [`ResponseStatus::Ok`].
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    /// `image/jpeg`, `image/png`, etc., when `path` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    mime: Option<String>,
    /// Extra context for operators and UIs.
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

/// Outcome of a resolve attempt.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResponseStatus {
    Ok,
    NotFound,
    Unsupported,
    BadRequest,
}

impl ArtworkResolveResponse {
    pub(crate) fn json_bytes(self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self)
    }
}

/// Map a file extension to a MIME type for common cover art files.
fn mime_for_path(p: &Path) -> Option<&'static str> {
    p.extension()
        .and_then(|e| e.to_str())
        .and_then(|e| match e.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Some("image/jpeg"),
            "png" => Some("image/png"),
            "webp" => Some("image/webp"),
            _ => None,
        })
}

/// If `mpd_file` is a local audio file path, look for a well-known cover
/// image in the same directory. Returns the first match in
/// [`COVER_FILE_NAMES`] order.
pub(crate) fn find_cover_beside_audio_file(mpd_file: &Path) -> Option<PathBuf> {
    let dir = mpd_file.parent()?;
    for name in COVER_FILE_NAMES {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Resolve MPD `file` string to a local [`PathBuf`] if the file exists.
fn resolve_audio_path(library_roots: &[PathBuf], value: &str) -> Option<PathBuf> {
    if value
        .get(..7)
        .map(|p| p.eq_ignore_ascii_case("http://"))
        .unwrap_or(false)
        || value
            .get(..8)
            .map(|p| p.eq_ignore_ascii_case("https://"))
            .unwrap_or(false)
    {
        return None;
    }

    let p = Path::new(value);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    for root in library_roots {
        let joined = root.join(value);
        if joined.is_file() {
            return Some(joined);
        }
    }
    None
}

/// Build the JSON response body. Returns [`Err`] only for internal failures
/// (non-UTF-8 path, cache I/O) that should map to [`PluginError::Permanent`].
pub(crate) fn resolve_artwork(
    library_roots: &[PathBuf],
    state_dir: Option<&Path>,
    payload: &[u8],
) -> Result<ArtworkResolveResponse, String> {
    if payload.is_empty() {
        return Ok(ArtworkResolveResponse {
            v: 1,
            status: ResponseStatus::BadRequest,
            path: None,
            mime: None,
            detail: Some("empty payload".to_string()),
        });
    }

    let text = match std::str::from_utf8(payload) {
        Ok(t) => t,
        Err(e) => {
            return Ok(ArtworkResolveResponse {
                v: 1,
                status: ResponseStatus::BadRequest,
                path: None,
                mime: None,
                detail: Some(format!("payload is not UTF-8: {e}")),
            });
        }
    };

    let req: ArtworkResolveRequest = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            return Ok(ArtworkResolveResponse {
                v: 1,
                status: ResponseStatus::BadRequest,
                path: None,
                mime: None,
                detail: Some(format!("invalid JSON: {e}")),
            });
        }
    };
    if req.v != 1 {
        return Ok(ArtworkResolveResponse {
            v: 1,
            status: ResponseStatus::BadRequest,
            path: None,
            mime: None,
            detail: Some(format!("unsupported request v: {}", req.v)),
        });
    }

    match req.target.scheme.as_str() {
        SCHEME_MPD_ALBUM => Ok(ArtworkResolveResponse {
            v: 1,
            status: ResponseStatus::Unsupported,
            path: None,
            mime: None,
            detail: Some(
                "mpd_album: directory resolution is not available yet; use scheme mpd-path with the track file"
                    .to_string(),
            ),
        }),
        SCHEME_MPD_PATH => resolve_mpd_path(
            library_roots,
            state_dir,
            &req.target.value,
        ),
        other => Ok(ArtworkResolveResponse {
            v: 1,
            status: ResponseStatus::BadRequest,
            path: None,
            mime: None,
            detail: Some(format!("unknown target.scheme: {other}")),
        }),
    }
}

fn ok_from_path(cover: PathBuf) -> Result<ArtworkResolveResponse, String> {
    let mime = mime_for_path(&cover).map(str::to_string);
    let path = cover
        .to_str()
        .ok_or("cover path is not valid UTF-8; cannot represent in JSON")?
        .to_string();
    Ok(ArtworkResolveResponse {
        v: 1,
        status: ResponseStatus::Ok,
        path: Some(path),
        mime,
        detail: None,
    })
}

fn resolve_mpd_path(
    library_roots: &[PathBuf],
    state_dir: Option<&Path>,
    value: &str,
) -> Result<ArtworkResolveResponse, String> {
    if value.is_empty() {
        return Ok(ArtworkResolveResponse {
            v: 1,
            status: ResponseStatus::BadRequest,
            path: None,
            mime: None,
            detail: Some("empty mpd-path value".to_string()),
        });
    }

    let Some(track_path) = resolve_audio_path(library_roots, value) else {
        return Ok(ArtworkResolveResponse {
            v: 1,
            status: ResponseStatus::NotFound,
            path: None,
            mime: None,
            detail: Some("audio file not found for mpd_path".to_string()),
        });
    };

    if let Some(cover) = find_cover_beside_audio_file(&track_path) {
        return ok_from_path(cover);
    }

    if let Some(img) = embedded::read_embedded_cover(&track_path) {
        let Some(dir) = state_dir else {
            return Ok(ArtworkResolveResponse {
                v: 1,
                status: ResponseStatus::NotFound,
                path: None,
                mime: None,
                detail: Some(
                    "embedded cover in tags but no state_dir to write cache; cannot expose path"
                        .to_string(),
                ),
            });
        };
        let cached = embedded::write_embedded_to_cache(dir, &track_path, &img)?;
        return ok_from_path(cached);
    }

    Ok(ArtworkResolveResponse {
        v: 1,
        status: ResponseStatus::NotFound,
        path: None,
        mime: None,
        detail: Some("no sidecar or embedded cover for this track".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_cover_prefers_first_name() {
        let dir = tempfile::tempdir().unwrap();
        let flac = dir.path().join("1.flac");
        std::fs::write(&flac, b"x").unwrap();
        let _ = std::fs::write(dir.path().join("folder.jpg"), b"jpeg");
        let f2 = dir.path().join("cover.jpg");
        std::fs::write(&f2, b"j2").unwrap();
        // COVER_FILE_NAMES has cover.jpg before folder.jpg
        let got = find_cover_beside_audio_file(&flac).unwrap();
        assert_eq!(got.file_name().unwrap(), "cover.jpg");
    }

    #[test]
    fn resolve_mpd_path_with_root() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("Artist").join("Alb");
        std::fs::create_dir_all(&sub).unwrap();
        let flac = sub.join("1.flac");
        std::fs::write(&flac, b"x").unwrap();
        let _ = std::fs::write(sub.join("folder.jpg"), b"jpeg");

        let body = format!(
            r#"{{"v":1,"target":{{"scheme":"{}","value":"Artist/Alb/1.flac"}}}}"#,
            SCHEME_MPD_PATH
        );
        let r = resolve_artwork(&[dir.path().to_path_buf()], None, body.as_bytes()).unwrap();
        assert_eq!(r.status, ResponseStatus::Ok);
        assert!(r.path.as_ref().unwrap().ends_with("folder.jpg"));
        assert_eq!(r.mime.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn http_mpd_path_not_found() {
        let r = resolve_artwork(
            &[],
            None,
            r#"{"v":1,"target":{"scheme":"mpd-path","value":"http://x/a.flac"}}"#.as_bytes(),
        )
        .unwrap();
        assert_eq!(r.status, ResponseStatus::NotFound);
    }
}
