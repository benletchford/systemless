//! Unix JSON-line transport for [`systemless::debug`]. The library module
//! documents envelopes and client usage; request types document RPC parameters.
//! This worker owns socket I/O only. The frontend must pump requests between
//! runner calls, including while paused or halted.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use systemless::debug::{handle_debug_request, DebugError, DebugReply, DebugRequest};
use systemless::runner::FixtureRunner;

const MAX_ENVELOPE_BYTES: usize = 1024 * 1024;

// Socket I/O stays on the worker thread; requests cross to the runner thread
// and are applied only when the frontend pumps its command-safe point.

struct Envelope {
    id: u64,
    request: DebugRequest,
}

#[derive(Deserialize)]
struct EnvelopeIn {
    #[serde(default)]
    id: u64,
    request: DebugRequest,
}

#[derive(Serialize)]
struct EnvelopeOut {
    id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply: Option<DebugReply>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<DebugError>,
}

pub struct DebugServer {
    to_runner: Receiver<Envelope>,
    from_runner: Sender<Outgoing>,
}

struct Outgoing {
    id: u64,
    result: Result<DebugReply, DebugError>,
}

impl DebugServer {
    pub fn bind(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        if path.exists() {
            if std::fs::symlink_metadata(path)?.file_type().is_socket() {
                std::fs::remove_file(path)?;
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("debug socket path {} is occupied", path.display()),
                ));
            }
        }
        let listener = UnixListener::bind(path)?;
        let (to_runner_tx, to_runner) = std::sync::mpsc::channel();
        let (from_runner, from_runner_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("systemless-debug".to_string())
            .spawn(move || accept_loop(listener, to_runner_tx, from_runner_rx))?;
        eprintln!("[DEBUG] Listening on {}", path.display());
        Ok(Self {
            to_runner,
            from_runner,
        })
    }

    pub fn pump(&mut self, runner: &mut FixtureRunner) -> usize {
        let mut served = 0;
        while let Ok(envelope) = self.to_runner.try_recv() {
            served += 1;
            let _ = self.from_runner.send(Outgoing {
                id: envelope.id,
                result: handle_debug_request(runner, envelope.request),
            });
        }
        served
    }
}

// One controlling client at a time. Accepting stays on this thread so a second
// connection is refused with a typed error instead of blocking in the kernel
// accept queue with no indication of why it is stalled; the served client runs
// on its own thread. `busy` is the authority on exclusivity, so the reply
// channel is never contended.
fn accept_loop(
    listener: UnixListener,
    to_runner: Sender<Envelope>,
    from_runner: Receiver<Outgoing>,
) {
    let busy = Arc::new(AtomicBool::new(false));
    // Receiver is not Sync, so sharing it with the client thread needs a Mutex.
    // The lock is uncontended in practice: `busy` guarantees at most one client
    // holds it, for that client's whole connection.
    let from_runner = Arc::new(Mutex::new(from_runner));
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        if busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            refuse_client(stream);
            continue;
        }
        let to_runner = to_runner.clone();
        let from_runner = Arc::clone(&from_runner);
        let client_busy = Arc::clone(&busy);
        let served = std::thread::Builder::new()
            .name("systemless-debug-client".to_string())
            .spawn(move || {
                {
                    let from_runner = from_runner
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if let Err(error) = handle_client(stream, &to_runner, &from_runner) {
                        eprintln!("[DEBUG] Client disconnected: {error}");
                    }
                }
                client_busy.store(false, Ordering::SeqCst);
            });
        if served.is_err() {
            busy.store(false, Ordering::SeqCst);
            eprintln!("[DEBUG] Cannot spawn a client thread; connection dropped");
        }
    }
}

// The refused connection gets the same envelope shape as any other failure, so
// a client parsing replies sees the reason rather than an unexplained close.
fn refuse_client(stream: UnixStream) {
    eprintln!("[DEBUG] Refused a second client: one controlling client at a time");
    let mut stream = stream;
    let _ = write_envelope(
        &EnvelopeOut {
            id: 0,
            reply: None,
            error: Some(DebugError::InvalidState {
                detail: "a controlling client is already connected; \
                         the debug socket serves one client at a time"
                    .to_string(),
            }),
        },
        &mut stream,
    );
}

fn handle_client(
    stream: UnixStream,
    to_runner: &Sender<Envelope>,
    from_runner: &Receiver<Outgoing>,
) -> std::io::Result<()> {
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    while let Some(line) = read_capped_line(&mut reader)? {
        let line = match line {
            Ok(line) => line,
            Err(()) => {
                write_envelope(
                    &EnvelopeOut {
                        id: 0,
                        reply: None,
                        error: Some(DebugError::TooLarge {
                            limit: MAX_ENVELOPE_BYTES as u64,
                            requested: MAX_ENVELOPE_BYTES as u64 + 1,
                        }),
                    },
                    &mut writer,
                )?;
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let input: EnvelopeIn = match serde_json::from_str(&line) {
            Ok(input) => input,
            Err(error) => {
                let out = EnvelopeOut {
                    id: 0,
                    reply: None,
                    error: Some(DebugError::InvalidValue {
                        detail: format!("malformed request envelope: {error}"),
                    }),
                };
                write_envelope(&out, &mut writer)?;
                continue;
            }
        };
        let id = input.id;
        if to_runner
            .send(Envelope {
                id,
                request: input.request,
            })
            .is_err()
        {
            break;
        }
        let Ok(outgoing) = from_runner.recv() else {
            break;
        };
        let out = EnvelopeOut {
            id: outgoing.id,
            reply: outgoing.result.as_ref().ok().cloned(),
            error: outgoing.result.as_ref().err().cloned(),
        };
        write_envelope(&out, &mut writer)?;
    }
    Ok(())
}

fn read_capped_line(reader: &mut impl BufRead) -> std::io::Result<Option<Result<String, ()>>> {
    let mut bytes = Vec::new();
    let mut too_large = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if bytes.is_empty() && !too_large {
                return Ok(None);
            }
            break;
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if !too_large {
            let remaining = MAX_ENVELOPE_BYTES.saturating_sub(bytes.len());
            bytes.extend_from_slice(&available[..consumed.min(remaining)]);
            too_large = consumed > remaining;
        }
        let ended = available[consumed - 1] == b'\n';
        reader.consume(consumed);
        if ended {
            break;
        }
    }
    if too_large {
        return Ok(Some(Err(())));
    }
    String::from_utf8(bytes)
        .map(|line| Some(Ok(line)))
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn write_envelope(out: &EnvelopeOut, writer: &mut impl Write) -> std::io::Result<()> {
    let text = serde_json::to_string(out).unwrap_or_else(|error| {
        serde_json::json!({
            "id": out.id,
            "error": {
                "error": "internal",
                "detail": format!("reply serialization failed: {error}"),
            }
        })
        .to_string()
    });
    writeln!(writer, "{text}")?;
    writer.flush()
}
