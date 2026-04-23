//! Long-lived MPD connection.
//!
//! One [`MpdConnection`] wraps one logical connection to an MPD
//! daemon, held for the duration of a custody (per the plugin's
//! warden contract). Phase 3.2 will build transport commands
//! (play, pause, seek, etc.), the `idle` subscription (in a second
//! [`MpdConnection`]; MPD blocks the connection during `idle` so
//! two connections are required for event-driven operation), and
//! reconnection logic on top of this layer.
//!
//! Every operation has an explicit deadline. No unbounded waits.
//! The connection is failure-honest: classified errors surface the
//! cause without masking transient conditions as permanent or vice
//! versa.

use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpStream, UnixStream};
use tokio::time;

use super::endpoint::MpdEndpoint;
use super::error::{MpdError, ProtocolError, TransportError};
use super::framing::Framing;
use super::protocol::{self, ClassifiedLine, Field};
use super::types::{MpdSong, MpdStatus, MpdVersion, PlayState};

/// Timeout budgets for a single connection.
///
/// Defaults tuned for a healthy local MPD: generous enough to
/// tolerate a loaded daemon, tight enough that a dead MPD does not
/// stall the warden. All values overridable when the Phase 3.3
/// configuration layer lands.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConnectTimeouts {
    /// Budget for completing the TCP or Unix connect syscall.
    pub(crate) connect: Duration,
    /// Budget for reading the welcome banner after the transport is
    /// up.
    pub(crate) welcome: Duration,
    /// Budget for a single command dispatch (write + read until OK
    /// or ACK).
    pub(crate) command: Duration,
}

impl Default for ConnectTimeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(5),
            welcome: Duration::from_secs(2),
            command: Duration::from_secs(3),
        }
    }
}

/// A live, one-shot connection to an MPD daemon.
///
/// Not cloneable, not reusable after failure: once a method returns
/// an error that indicates the connection is done for (closed,
/// protocol violation), the caller should drop this connection and
/// construct a new one. Phase 3.2 will wrap this in a supervisor
/// that does the reconnection automatically.
pub(crate) struct MpdConnection {
    framing: Framing<
        Box<dyn AsyncRead + Send + Unpin>,
        Box<dyn AsyncWrite + Send + Unpin>,
    >,
    version: MpdVersion,
    endpoint: MpdEndpoint,
    connected_at: Instant,
    command_timeout: Duration,
}

impl std::fmt::Debug for MpdConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MpdConnection")
            .field("endpoint", &self.endpoint)
            .field("version", &self.version)
            .field("connected_at", &self.connected_at)
            .field("command_timeout", &self.command_timeout)
            .finish()
    }
}

impl MpdConnection {
    /// Connect to `endpoint` with the default timeout budget, read
    /// the welcome banner, and return the live connection.
    pub(crate) async fn connect(endpoint: MpdEndpoint) -> Result<Self, MpdError> {
        Self::connect_with_timeouts(endpoint, ConnectTimeouts::default()).await
    }

    /// Connect with a caller-specified timeout budget.
    ///
    /// Used by tests and by the Phase 3.3 configuration layer where
    /// the operator can override the defaults.
    pub(crate) async fn connect_with_timeouts(
        endpoint: MpdEndpoint,
        timeouts: ConnectTimeouts,
    ) -> Result<Self, MpdError> {
        let (reader, writer) = open_streams(&endpoint, timeouts.connect).await?;
        handshake(reader, writer, endpoint, timeouts).await
    }

    /// The MPD protocol version negotiated at connect.
    pub(crate) fn version(&self) -> MpdVersion {
        self.version
    }

    /// The endpoint this connection points at. Useful for log
    /// context and for future reconnection logic.
    pub(crate) fn endpoint(&self) -> &MpdEndpoint {
        &self.endpoint
    }

    /// When this connection completed its handshake.
    pub(crate) fn connected_at(&self) -> Instant {
        self.connected_at
    }

    /// Dispatch `status` and project the response into [`MpdStatus`].
    pub(crate) async fn status(&mut self) -> Result<MpdStatus, MpdError> {
        let fields = self.dispatch("status", &[]).await?;
        parse_status(&fields)
    }

