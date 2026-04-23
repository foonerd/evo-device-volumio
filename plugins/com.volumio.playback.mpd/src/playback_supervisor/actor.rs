//! Playback supervisor actor.
//!
//! Long-lived orchestrator that holds two [`MpdConnection`]s for
//! the duration of a custody: one dedicated to command dispatch,
//! one dedicated to the MPD idle subprotocol. Two connections are
//! required because MPD blocks the connection while an idle call
//! is pending, so running idle and commands on the same socket is
//! impossible.
//!
//! # Architecture
//!
//! Two tokio tasks communicating via channels:
//!
//! - **Main supervisor task** ([`supervisor_run`]): owns
//!   `command_connection`. Receives [`SupervisorMessage`] values
//!   from an `mpsc::Receiver`, dispatches them against the command
//!   connection, emits state reports through the reporter. Handles
//!   shutdown on a `oneshot::Receiver`. Reconnects with bounded
//!   exponential backoff when the command connection fails.
//! - **Idle task** ([`idle_task`]): owns `idle_connection`. Loops
//!   on [`MpdConnection::idle`] against `[Player, Mixer, Options,
//!   Playlist]` with a 30s per-call budget. Sends `IdleEvent`
//!   values to the main supervisor via a second `mpsc::Sender`.
//!   Reconnects with the same backoff when idle fails.
//!
//! Separation by task rather than a single `select!` avoids the
//! borrow-conflict and cancellation hazard: a `select!` arm that
//! called `idle(&mut self, ...)` would hold `&mut conn` across an
//! await for up to 30s, blocking the other arms from using the
//! same connection even if they wanted a different one.
//!
//! # Failure classification
//!
//! - [`MpdError::Ack`]: command-level rejection, connection stays
//!   healthy. Not retried; surfaced as [`PlaybackError::Ack`].
//! - [`MpdError::Transport`] / [`MpdError::Timeout`]: connection
//!   is suspect. Triggers reconnection with backoff; the command
//!   is retried exactly once after a successful reconnect.
//! - [`MpdError::Protocol`] / [`MpdError::Config`]: non-retryable.
//!   Surfaced as [`PlaybackError::Protocol`].
//!
//! # State reports
//!
//! Emitted at three trigger points:
//! 1. Initial report during [`spawn`] (synchronous; failure here
//!    aborts spawn).
//! 2. After every successful command (best-effort; failure warns
//!    but does not break the supervisor).
//! 3. After every non-empty idle event (best-effort).
//!
//! Each emission is a fresh `status` + `currentsong` on the
//! command connection, projected to [`PlaybackStateReport`],
//! serialised to TOML, sent via the reporter.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use evo_plugin_sdk::contract::{CustodyHandle, CustodyStateReporter, HealthStatus};

use crate::mpd::{
    ConnectTimeouts, IdleSubsystem, MpdConnection, MpdEndpoint, MpdError,
};
use crate::PLUGIN_NAME;

use super::command::{PlaybackCommand, PlaybackError};
use super::report::PlaybackStateReport;

// ----- tuning constants -----

/// Initial delay before the first reconnect attempt.
const RECONNECT_INITIAL: Duration = Duration::from_millis(100);
/// Upper bound on the delay between reconnect attempts.
const RECONNECT_MAX: Duration = Duration::from_secs(10);
/// Maximum number of reconnect attempts before reporting
/// exhausted.
const RECONNECT_MAX_ATTEMPTS: u32 = 10;
/// Budget per [`MpdConnection::idle`] call on the idle task.
const IDLE_BUDGET: Duration = Duration::from_secs(30);
/// Subsystems the idle task subscribes to. Covers everything that
/// affects the fields reported in [`PlaybackStateReport`].
const IDLE_SUBSYSTEMS: &[IdleSubsystem] = &[
    IdleSubsystem::Player,
    IdleSubsystem::Mixer,
    IdleSubsystem::Options,
    IdleSubsystem::Playlist,
];
/// Bounded capacity for the external-command channel. Values
/// smaller than ~8 would risk blocking the warden's
/// `course_correct`; larger than ~64 buys nothing for a human-
/// driven UI.
const COMMAND_CHANNEL_CAPACITY: usize = 32;
/// Bounded capacity for the idle-event channel. MPD idle events
/// arrive sparsely (seconds apart at most), so a small capacity
/// suffices.
const IDLE_CHANNEL_CAPACITY: usize = 8;

