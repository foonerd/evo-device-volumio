//! Shared test fixtures for the `playback_supervisor` module and
//! its consumers.
//!
//! Kept `#[cfg(test)]` so it is only compiled during test builds
//! and does not inflate the release binary. Visibility is
//! `pub(crate)` so `lib.rs` tests can import these fixtures in
//! addition to `actor.rs` tests; keeping one copy of the mock
//! avoids drift between the integration tests for the supervisor
//! itself and the integration tests for the warden that wraps it.

#![cfg(test)]

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use evo_plugin_sdk::contract::{
    CustodyHandle, CustodyStateReporter, HealthStatus, ReportError,
};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::mpd::{ConnectTimeouts, MpdEndpoint};

// ----- timeouts and handles -----

/// Short timeouts suitable for tests. Generous enough to tolerate
/// a loaded CI machine; tight enough that a test against an
/// unresponsive mock fails in well under a second.
pub(crate) fn short_timeouts() -> ConnectTimeouts {
    ConnectTimeouts {
        connect: Duration::from_millis(500),
        welcome: Duration::from_millis(500),
        command: Duration::from_millis(500),
    }
}

/// A deterministic [`CustodyHandle`] for tests that do not care
/// about handle identity.
pub(crate) fn test_custody_handle() -> CustodyHandle {
    CustodyHandle::new("custody-test")
}

// ----- capturing reporter -----

/// Reporter that records every `report()` invocation. Used by
/// both `actor.rs` and `lib.rs` tests to assert on initial and
/// follow-up state reports.
#[derive(Default)]
pub(crate) struct CapturingReporter {
    reports: Mutex<Vec<(CustodyHandle, Vec<u8>, HealthStatus)>>,
    count: AtomicUsize,
}

impl CapturingReporter {
    pub(crate) fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    /// Full record of the most recent report, if any.
    pub(crate) fn last(
        &self,
    ) -> Option<(CustodyHandle, Vec<u8>, HealthStatus)> {
        self.reports.lock().unwrap().last().cloned()
    }

    /// Convenience: the payload of the most recent report.
    pub(crate) fn last_payload(&self) -> Option<Vec<u8>> {
        self.last().map(|(_, p, _)| p)
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
        let handle = handle.clone();
        Box::pin(async move {
            self.reports
                .lock()
                .unwrap()
                .push((handle, payload, health));
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

// ----- TCP mock MPD -----

/// Behaviour for a single connection accepted by the mock.
///
/// Variants describe what the mock does after sending its welcome
/// banner. Each connection consumes one variant in the order the
/// mock was configured; extras are dropped.
#[derive(Clone)]
pub(crate) enum ConnBehaviour {
    /// Generic "MPD is working" handler:
    /// - `status`     => `state: stop\nOK\n`
    /// - `currentsong`=> `OK\n` (empty current song)
    /// - `idle`       => hold without response
    /// - anything else=> `OK\n`
    Standard,
    /// Same as [`Standard`] but the Nth command (1-indexed) is
    /// met with an ACK reply instead of OK.
    ///
    /// [`Standard`]: Self::Standard
    AckOnNth {
        nth: usize,
        code: u32,
        message: String,
    },
    /// Same as [`Standard`] but the Nth command (1-indexed)
    /// causes the connection to close without replying.
    ///
    /// [`Standard`]: Self::Standard
    CloseOnNth { nth: usize },
    /// Welcome then silence. Useful for idle-side connection
    /// slots when the test does not exercise idle.
    HoldAfterWelcome,
    /// Welcome, then respond to the first `idle` command with
    /// `changed: player\nOK\n`, then hold.
    IdleOnceThenHold,
}

/// Bind a loopback listener and serve incoming connections with
/// the supplied behaviours, in order. Extra connections beyond
/// the end of `behaviours` are dropped on accept.
///
/// Returns the endpoint to hand to the supervisor (or the warden)
/// plus the listener task's `JoinHandle`. Dropping the handle
/// does not close the listener; the listener lives until the
/// tokio runtime shuts down.
pub(crate) async fn spawn_mock_mpd(
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

/// The "nothing responds" variant: binds a listener but never
/// sends a welcome. Useful for tests that need the supervisor's
/// connect / welcome path to fail. Connections accepted are held
/// open silently until the listener drops.
pub(crate) async fn spawn_unresponsive_mock() -> (MpdEndpoint, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint =
        MpdEndpoint::tcp(addr.ip().to_string(), addr.port()).unwrap();
    let task = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(async move {
                        // Hold but do not write the welcome; the
                        // supervisor's welcome timeout fires.
                        tokio::time::sleep(Duration::from_secs(60))
                            .await;
                        drop(stream);
                    });
                }
                Err(_) => return,
            }
        }
    });
    (endpoint, task)
}

async fn serve_connection(mut stream: TcpStream, b: ConnBehaviour) {
    let (r, mut w) = stream.split();
    let mut reader = BufReader::new(r);

    // Welcome first, unconditionally.
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
                    let _ =
                        w.write_all(b"changed: player\nOK\n").await;
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
                    let _ = w.write_all(b"state: stop\nOK\n").await;
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
