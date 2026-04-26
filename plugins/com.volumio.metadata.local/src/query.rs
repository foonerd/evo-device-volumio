//! `metadata.query` v1: resolve tag metadata for a track file (MPD `mpd-path`).

use std::path::{Path, PathBuf};

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::read_from_path;
use lofty::tag::{Accessor, Tag};
use serde::Deserialize;
use serde::Serialize;

/// `mpd-path`: MPD's `file` (library-relative or absolute).
pub(crate) const SCHEME_MPD_PATH: &str = "mpd-path";
/// `mpd-album`: `Artist|Album` — not implemented here.
pub(crate) const SCHEME_MPD_ALBUM: &str = "mpd-album";

/// Request (JSON v1; UTF-8). `target` matches [`ExternalAddressing`]-style schemes
/// from `com.volumio.playback.mpd`, aligned with `com.volumio.artwork.local`.
#[derive(Debug, Deserialize)]
pub(crate) struct MetadataQueryRequest {
    pub(crate) v: u8,
    pub(crate) target: QueryTarget,
}

#[derive(Debug, Deserialize)]
pub(crate) struct QueryTarget {
    pub(crate) scheme: String,
    pub(crate) value: String,
}

/// JSON body returned to the steward (v1).
#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct MetadataQueryResponse {
    v: u8,
    status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    genre: Option<String>,
    /// 1-based track number when present in tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    track: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    track_total: Option<u32>,
    /// Four-digit year when present / parseable.
    #[serde(skip_serializing_if = "Option::is_none")]
    year: Option<u32>,
    /// Audio length from the container; 0 if unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

/// Outcome of a query.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResponseStatus {
    Ok,
    NotFound,
    Unsupported,
    BadRequest,
}

impl MetadataQueryResponse {
    pub(crate) fn json_bytes(self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self)
    }
}

/// Resolve MPD `file` to a local path if the file exists.
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

/// Read tags and duration with lofty.
fn read_file_metadata(path: &Path) -> Result<MetadataQueryResponse, String> {
    let tagged = read_from_path(path).map_err(|e| format!("read audio file: {e}"))?;
    let duration = tagged.properties().duration();
    let duration_ms: u64 = duration.as_millis() as u64;
    let tag: Option<&Tag> = tagged.primary_tag().or_else(|| tagged.first_tag());
    let Some(t) = tag else {
        return Ok(metadata_response_from_parts(
            ResponseStatus::Ok,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            (duration_ms > 0).then_some(duration_ms),
            None,
        ));
    };
    Ok(response_from_tag(t, duration_ms, None))
}

/// Build a response from a single [`Tag`], e.g. for unit tests.
pub(crate) fn response_from_tag(
    tag: &Tag,
    duration_ms: u64,
    detail: Option<String>,
) -> MetadataQueryResponse {
    metadata_response_from_parts(
        ResponseStatus::Ok,
        opt_cow(tag.title()),
        opt_cow(tag.artist()),
        opt_cow(tag.album()),
        opt_cow(tag.genre()),
        tag.track(),
        tag.track_total(),
        tag.year(),
        (duration_ms > 0).then_some(duration_ms),
        detail,
    )
}

fn opt_cow(c: Option<std::borrow::Cow<'_, str>>) -> Option<String> {
    c.map(|s| s.into_owned())
}

// Central mapping from optional fields to the wire JSON; arity matches response columns.
#[allow(clippy::too_many_arguments)]
fn metadata_response_from_parts(
    status: ResponseStatus,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    genre: Option<String>,
    track: Option<u32>,
    track_total: Option<u32>,
    year: Option<u32>,
    duration_ms: Option<u64>,
    detail: Option<String>,
) -> MetadataQueryResponse {
    MetadataQueryResponse {
        v: 1,
        status,
        title,
        artist,
        album,
        genre,
        track,
        track_total,
        year,
        duration_ms,
        detail,
    }
}

/// Handle a `metadata.query` payload: parse JSON, resolve `mpd-path` to a file, read tags.
pub(crate) fn query_metadata(
    library_roots: &[PathBuf],
    payload: &[u8],
) -> Result<MetadataQueryResponse, String> {
    if payload.is_empty() {
        return Ok(metadata_response_from_parts(
            ResponseStatus::BadRequest,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("empty payload".to_string()),
        ));
    }

    let text = match std::str::from_utf8(payload) {
        Ok(t) => t,
        Err(e) => {
            return Ok(metadata_response_from_parts(
                ResponseStatus::BadRequest,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(format!("payload is not UTF-8: {e}")),
            ));
        }
    };

    let req: MetadataQueryRequest = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            return Ok(metadata_response_from_parts(
                ResponseStatus::BadRequest,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(format!("invalid JSON: {e}")),
            ));
        }
    };

    if req.v != 1 {
        return Ok(metadata_response_from_parts(
            ResponseStatus::BadRequest,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(format!("unsupported request v: {}", req.v)),
        ));
    }

    match req.target.scheme.as_str() {
        SCHEME_MPD_ALBUM => Ok(metadata_response_from_parts(
            ResponseStatus::Unsupported,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(
                "mpd_album: use scheme mpd-path with the track file until graph or library \
                 resolution exists"
                    .to_string(),
            ),
        )),
        SCHEME_MPD_PATH => {
            if req.target.value.is_empty() {
                return Ok(metadata_response_from_parts(
                    ResponseStatus::BadRequest,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some("empty mpd-path value".to_string()),
                ));
            }
            let Some(path) = resolve_audio_path(library_roots, &req.target.value) else {
                return Ok(metadata_response_from_parts(
                    ResponseStatus::NotFound,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some("audio file not found for mpd_path".to_string()),
                ));
            };
            read_file_metadata(&path).or_else(|e| {
                Ok(metadata_response_from_parts(
                    ResponseStatus::NotFound,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(e),
                ))
            })
        }
        other => Ok(metadata_response_from_parts(
            ResponseStatus::BadRequest,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(format!("unknown target.scheme: {other}")),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::tag::Tag;
    use lofty::tag::TagType;

    #[test]
    fn response_from_tag_maps_fields() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("Song".to_string());
        tag.set_artist("Band".to_string());
        tag.set_album("LP".to_string());
        tag.set_track(3);
        tag.set_genre("Rock".to_string());
        let r = response_from_tag(&tag, 120_000, None);
        assert_eq!(r.status, ResponseStatus::Ok);
        assert_eq!(r.title.as_deref(), Some("Song"));
        assert_eq!(r.artist.as_deref(), Some("Band"));
        assert_eq!(r.album.as_deref(), Some("LP"));
        assert_eq!(r.genre.as_deref(), Some("Rock"));
        assert_eq!(r.track, Some(3));
        assert_eq!(r.duration_ms, Some(120_000));
    }

    #[test]
    fn not_found_for_http_url() {
        let r = query_metadata(
            &[],
            r#"{"v":1,"target":{"scheme":"mpd-path","value":"http://x/a.flac"}}"#.as_bytes(),
        )
        .unwrap();
        assert_eq!(r.status, ResponseStatus::NotFound);
    }
}