// ----- public-within-crate surface -----

/// Handle the warden retains for the life of a custody. Dropping
/// it is equivalent to calling [`SupervisorHandle::shutdown`]: the
/// `command_tx` half drops, the supervisor's `recv` returns
/// `None`, the run loop exits. Explicit `shutdown()` is preferred
/// so the caller can await completion.
pub(crate) struct SupervisorHandle {
    command_tx: mpsc::Sender<SupervisorMessage>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task_handle: Option<JoinHandle<()>>,
}

impl SupervisorHandle {
    /// Dispatch a command. Returns once the supervisor has either
    /// executed the command, surfaced an ACK, reached the
    /// reconnection limit, or shut down.
    pub(crate) async fn command(
        &self,
        cmd: PlaybackCommand,
    ) -> Result<(), PlaybackError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(SupervisorMessage::Command {
                cmd,
                reply: reply_tx,
            })
            .await
            .map_err(|_| PlaybackError::Shutdown)?;
        match reply_rx.await {
            Ok(result) => result,
            Err(_) => Err(PlaybackError::Shutdown),
        }
    }

    /// Signal shutdown and wait for the supervisor's task to
    /// finish. Idempotent: calling a second time is a no-op.
    pub(crate) async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.task_handle.take() {
            let _ = h.await;
        }
    }
}

/// Open both connections, emit the initial state report, spawn
/// both tasks, return the handle.
///
/// Either connection failing to open, or the initial report
/// failing to be produced, aborts the whole spawn: no tasks are
/// spawned, no resources leak. The caller's
/// [`Plugin::take_custody`] impl propagates the error.
pub(crate) async fn spawn(
    endpoint: MpdEndpoint,
    timeouts: ConnectTimeouts,
    custody_handle: CustodyHandle,
    reporter: Arc<dyn CustodyStateReporter>,
) -> Result<SupervisorHandle, PlaybackError> {
    tracing::info!(
        plugin = PLUGIN_NAME,
        handle = %custody_handle.id,
        endpoint = %endpoint,
        "spawning playback supervisor"
    );

    let mut cmd_conn = MpdConnection::connect_with_timeouts(
        endpoint.clone(),
        timeouts,
    )
    .await
    .map_err(classify_connect_error)?;
    let idle_conn = MpdConnection::connect_with_timeouts(
        endpoint.clone(),
        timeouts,
    )
    .await
    .map_err(classify_connect_error)?;

    // Initial report: failure here means MPD is unusable, so bail
    // before spawning anything.
    emit_initial_report(
        &mut cmd_conn,
        &custody_handle,
        reporter.as_ref(),
    )
    .await?;

    let (command_tx, command_rx) =
        mpsc::channel::<SupervisorMessage>(COMMAND_CHANNEL_CAPACITY);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let (idle_tx, idle_rx) =
        mpsc::channel::<IdleEvent>(IDLE_CHANNEL_CAPACITY);

    let idle_endpoint = endpoint.clone();
    tokio::spawn(idle_task(idle_conn, idle_endpoint, timeouts, idle_tx));

    let task_handle = tokio::spawn(supervisor_run(
        cmd_conn,
        endpoint,
        timeouts,
        command_rx,
        shutdown_rx,
        idle_rx,
        custody_handle,
        reporter,
    ));

    Ok(SupervisorHandle {
        command_tx,
        shutdown_tx: Some(shutdown_tx),
        task_handle: Some(task_handle),
    })
}

// ----- internal types -----

