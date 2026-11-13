//! Blocking runtime coordinator. Algorithms live in the existing modules.
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::sync::mpsc::{error::TryRecvError, Receiver};

use crate::audio_capture::AudioChunk;
use crate::core::error::{AppError, AppResult};

pub trait CaptureControl: Send {
    fn request_stop(&self);
}

pub struct CaptureStream {
    pub receiver: Receiver<AudioChunk>,
    pub control: Box<dyn CaptureControl>,
}

pub trait Backend: Send + 'static {
    fn capture(&mut self) -> AppResult<CaptureStream>;
    fn process(&mut self, chunk: AudioChunk) -> AppResult<()>;
    fn finish(&mut self) -> AppResult<()>;
    fn failed(&mut self, error: &AppError);
}

#[derive(Clone, Copy)]
enum Action {
    Pause,
    Resume,
    Stop,
}
struct Request {
    action: Action,
    reply: mpsc::SyncSender<AppResult<()>>,
}

pub struct Runtime {
    requests: mpsc::Sender<Request>,
    worker: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Runtime")
    }
}

fn worker_error() -> AppError {
    AppError::Internal("Audio runtime worker exited".into())
}

impl Runtime {
    /// Initialization, model loading and all inference run on this dedicated
    /// thread, never on Tokio's executor. Factory errors reach the caller.
    pub fn start(
        factory: impl FnOnce() -> AppResult<Box<dyn Backend>> + Send + 'static,
    ) -> AppResult<Self> {
        let (requests, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("voxy-pipeline".into())
            .spawn(move || {
                let init = factory().and_then(|mut backend| {
                    let stream = backend.capture()?;
                    Ok((backend, stream))
                });
                match init {
                    Ok((mut backend, stream)) => {
                        if ready_tx.send(Ok(())).is_ok() {
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    run(backend.as_mut(), Some(stream), rx)
                                }));
                            if result.is_err() {
                                backend
                                    .failed(&AppError::Internal("Audio runtime panicked".into()));
                            }
                        } else {
                            let _ = drain(backend.as_mut(), stream, true);
                        }
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
            })
            .map_err(|error| AppError::Internal(error.to_string()))?;
        match ready_rx.recv().map_err(|_| worker_error())? {
            Ok(()) => Ok(Self {
                requests,
                worker: Some(worker),
            }),
            Err(error) => {
                let _ = worker.join();
                Err(error)
            }
        }
    }

    fn request(&self, action: Action) -> AppResult<()> {
        let (reply, rx) = mpsc::sync_channel(1);
        self.requests
            .send(Request { action, reply })
            .map_err(|_| worker_error())?;
        rx.recv().map_err(|_| worker_error())?
    }
    pub fn pause(&self) -> AppResult<()> {
        self.request(Action::Pause)
    }
    pub fn resume(&self) -> AppResult<()> {
        self.request(Action::Resume)
    }
    pub fn shutdown(mut self) -> AppResult<()> {
        let result = self.request(Action::Stop);
        let joined = self
            .worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| worker_error());
        result.and(joined)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // Nonblocking cancellation: the worker owns capture, drains it and joins
        // its producer. Explicit shutdown additionally awaits this worker.
        if self.worker.is_some() {
            let (reply, _) = mpsc::sync_channel(1);
            let _ = self.requests.send(Request {
                action: Action::Stop,
                reply,
            });
        }
    }
}

/// Signal producers first. Continue consuming while blocking_send may be
/// pending. Only drop/join their handles once all senders have gone away.
fn drain(backend: &mut dyn Backend, mut stream: CaptureStream, finish: bool) -> AppResult<()> {
    stream.control.request_stop();
    let mut result = Ok(());
    while let Some(chunk) = stream.receiver.blocking_recv() {
        if result.is_ok() {
            result = backend.process(chunk);
        }
    }
    drop(stream);
    if finish {
        let flushed = backend.finish();
        result = result.and(flushed);
    }
    result
}

fn run(backend: &mut dyn Backend, mut stream: Option<CaptureStream>, rx: mpsc::Receiver<Request>) {
    loop {
        let request = if stream.is_none() {
            // Paused runtimes wait for control instead of polling or advancing audio.
            rx.recv().map_err(|_| mpsc::TryRecvError::Disconnected)
        } else {
            rx.try_recv()
        };
        match request {
            Ok(request) => {
                let stopping = matches!(request.action, Action::Stop);
                let result = match request.action {
                    Action::Pause | Action::Stop => {
                        let result = if let Some(active) = stream.take() {
                            drain(backend, active, stopping)
                        } else if stopping {
                            backend.finish()
                        } else {
                            Err(AppError::InvalidConfig("Runtime is already paused".into()))
                        };
                        result
                    }
                    Action::Resume => {
                        if stream.is_some() {
                            Err(AppError::InvalidConfig("Runtime is already active".into()))
                        } else {
                            backend.capture().map(|active| {
                                stream = Some(active);
                            })
                        }
                    }
                };
                // Resume failures leave a paused runtime usable for retry/stop.
                let terminal = result.is_err() && !matches!(request.action, Action::Resume);
                if terminal {
                    if !stopping {
                        let _ = backend.finish();
                    }
                    backend.failed(result.as_ref().unwrap_err());
                }
                let _ = request.reply.send(result);
                if stopping || terminal {
                    break;
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some(active) = stream.take() {
                    let _ = drain(backend, active, true);
                } else {
                    let _ = backend.finish();
                }
                break;
            }
            Err(mpsc::TryRecvError::Empty) => {
                if let Some(active) = stream.as_mut() {
                    let result = match active.receiver.try_recv() {
                        Ok(chunk) => backend.process(chunk),
                        Err(TryRecvError::Empty) => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(TryRecvError::Disconnected) => Err(AppError::AudioCaptureFailed(
                            "Capture stream ended unexpectedly".into(),
                        )),
                    };
                    if let Err(error) = result {
                        let _ = drain(backend, stream.take().unwrap(), true);
                        backend.failed(&error);
                        break;
                    }
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    include!("runtime/tests.rs");
}