    /// Dispatch `currentsong` and project the response into
    /// `Option<MpdSong>`. Returns `None` when MPD's response is
    /// empty (no current song; queue empty or player stopped).
    pub(crate) async fn current_song(&mut self) -> Result<Option<MpdSong>, MpdError> {
        let fields = self.dispatch("currentsong", &[]).await?;
        parse_current_song(&fields)
    }

    /// Send a command and collect its body fields until OK or ACK.
    async fn dispatch(
        &mut self,
        command: &str,
        args: &[&str],
    ) -> Result<Vec<Field>, MpdError> {
        let bytes = protocol::serialise_command(command, args)?;

        tracing::debug!(
            plugin = crate::PLUGIN_NAME,
            endpoint = %self.endpoint,
            command,
            "mpd command dispatch"
        );

        self.framing
            .write_all_with_timeout(&bytes, self.command_timeout, "write_command")
            .await?;

        let mut fields = Vec::new();
        loop {
            let line = self
                .framing
                .read_line_with_timeout(self.command_timeout, "read_response")
                .await?;
            match protocol::classify_line(&line)? {
                ClassifiedLine::Ok => return Ok(fields),
                ClassifiedLine::Ack {
                    code,
                    list_position,
                    command,
                    message,
                } => {
                    return Err(MpdError::Ack {
                        code,
                        list_position,
                        command,
                        message,
                    });
                }
                ClassifiedLine::Field(f) => fields.push(f),
            }
        }
    }
}

/// Open the appropriate transport for `endpoint`, with a hard
/// connect-timeout budget. Returns the two type-erased halves ready
/// to be handed to [`Framing`].
async fn open_streams(
    endpoint: &MpdEndpoint,
    connect_budget: Duration,
) -> Result<
    (
        Box<dyn AsyncRead + Send + Unpin>,
        Box<dyn AsyncWrite + Send + Unpin>,
    ),
    MpdError,
> {
    match endpoint {
        MpdEndpoint::Tcp { host, port } => {
            let addr = format!("{}:{}", host, port);
            let stream = time::timeout(connect_budget, TcpStream::connect(&addr))
                .await
                .map_err(|_| MpdError::Timeout {
                    operation: "tcp_connect",
                    elapsed: connect_budget,
                })?
                .map_err(|e| {
                    MpdError::Transport(TransportError::TcpConnect {
                        endpoint: addr.clone(),
                        source: e,
                    })
                })?;
            // Disable Nagle: MPD dispatch is request-response on small
            // commands; coalescing adds latency without throughput gain.
            if let Err(e) = stream.set_nodelay(true) {
                tracing::warn!(
                    plugin = crate::PLUGIN_NAME,
                    error = %e,
                    "failed to set TCP_NODELAY; continuing"
                );
            }
            let (r, w) = stream.into_split();
            Ok((Box::new(r), Box::new(w)))
        }
        MpdEndpoint::Unix { path } => {
            let stream = time::timeout(connect_budget, UnixStream::connect(path))
                .await
                .map_err(|_| MpdError::Timeout {
                    operation: "unix_connect",
                    elapsed: connect_budget,
                })?
                .map_err(|e| {
                    MpdError::Transport(TransportError::UnixConnect {
                        path: path.display().to_string(),
                        source: e,
                    })
                })?;
            let (r, w) = stream.into_split();
            Ok((Box::new(r), Box::new(w)))
        }
    }
}

/// Read the welcome banner and construct the connection wrapper.
///
/// Extracted so tests can feed it a duplex pair without going
/// through real sockets.
async fn handshake(
    reader: Box<dyn AsyncRead + Send + Unpin>,
    writer: Box<dyn AsyncWrite + Send + Unpin>,
    endpoint: MpdEndpoint,
    timeouts: ConnectTimeouts,
) -> Result<MpdConnection, MpdError> {
    let mut framing = Framing::new(reader, writer);
    let welcome = framing
        .read_line_with_timeout(timeouts.welcome, "welcome")
        .await?;
    let version = protocol::parse_welcome(&welcome)?;

    tracing::info!(
        plugin = crate::PLUGIN_NAME,
        endpoint = %endpoint,
        mpd_version = %version,
        "mpd connection established"
    );

    Ok(MpdConnection {
        framing,
        version,
        endpoint,
        connected_at: Instant::now(),
        command_timeout: timeouts.command,
    })
}

// ----- Field projection into narrow types -----