/// Messages the main supervisor task consumes on its command
/// channel. Extending the enum (e.g. for health-probe queries) is
/// a source-only change; the channel signature is
/// `mpsc::Sender<SupervisorMessage>`.
enum SupervisorMessage {
    Command {
        cmd: PlaybackCommand,
        reply: oneshot::Sender<Result<(), PlaybackError>>,
    },
}

/// Events the idle task sends to the main supervisor.
enum IdleEvent {
    /// One or more subsystems changed. The supervisor emits a
    /// fresh state report in response.
    Changed(Vec<IdleSubsystem>),
    /// The idle task exhausted its reconnect attempts and has
    /// terminated. No further events will arrive on this channel.
    /// The supervisor logs and continues running command-only.
    Exhausted,
}

/// Exponential backoff state, per reconnection sequence.
///
/// `next_delay` doubles the delay each call up to [`RECONNECT_MAX`],
/// returning `None` after [`RECONNECT_MAX_ATTEMPTS`] have been
/// consumed. `reset` returns the state to its initial condition
/// after a successful connect.
struct BackoffState {
    attempt: u32,
    max_attempts: u32,
    initial: Duration,
    max: Duration,
}

impl BackoffState {
    fn new() -> Self {
        Self {
            attempt: 0,
            max_attempts: RECONNECT_MAX_ATTEMPTS,
            initial: RECONNECT_INITIAL,
            max: RECONNECT_MAX,
        }
    }

    fn next_delay(&mut self) -> Option<Duration> {
        if self.attempt >= self.max_attempts {
            return None;
        }
        let multiplier = 1u32 << self.attempt.min(16);
        let raw = self.initial.saturating_mul(multiplier);
        let delay = if raw > self.max { self.max } else { raw };
        self.attempt += 1;
        Some(delay)
    }

    fn attempts_used(&self) -> u32 {
        self.attempt
    }
}

// ----- main supervisor task -----

async fn supervisor_run(
    mut cmd_conn: MpdConnection,
    endpoint: MpdEndpoint,
    timeouts: ConnectTimeouts,
    mut command_rx: mpsc::Receiver<SupervisorMessage>,
    mut shutdown_rx: oneshot::Receiver<()>,
    mut idle_rx: mpsc::Receiver<IdleEvent>,
    custody_handle: CustodyHandle,
    reporter: Arc<dyn CustodyStateReporter>,
) {
    tracing::info!(
        plugin = PLUGIN_NAME,
        handle = %custody_handle.id,
        "playback supervisor task started"
    );

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown_rx => {
                tracing::info!(
                    plugin = PLUGIN_NAME,
                    handle = %custody_handle.id,
                    "supervisor received shutdown signal"
                );
                return;
            }
            msg = command_rx.recv() => {
                match msg {
                    None => {
                        tracing::info!(
                            plugin = PLUGIN_NAME,
                            handle = %custody_handle.id,
                            "command channel closed; supervisor exiting"
                        );
                        return;
                    }
                    Some(SupervisorMessage::Command { cmd, reply }) => {
                        let result = handle_command(
                            cmd,
                            &mut cmd_conn,
                            &endpoint,
                            timeouts,
                        ).await;
                        let ok = result.is_ok();
                        let _ = reply.send(result);
                        if ok {
                            emit_best_effort_report(
                                &mut cmd_conn,
                                &custody_handle,
                                reporter.as_ref(),
                            ).await;
                        }
                    }
                }
            }
            evt = idle_rx.recv() => {
                match evt {
                    None | Some(IdleEvent::Exhausted) => {
                        tracing::warn!(
                            plugin = PLUGIN_NAME,
                            handle = %custody_handle.id,
                            "idle task terminated; continuing command-only"
                        );
                    }
                    Some(IdleEvent::Changed(changed)) => {
                        tracing::debug!(
                            plugin = PLUGIN_NAME,
                            handle = %custody_handle.id,
                            changed_count = changed.len(),
                            "idle wake"
                        );
                        emit_best_effort_report(
                            &mut cmd_conn,
                            &custody_handle,
                            reporter.as_ref(),
                        ).await;
                    }
                }
            }
        }
    }
}

async fn handle_command(
    cmd: PlaybackCommand,
    cmd_conn: &mut MpdConnection,
    endpoint: &MpdEndpoint,
    timeouts: ConnectTimeouts,
) -> Result<(), PlaybackError> {
    // First attempt on the current connection.
    match dispatch_command(cmd.clone(), cmd_conn).await {
        Ok(()) => return Ok(()),
        Err(e) if !error_calls_for_reconnect(&e) => {
            return Err(classify_command_error(e));
        }
        Err(e) => {
            tracing::warn!(
                plugin = PLUGIN_NAME,
                error = %e,
                "command hit transient error; reconnecting"
            );
        }
    }

    // Reconnect loop with backoff.
    let mut backoff = BackoffState::new();
    loop {
        let delay = match backoff.next_delay() {
            Some(d) => d,
            None => {
                return Err(PlaybackError::ConnectionExhausted {
                    attempts: backoff.attempts_used(),
                });
            }
        };
        tokio::time::sleep(delay).await;

        match MpdConnection::connect_with_timeouts(
            endpoint.clone(),
            timeouts,
        )
        .await
        {
            Ok(new_conn) => {
                *cmd_conn = new_conn;
                tracing::info!(
                    plugin = PLUGIN_NAME,
                    attempts = backoff.attempts_used(),
                    "command connection re-established"
                );
                break;
            }
            Err(e) if error_calls_for_reconnect(&e) => {
                tracing::debug!(
                    plugin = PLUGIN_NAME,
                    error = %e,
                    attempt = backoff.attempts_used(),
                    "reconnect attempt failed"
                );
                continue;
            }
            Err(e) => {
                return Err(classify_command_error(e));
            }
        }
    }

    // Retry the command once on the fresh connection.
    match dispatch_command(cmd, cmd_conn).await {
        Ok(()) => Ok(()),
        Err(e) => Err(classify_command_error(e)),
    }
}

async fn dispatch_command(
    cmd: PlaybackCommand,
    cmd_conn: &mut MpdConnection,
) -> Result<(), MpdError> {
    match cmd {
        PlaybackCommand::Play => cmd_conn.play().await,
        PlaybackCommand::PlayPosition(p) => cmd_conn.play_position(p).await,
        PlaybackCommand::Pause(p) => cmd_conn.pause(p).await,
        PlaybackCommand::Stop => cmd_conn.stop().await,
        PlaybackCommand::Next => cmd_conn.next().await,
        PlaybackCommand::Previous => cmd_conn.previous().await,
        PlaybackCommand::Seek(d) => cmd_conn.seek(d).await,
        PlaybackCommand::SetVolume(v) => cmd_conn.set_volume(v).await,
    }
}

fn error_calls_for_reconnect(e: &MpdError) -> bool {
    matches!(e, MpdError::Transport(_) | MpdError::Timeout { .. })
}

fn classify_connect_error(e: MpdError) -> PlaybackError {
    match e {
        MpdError::Transport(_) | MpdError::Timeout { .. } => {
            PlaybackError::ConnectionExhausted { attempts: 1 }
        }
        MpdError::Protocol(_) | MpdError::Config(_) => {
            PlaybackError::Protocol(format!("{}", e))
        }
        MpdError::Ack { code, message, .. } => {
            PlaybackError::Ack { code, message }
        }
    }
}

fn classify_command_error(e: MpdError) -> PlaybackError {
    match e {
        MpdError::Ack { code, message, .. } => {
            PlaybackError::Ack { code, message }
        }
        MpdError::Transport(_) | MpdError::Timeout { .. } => {
            PlaybackError::ConnectionExhausted {
                attempts: RECONNECT_MAX_ATTEMPTS,
            }
        }
        MpdError::Protocol(_) | MpdError::Config(_) => {
            PlaybackError::Protocol(format!("{}", e))
        }
    }
}