fn parse_status(fields: &[Field]) -> Result<MpdStatus, MpdError> {
    let mut state: Option<PlayState> = None;
    let mut song_position: Option<u32> = None;
    let mut elapsed: Option<Duration> = None;
    let mut duration: Option<Duration> = None;
    let mut volume: Option<u8> = None;

    for f in fields {
        match f.key.as_str() {
            "state" => {
                state = Some(match f.value.as_str() {
                    "play" => PlayState::Playing,
                    "pause" => PlayState::Paused,
                    "stop" => PlayState::Stopped,
                    _ => {
                        return Err(MpdError::Protocol(ProtocolError::UnknownPlayState(
                            f.value.clone(),
                        )));
                    }
                });
            }
            "song" => {
                song_position = Some(parse_u32_field("song", &f.value)?);
            }
            "elapsed" => {
                elapsed = parse_duration_secs_field("elapsed", &f.value)?;
            }
            "duration" => {
                duration = parse_duration_secs_field("duration", &f.value)?;
            }
            "volume" => {
                volume = parse_volume_field(&f.value)?;
            }
            _ => {}
        }
    }

    let state = state.ok_or(MpdError::Protocol(ProtocolError::MissingField {
        field: "state",
    }))?;

    Ok(MpdStatus {
        state,
        song_position,
        elapsed,
        duration,
        volume,
    })
}

fn parse_current_song(fields: &[Field]) -> Result<Option<MpdSong>, MpdError> {
    if fields.is_empty() {
        return Ok(None);
    }

    let mut file_path: Option<String> = None;
    let mut title: Option<String> = None;
    let mut artist: Option<String> = None;
    let mut album: Option<String> = None;
    let mut duration: Option<Duration> = None;

    for f in fields {
        match f.key.as_str() {
            "file" => file_path = Some(f.value.clone()),
            "Title" => title = Some(f.value.clone()),
            "Artist" => artist = Some(f.value.clone()),
            "Album" => album = Some(f.value.clone()),
            "duration" => {
                duration = parse_duration_secs_field("duration", &f.value)?;
            }
            "Time" => {
                // Older MPD versions use integer seconds under `Time`.
                // Only use as fallback when `duration` was not present.
                if duration.is_none() {
                    if let Ok(secs) = f.value.parse::<u64>() {
                        duration = Some(Duration::from_secs(secs));
                    }
                }
            }
            _ => {}
        }
    }

    let Some(file_path) = file_path else {
        // `currentsong` returned fields but no `file`. Unusual; treat
        // as no current song rather than error, matching MPD's own
        // edge-case behaviour.
        return Ok(None);
    };

    Ok(Some(MpdSong {
        file_path,
        title,
        artist,
        album,
        duration,
    }))
}

fn parse_u32_field(field: &'static str, value: &str) -> Result<u32, MpdError> {
    value.parse::<u32>().map_err(|_| {
        MpdError::Protocol(ProtocolError::UnparseableField {
            field,
            value: value.to_string(),
        })
    })
}

fn parse_duration_secs_field(
    field: &'static str,
    value: &str,
) -> Result<Option<Duration>, MpdError> {
    let secs = value.parse::<f64>().map_err(|_| {
        MpdError::Protocol(ProtocolError::UnparseableField {
            field,
            value: value.to_string(),
        })
    })?;
    if !secs.is_finite() || secs < 0.0 {
        return Ok(None);
    }
    Ok(Some(Duration::from_secs_f64(secs)))
}