// ----- state report emission -----

async fn emit_initial_report(
    cmd_conn: &mut MpdConnection,
    custody_handle: &CustodyHandle,
    reporter: &dyn CustodyStateReporter,
) -> Result<(), PlaybackError> {
    let status = cmd_conn.status().await.map_err(classify_command_error)?;
    let song = cmd_conn
        .current_song()
        .await
        .map_err(classify_command_error)?;
    let report = PlaybackStateReport::from_mpd(status, song);
    let payload = report.serialise().into_bytes();
    if let Err(e) = reporter
        .report(custody_handle, payload, HealthStatus::Healthy)
        .await
    {
        tracing::warn!(
            plugin = PLUGIN_NAME,
            handle = %custody_handle.id,
            error = %e,
            "initial state report delivery failed; spawn proceeds anyway"
        );
    }
    Ok(())
}

async fn emit_best_effort_report(
    cmd_conn: &mut MpdConnection,
    custody_handle: &CustodyHandle,
    reporter: &dyn CustodyStateReporter,
) {
    let status = match cmd_conn.status().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                plugin = PLUGIN_NAME,
                handle = %custody_handle.id,
                error = %e,
                "state report: status query failed"
            );
            return;
        }
    };
    let song = match cmd_conn.current_song().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                plugin = PLUGIN_NAME,
                handle = %custody_handle.id,
                error = %e,
                "state report: currentsong query failed"
            );
            return;
        }
    };
    let report = PlaybackStateReport::from_mpd(status, song);
    let payload = report.serialise().into_bytes();
    if let Err(e) = reporter
        .report(custody_handle, payload, HealthStatus::Healthy)
        .await
    {
        tracing::warn!(
            plugin = PLUGIN_NAME,
            handle = %custody_handle.id,
            error = %e,
            "state report delivery failed"
        );
    }
}

// ----- idle task -----

async fn idle_task(
    mut idle_conn: MpdConnection,
    endpoint: MpdEndpoint,
    timeouts: ConnectTimeouts,
    tx: mpsc::Sender<IdleEvent>,
) {
    tracing::info!(plugin = PLUGIN_NAME, "idle task started");
    loop {
        match idle_conn.idle(IDLE_SUBSYSTEMS, IDLE_BUDGET).await {
            Ok(changed) if changed.is_empty() => {
                // No-change OK (e.g. from a noidle from elsewhere).
                // Re-enter idle; no event to emit.
                continue;
            }
            Ok(changed) => {
                if tx.send(IdleEvent::Changed(changed)).await.is_err() {
                    tracing::info!(
                        plugin = PLUGIN_NAME,
                        "idle task: event receiver dropped, exiting"
                    );
                    return;
                }
            }
            Err(e) => {
                tracing::warn!(
                    plugin = PLUGIN_NAME,
                    error = %e,
                    "idle failed; will reconnect"
                );
                let mut backoff = BackoffState::new();
                let reconnected = loop {
                    let delay = match backoff.next_delay() {
                        Some(d) => d,
                        None => break None,
                    };
                    tokio::time::sleep(delay).await;
                    match MpdConnection::connect_with_timeouts(
                        endpoint.clone(),
                        timeouts,
                    )
                    .await
                    {
                        Ok(c) => break Some(c),
                        Err(err) => {
                            tracing::debug!(
                                plugin = PLUGIN_NAME,
                                error = %err,
                                attempt = backoff.attempts_used(),
                                "idle reconnect attempt failed"
                            );
                            continue;
                        }
                    }
                };
                match reconnected {
                    Some(c) => {
                        idle_conn = c;
                        tracing::info!(
                            plugin = PLUGIN_NAME,
                            "idle connection re-established"
                        );
                    }
                    None => {
                        let _ = tx.send(IdleEvent::Exhausted).await;
                        tracing::warn!(
                            plugin = PLUGIN_NAME,
                            "idle task exhausted reconnect attempts; exiting"
                        );
                        return;
                    }
                }
            }
        }
    }
}

// ----- tests -----

#[cfg(test)]
mod tests {
    use super::*;

    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use evo_plugin_sdk::contract::ReportError;

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::task::JoinHandle;

    use std::future::Future;

    // ----- backoff unit tests -----

    #[test]
    fn backoff_delays_double_up_to_cap() {
        let mut b = BackoffState::new();
        assert_eq!(b.next_delay(), Some(Duration::from_millis(100)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(200)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(400)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(800)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(1600)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(3200)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(6400)));
        // Next raw would be 12800ms; capped to 10000ms.
        assert_eq!(b.next_delay(), Some(RECONNECT_MAX));
        assert_eq!(b.next_delay(), Some(RECONNECT_MAX));
        assert_eq!(b.next_delay(), Some(RECONNECT_MAX));
    }

    #[test]
    fn backoff_returns_none_after_max_attempts() {
        let mut b = BackoffState::new();
        for _ in 0..RECONNECT_MAX_ATTEMPTS {
            assert!(b.next_delay().is_some());
        }
        assert_eq!(b.next_delay(), None);
        assert_eq!(b.attempts_used(), RECONNECT_MAX_ATTEMPTS);
    }

    // ----- reporter fixture -----

    #[derive(Default)]
    struct CapturingReporter {
        reports: Mutex<Vec<(String, Vec<u8>, HealthStatus)>>,
        count: AtomicUsize,
    }

    impl CapturingReporter {
        fn count(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
        fn last_payload(&self) -> Option<Vec<u8>> {
            self.reports
                .lock()
                .unwrap()
                .last()
                .map(|(_, p, _)| p.clone())
        }
    }