fn parse_volume_field(value: &str) -> Result<Option<u8>, MpdError> {
    let raw = value.parse::<i32>().map_err(|_| {
        MpdError::Protocol(ProtocolError::UnparseableField {
            field: "volume",
            value: value.to_string(),
        })
    })?;
    // MPD reports -1 when no mixer is configured. Other out-of-range
    // values are treated as "unknown" rather than erroring out,
    // matching MPD's own liberal clamping behaviour.
    if (0..=100).contains(&raw) {
        Ok(Some(raw as u8))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;
    use std::time::Duration;

    use tokio::io::{duplex, AsyncWriteExt};
    use tokio::net::{TcpListener, UnixListener};

    // ----- helpers -----

    fn fake_endpoint() -> MpdEndpoint {
        MpdEndpoint::tcp("mock", 0).unwrap()
    }

    fn short_timeouts() -> ConnectTimeouts {
        ConnectTimeouts {
            connect: Duration::from_millis(500),
            welcome: Duration::from_millis(500),
            command: Duration::from_millis(500),
        }
    }

    /// Spawn a mock MPD on the given duplex-server half. The script
    /// is written to the client; everything the client writes is
    /// drained silently.
    fn spawn_script(mut server: tokio::io::DuplexStream, script: &'static [u8]) {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            server.write_all(script).await.unwrap();
            server.flush().await.unwrap();
            // Drain whatever the client writes, until disconnect.
            let mut buf = vec![0u8; 1024];
            loop {
                match server.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });
    }

    /// Spawn a mock MPD that writes `script` and, once it detects the
    /// client has sent any command, follows up with `response`.
    fn spawn_scripted_exchange(
        mut server: tokio::io::DuplexStream,
        welcome: &'static [u8],
        response: &'static [u8],
    ) {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            server.write_all(welcome).await.unwrap();
            server.flush().await.unwrap();

            // Wait until the client has sent a full command line
            // (anything ending in '\n').
            let mut accum: Vec<u8> = Vec::new();
            let mut buf = vec![0u8; 1024];
            loop {
                let n = match server.read(&mut buf).await {
                    Ok(0) => return,
                    Ok(n) => n,
                    Err(_) => return,
                };
                accum.extend_from_slice(&buf[..n]);
                if accum.contains(&b'\n') {
                    break;
                }
            }

            server.write_all(response).await.unwrap();
            server.flush().await.unwrap();

            // Keep connection open so the client's subsequent OK-read
            // does not see EOF.
            loop {
                match server.read(&mut buf).await {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {}
                }
            }
        });
    }

    async fn handshake_from_duplex(
        server: tokio::io::DuplexStream,
        client: tokio::io::DuplexStream,
        welcome: &'static [u8],
    ) -> Result<MpdConnection, MpdError> {
        spawn_script(server, welcome);
        let (r, w) = tokio::io::split(client);
        handshake(Box::new(r), Box::new(w), fake_endpoint(), short_timeouts()).await
    }

    // ----- handshake behaviour -----

    #[tokio::test]
    async fn connect_parses_welcome_banner() {
        let (server, client) = duplex(1024);
        let conn = handshake_from_duplex(server, client, b"OK MPD 0.23.5\n")
            .await
            .unwrap();
        assert_eq!(conn.version(), MpdVersion::new(0, 23, 5));
    }

    #[tokio::test]
    async fn connect_rejects_bad_welcome() {
        let (server, client) = duplex(1024);
        let err = handshake_from_duplex(server, client, b"NOT A WELCOME\n")
            .await
            .unwrap_err();
        assert!(matches!(err, MpdError::Protocol(ProtocolError::BadWelcome(_))));
    }

    #[tokio::test]
    async fn connect_rejects_bad_version() {
        let (server, client) = duplex(1024);
        let err = handshake_from_duplex(server, client, b"OK MPD something\n")
            .await
            .unwrap_err();
        assert!(matches!(err, MpdError::Protocol(ProtocolError::BadVersion(_))));
    }

    #[tokio::test]
    async fn connect_returns_closed_when_peer_closes_without_welcome() {
        let (server, client) = duplex(1024);
        drop(server);
        let (r, w) = tokio::io::split(client);
        let err = handshake(
            Box::new(r),
            Box::new(w),
            fake_endpoint(),
            short_timeouts(),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            err,
            MpdError::Transport(TransportError::Closed)
        ));
    }

    // ----- status dispatch -----

    #[tokio::test]
    async fn status_parses_play_state_and_fields() {
        let (server, client) = duplex(4096);
        spawn_scripted_exchange(
            server,
            b"OK MPD 0.23.5\n",
            b"volume: 50\nstate: play\nsong: 3\nelapsed: 12.345\nduration: 180.0\nOK\n",
        );
        let (r, w) = tokio::io::split(client);
        let mut conn = handshake(
            Box::new(r),
            Box::new(w),
            fake_endpoint(),
            short_timeouts(),
        )
        .await
        .unwrap();

        let s = conn.status().await.unwrap();
        assert_eq!(s.state, PlayState::Playing);
        assert_eq!(s.song_position, Some(3));
        assert_eq!(s.volume, Some(50));
        assert_eq!(s.elapsed, Some(Duration::from_millis(12_345)));
        assert_eq!(s.duration, Some(Duration::from_millis(180_000)));
    }

    #[tokio::test]
    async fn status_handles_volume_minus_one_as_unknown() {
        let (server, client) = duplex(4096);
        spawn_scripted_exchange(
            server,
            b"OK MPD 0.23.5\n",
            b"volume: -1\nstate: stop\nOK\n",
        );
        let (r, w) = tokio::io::split(client);
        let mut conn = handshake(
            Box::new(r),
            Box::new(w),
            fake_endpoint(),
            short_timeouts(),
        )
        .await
        .unwrap();

        let s = conn.status().await.unwrap();
        assert_eq!(s.state, PlayState::Stopped);
        assert_eq!(s.volume, None);
        assert_eq!(s.song_position, None);
    }

    #[tokio::test]
    async fn status_reports_pause_state() {
        let (server, client) = duplex(4096);
        spawn_scripted_exchange(
            server,
            b"OK MPD 0.23.5\n",
            b"state: pause\nsong: 0\nOK\n",
        );
        let (r, w) = tokio::io::split(client);
        let mut conn = handshake(
            Box::new(r),
            Box::new(w),
            fake_endpoint(),
            short_timeouts(),
        )
        .await
        .unwrap();

        let s = conn.status().await.unwrap();
        assert_eq!(s.state, PlayState::Paused);
    }

    #[tokio::test]
    async fn status_errors_on_unknown_play_state() {
        let (server, client) = duplex(4096);
        spawn_scripted_exchange(
            server,
            b"OK MPD 0.23.5\n",
            b"state: wibbling\nOK\n",
        );
        let (r, w) = tokio::io::split(client);
        let mut conn = handshake(
            Box::new(r),
            Box::new(w),
            fake_endpoint(),
            short_timeouts(),
        )
        .await
        .unwrap();

        let err = conn.status().await.unwrap_err();
        assert!(matches!(
            err,
            MpdError::Protocol(ProtocolError::UnknownPlayState(_))
        ));
    }

    #[tokio::test]
    async fn status_errors_when_state_field_missing() {
        let (server, client) = duplex(4096);
        spawn_scripted_exchange(
            server,
            b"OK MPD 0.23.5\n",
            b"volume: 50\nsong: 3\nOK\n",
        );
        let (r, w) = tokio::io::split(client);
        let mut conn = handshake(
            Box::new(r),
            Box::new(w),
            fake_endpoint(),
            short_timeouts(),
        )
        .await
        .unwrap();

        let err = conn.status().await.unwrap_err();
        assert!(matches!(
            err,
            MpdError::Protocol(ProtocolError::MissingField { field: "state" })
        ));
    }

    #[tokio::test]
    async fn status_surfaces_ack_as_mpderror_ack() {
        let (server, client) = duplex(4096);
        spawn_scripted_exchange(
            server,
            b"OK MPD 0.23.5\n",
            b"ACK [2@0] {status} Bad argument\n",
        );
        let (r, w) = tokio::io::split(client);
        let mut conn = handshake(
            Box::new(r),
            Box::new(w),
            fake_endpoint(),
            short_timeouts(),
        )
        .await
        .unwrap();

        let err = conn.status().await.unwrap_err();
        match err {
            MpdError::Ack { code, command, message, .. } => {
                assert_eq!(code, 2);
                assert_eq!(command, "status");
                assert_eq!(message, "Bad argument");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    // ----- current_song dispatch -----

    #[tokio::test]
    async fn current_song_populated_returns_some() {
        let (server, client) = duplex(4096);
        spawn_scripted_exchange(
            server,
            b"OK MPD 0.23.5\n",
            b"file: INTERNAL/Artist/Album/track.flac\nTitle: Track One\nArtist: An Artist\nAlbum: An Album\nduration: 242.5\nOK\n",
        );
        let (r, w) = tokio::io::split(client);
        let mut conn = handshake(
            Box::new(r),
            Box::new(w),
            fake_endpoint(),
            short_timeouts(),
        )
        .await
        .unwrap();

        let s = conn.current_song().await.unwrap().unwrap();
        assert_eq!(s.file_path, "INTERNAL/Artist/Album/track.flac");
        assert_eq!(s.title.as_deref(), Some("Track One"));
        assert_eq!(s.artist.as_deref(), Some("An Artist"));
        assert_eq!(s.album.as_deref(), Some("An Album"));
        assert_eq!(s.duration, Some(Duration::from_millis(242_500)));
    }

    #[tokio::test]
    async fn current_song_empty_response_returns_none() {
        let (server, client) = duplex(4096);
        spawn_scripted_exchange(server, b"OK MPD 0.23.5\n", b"OK\n");
        let (r, w) = tokio::io::split(client);
        let mut conn = handshake(
            Box::new(r),
            Box::new(w),
            fake_endpoint(),
            short_timeouts(),
        )
        .await
        .unwrap();

        let s = conn.current_song().await.unwrap();
        assert!(s.is_none());
    }

    #[tokio::test]
    async fn current_song_uses_time_as_duration_fallback_on_old_mpd() {
        let (server, client) = duplex(4096);
        spawn_scripted_exchange(
            server,
            b"OK MPD 0.21.0\n",
            b"file: x.flac\nTitle: t\nTime: 300\nOK\n",
        );
        let (r, w) = tokio::io::split(client);
        let mut conn = handshake(
            Box::new(r),
            Box::new(w),
            fake_endpoint(),
            short_timeouts(),
        )
        .await
        .unwrap();

        let s = conn.current_song().await.unwrap().unwrap();
        assert_eq!(s.duration, Some(Duration::from_secs(300)));
    }

    // ----- real-transport integration -----

    #[tokio::test]
    async fn connect_works_over_real_tcp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(b"OK MPD 0.23.5\n").await.unwrap();
            stream.flush().await.unwrap();
            // Keep open briefly for the handshake to complete.
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        let endpoint = MpdEndpoint::tcp(addr.ip().to_string(), addr.port()).unwrap();
        let conn = MpdConnection::connect_with_timeouts(endpoint, short_timeouts())
            .await
            .unwrap();
        assert_eq!(conn.version(), MpdVersion::new(0, 23, 5));
    }

    #[tokio::test]
    async fn connect_works_over_real_unix_socket() {
        let dir = std::env::temp_dir();
        let path: PathBuf = dir.join(format!(
            "evo-volumio-mpd-test-{}.sock",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let listener = UnixListener::bind(&path).unwrap();
        let path_for_endpoint = path.clone();
        let path_for_cleanup = path.clone();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(b"OK MPD 0.23.5\n").await.unwrap();
            stream.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        let endpoint = MpdEndpoint::unix(path_for_endpoint).unwrap();
        let conn = MpdConnection::connect_with_timeouts(endpoint, short_timeouts()).await;
        let _ = server.await;
        let _ = std::fs::remove_file(&path_for_cleanup);

        let conn = conn.unwrap();
        assert_eq!(conn.version(), MpdVersion::new(0, 23, 5));
    }

    #[tokio::test]
    async fn connect_times_out_when_welcome_never_arrives() {
        let (server, client) = duplex(1024);
        // Hold server open, never write anything.
        let _hold = tokio::spawn(async move {
            let _keep = server;
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        let (r, w) = tokio::io::split(client);

        let tight = ConnectTimeouts {
            connect: Duration::from_millis(500),
            welcome: Duration::from_millis(50),
            command: Duration::from_millis(500),
        };

        let err = handshake(Box::new(r), Box::new(w), fake_endpoint(), tight)
            .await
            .unwrap_err();
        match err {
            MpdError::Timeout { operation, .. } => {
                assert_eq!(operation, "welcome");
            }
            other => panic!("expected welcome timeout, got {other:?}"),
        }
    }

    // ----- field-projection unit tests -----

    #[test]
    fn parse_status_requires_state() {
        let fields = vec![Field {
            key: "volume".into(),
            value: "50".into(),
        }];
        let err = parse_status(&fields).unwrap_err();
        assert!(matches!(
            err,
            MpdError::Protocol(ProtocolError::MissingField { field: "state" })
        ));
    }

    #[test]
    fn parse_status_ignores_unknown_fields() {
        let fields = vec![
            Field { key: "state".into(), value: "play".into() },
            Field { key: "unknown_field".into(), value: "value".into() },
            Field { key: "xfade".into(), value: "2".into() },
        ];
        let s = parse_status(&fields).unwrap();
        assert_eq!(s.state, PlayState::Playing);
    }

    #[test]
    fn parse_current_song_missing_file_returns_none() {
        let fields = vec![Field {
            key: "Title".into(),
            value: "Something".into(),
        }];
        let s = parse_current_song(&fields).unwrap();
        assert!(s.is_none());
    }
}