    impl CustodyStateReporter for CapturingReporter {
        fn report<'a>(
            &'a self,
            handle: &'a CustodyHandle,
            payload: Vec<u8>,
            health: HealthStatus,
        ) -> Pin<
            Box<dyn Future<Output = Result<(), ReportError>> + Send + 'a>,
        > {
            let handle_id = handle.id.clone();
            Box::pin(async move {
                self.reports
                    .lock()
                    .unwrap()
                    .push((handle_id, payload, health));
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    // ----- mock MPD infrastructure -----

    fn short_timeouts() -> ConnectTimeouts {
        ConnectTimeouts {
            connect: Duration::from_millis(500),
            welcome: Duration::from_millis(500),
            command: Duration::from_millis(500),
        }
    }

    /// Behaviour for a single connection to the mock MPD. Each
    /// variant describes how the mock responds to a sequence of
    /// client commands after the welcome banner.
    #[derive(Clone)]
    enum ConnBehaviour {
        /// Generic "MPD is working" handler:
        /// - status => "state: stop\\nOK\\n"
        /// - currentsong => "OK\\n"
        /// - idle => hold forever
        /// - anything else => "OK\\n"
        Standard,
        /// Same as Standard, but the Nth command (1-indexed) gets
        /// an ACK response instead of OK.
        AckOnNth {
            nth: usize,
            code: u32,
            message: String,
        },
        /// Same as Standard, but the Nth command (1-indexed)
        /// causes the connection to close immediately.
        CloseOnNth { nth: usize },
        /// Welcome then hold forever. Used for the idle-side
        /// connection in tests that do not care about idle.
        HoldAfterWelcome,
        /// Welcome, then respond to idle with an immediate
        /// "changed: player\\nOK\\n", then hold.
        IdleOnceThenHold,
    }

    /// Bind a loopback listener and serve each incoming
    /// connection with the behaviour at the matching position in
    /// `behaviours`. The mock tolerates more connections than
    /// behaviours (extras are dropped immediately).
    async fn spawn_mock_mpd(
        behaviours: Vec<ConnBehaviour>,
    ) -> (MpdEndpoint, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let endpoint =
            MpdEndpoint::tcp(addr.ip().to_string(), addr.port()).unwrap();
        let task = tokio::spawn(async move {
            let mut iter = behaviours.into_iter();
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(p) => p,
                    Err(_) => return,
                };
                match iter.next() {
                    Some(b) => {
                        tokio::spawn(serve_connection(stream, b));
                    }
                    None => {
                        drop(stream);
                    }
                }
            }
        });
        (endpoint, task)
    }

    async fn serve_connection(mut stream: TcpStream, b: ConnBehaviour) {
        let (r, mut w) = stream.split();
        let mut reader = BufReader::new(r);

        // Welcome first, always.
        if w.write_all(b"OK MPD 0.23.5\n").await.is_err() {
            return;
        }
        if w.flush().await.is_err() {
            return;
        }

        match b {
            ConnBehaviour::HoldAfterWelcome => {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            ConnBehaviour::IdleOnceThenHold => {
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => return,
                        Ok(_) => {}
                    }
                    if line.starts_with("idle") {
                        let _ = w
                            .write_all(b"changed: player\nOK\n")
                            .await;
                        let _ = w.flush().await;
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        return;
                    }
                    let _ = w.write_all(b"OK\n").await;
                    let _ = w.flush().await;
                }
            }
            ConnBehaviour::Standard
            | ConnBehaviour::AckOnNth { .. }
            | ConnBehaviour::CloseOnNth { .. } => {
                let mut seq: usize = 0;
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => return,
                        Ok(_) => {}
                    }
                    seq += 1;

                    if let ConnBehaviour::AckOnNth {
                        nth,
                        code,
                        ref message,
                    } = b
                    {
                        if seq == nth {
                            let cmd_name =
                                line.split_whitespace().next().unwrap_or("");
                            let ack = format!(
                                "ACK [{}@0] {{{}}} {}\n",
                                code, cmd_name, message
                            );
                            let _ = w.write_all(ack.as_bytes()).await;
                            let _ = w.flush().await;
                            continue;
                        }
                    }
                    if let ConnBehaviour::CloseOnNth { nth } = b {
                        if seq == nth {
                            return;
                        }
                    }

                    if line.starts_with("status") {
                        let _ = w
                            .write_all(b"state: stop\nOK\n")
                            .await;
                    } else if line.starts_with("currentsong") {
                        let _ = w.write_all(b"OK\n").await;
                    } else if line.starts_with("idle") {
                        // Hold forever on idle; no response.
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        return;
                    } else {
                        let _ = w.write_all(b"OK\n").await;
                    }
                    let _ = w.flush().await;
                }
            }
        }
    }

    fn test_custody_handle() -> CustodyHandle {
        CustodyHandle::new("custody-test")
    }

    // ----- integration tests -----

    #[tokio::test]
    async fn spawn_succeeds_and_emits_initial_report() {
        let (endpoint, _mock) = spawn_mock_mpd(vec![
            ConnBehaviour::Standard,
            ConnBehaviour::HoldAfterWelcome,
        ])
        .await;

        let reporter = Arc::new(CapturingReporter::default());
        let reporter_dyn: Arc<dyn CustodyStateReporter> = reporter.clone();

        let handle = spawn(
            endpoint,
            short_timeouts(),
            test_custody_handle(),
            reporter_dyn,
        )
        .await
        .unwrap();

        assert_eq!(reporter.count(), 1);
        let payload = reporter.last_payload().unwrap();
        let text = String::from_utf8(payload).unwrap();
        assert!(
            text.contains("state = \"stopped\""),
            "expected stopped state in report: {text:?}"
        );

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn command_dispatch_returns_ok_and_emits_followup_report() {
        let (endpoint, _mock) = spawn_mock_mpd(vec![
            ConnBehaviour::Standard,
            ConnBehaviour::HoldAfterWelcome,
        ])
        .await;

        let reporter = Arc::new(CapturingReporter::default());
        let reporter_dyn: Arc<dyn CustodyStateReporter> = reporter.clone();

        let handle = spawn(
            endpoint,
            short_timeouts(),
            test_custody_handle(),
            reporter_dyn,
        )
        .await
        .unwrap();

        // Initial report is already in.
        assert_eq!(reporter.count(), 1);

        handle.command(PlaybackCommand::Play).await.unwrap();

        // After the command, wait briefly for the follow-up
        // report to land.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            reporter.count(),
            2,
            "expected initial + post-command report, got {}",
            reporter.count()
        );

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn command_ack_returns_playback_error_ack() {
        // Command-conn: 1 = status (initial report),
        //               2 = currentsong (initial report),
        //               3 = play -> ACK.
        let (endpoint, _mock) = spawn_mock_mpd(vec![
            ConnBehaviour::AckOnNth {
                nth: 3,
                code: 2,
                message: "Bad song index".to_string(),
            },
            ConnBehaviour::HoldAfterWelcome,
        ])
        .await;

        let reporter = Arc::new(CapturingReporter::default());
        let reporter_dyn: Arc<dyn CustodyStateReporter> = reporter.clone();

        let handle = spawn(
            endpoint,
            short_timeouts(),
            test_custody_handle(),
            reporter_dyn,
        )
        .await
        .unwrap();

        let err = handle
            .command(PlaybackCommand::Play)
            .await
            .unwrap_err();
        match err {
            PlaybackError::Ack { code, message } => {
                assert_eq!(code, 2);
                assert_eq!(message, "Bad song index");
            }
            other => panic!("expected Ack, got {other:?}"),
        }

        // ACK does not kill the supervisor; shutdown still works.
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn command_reconnects_after_transient_drop() {
        // First command-conn: initial status (seq 1),
        //                     initial currentsong (seq 2),
        //                     play (seq 3) -> close connection.
        // Second command-conn (reconnect): Standard -> OK on play.
        // Idle conn: hold.
        let (endpoint, _mock) = spawn_mock_mpd(vec![
            ConnBehaviour::CloseOnNth { nth: 3 },
            ConnBehaviour::HoldAfterWelcome,
            ConnBehaviour::Standard,
        ])
        .await;

        let reporter = Arc::new(CapturingReporter::default());
        let reporter_dyn: Arc<dyn CustodyStateReporter> = reporter.clone();

        let handle = spawn(
            endpoint,
            short_timeouts(),
            test_custody_handle(),
            reporter_dyn,
        )
        .await
        .unwrap();

        // play fails the first time (conn closes), the supervisor
        // reconnects, retries on the new connection, succeeds.
        handle.command(PlaybackCommand::Play).await.unwrap();

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_completes_promptly() {
        let (endpoint, _mock) = spawn_mock_mpd(vec![
            ConnBehaviour::Standard,
            ConnBehaviour::HoldAfterWelcome,
        ])
        .await;

        let reporter = Arc::new(CapturingReporter::default());
        let reporter_dyn: Arc<dyn CustodyStateReporter> = reporter.clone();

        let handle = spawn(
            endpoint,
            short_timeouts(),
            test_custody_handle(),
            reporter_dyn,
        )
        .await
        .unwrap();

        let start = std::time::Instant::now();
        handle.shutdown().await;
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "shutdown took too long: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn idle_event_triggers_extra_state_report() {
        let (endpoint, _mock) = spawn_mock_mpd(vec![
            ConnBehaviour::Standard,
            ConnBehaviour::IdleOnceThenHold,
        ])
        .await;

        let reporter = Arc::new(CapturingReporter::default());
        let reporter_dyn: Arc<dyn CustodyStateReporter> = reporter.clone();

        let handle = spawn(
            endpoint,
            short_timeouts(),
            test_custody_handle(),
            reporter_dyn,
        )
        .await
        .unwrap();

        // The mock's idle connection responds with a single
        // `changed: player` event; the supervisor should emit a
        // follow-up report in response.
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert!(
            reporter.count() >= 2,
            "expected >= 2 reports (initial + idle-triggered), got {}",
            reporter.count()
        );

        handle.shutdown().await;
    }
}
