use super::*;
use crate::audio_capture::types::AudioSource;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

#[derive(Default)]
struct Counts {
    sent: AtomicUsize,
    processed: AtomicUsize,
    finished: AtomicUsize,
    failed: AtomicUsize,
    starts: AtomicUsize,
}
struct Producer {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl CaptureControl for Producer {
    fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}
impl Drop for Producer {
    fn drop(&mut self) {
        self.request_stop();
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
struct TestBackend {
    counts: Arc<Counts>,
    fail_process: bool,
    fail_resume: bool,
}
impl Backend for TestBackend {
    fn capture(&mut self) -> AppResult<CaptureStream> {
        let start = self.counts.starts.fetch_add(1, Ordering::SeqCst);
        if self.fail_resume && start > 0 {
            return Err(AppError::AudioCaptureFailed("resume failed".into()));
        }
        let (tx, receiver) = tokio::sync::mpsc::channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let producer_stop = stop.clone();
        let counts = self.counts.clone();
        let worker = thread::spawn(move || {
            while !producer_stop.load(Ordering::SeqCst) {
                let chunk = AudioChunk::new(vec![0.25; 16], 16_000, 0, AudioSource::Loopback);
                if tx.blocking_send(chunk).is_err() {
                    return;
                }
                counts.sent.fetch_add(1, Ordering::SeqCst);
            }
            // A final tail must be drained before the producer is joined.
            if tx
                .blocking_send(AudioChunk::new(
                    vec![0.25],
                    16_000,
                    0,
                    AudioSource::Loopback,
                ))
                .is_ok()
            {
                counts.sent.fetch_add(1, Ordering::SeqCst);
            }
        });
        Ok(CaptureStream {
            receiver,
            control: Box::new(Producer {
                stop,
                worker: Some(worker),
            }),
        })
    }
    fn process(&mut self, _: AudioChunk) -> AppResult<()> {
        self.counts.processed.fetch_add(1, Ordering::SeqCst);
        if self.fail_process {
            Err(AppError::SpeechRecognitionError("inference failed".into()))
        } else {
            Ok(())
        }
    }
    fn finish(&mut self) -> AppResult<()> {
        self.counts.finished.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn failed(&mut self, _: &AppError) {
        self.counts.failed.fetch_add(1, Ordering::SeqCst);
    }
}
fn with_deadline(test: impl FnOnce() + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));
        let _ = tx.send(result);
    });
    rx.recv_timeout(Duration::from_secs(5))
        .expect("runtime deadlock")
        .unwrap();
}

#[test]
fn full_channel_pause_resume_and_shutdown_drain_before_join() {
    with_deadline(|| {
        let counts = Arc::new(Counts::default());
        let factory_counts = counts.clone();
        let runtime = Runtime::start(move || {
            Ok(Box::new(TestBackend {
                counts: factory_counts,
                fail_process: false,
                fail_resume: false,
            }))
        })
        .unwrap();
        runtime.pause().unwrap();
        assert_eq!(
            counts.sent.load(Ordering::SeqCst),
            counts.processed.load(Ordering::SeqCst)
        );
        assert_eq!(counts.finished.load(Ordering::SeqCst), 0);
        runtime.resume().unwrap();
        runtime.shutdown().unwrap();
        assert_eq!(counts.starts.load(Ordering::SeqCst), 2);
        assert_eq!(
            counts.sent.load(Ordering::SeqCst),
            counts.processed.load(Ordering::SeqCst)
        );
        assert_eq!(counts.finished.load(Ordering::SeqCst), 1);
    });
}
#[test]
fn failed_resume_leaves_paused_runtime_stoppable() {
    with_deadline(|| {
        let counts = Arc::new(Counts::default());
        let factory_counts = counts.clone();
        let runtime = Runtime::start(move || {
            Ok(Box::new(TestBackend {
                counts: factory_counts,
                fail_process: false,
                fail_resume: true,
            }))
        })
        .unwrap();
        runtime.pause().unwrap();
        assert!(runtime
            .resume()
            .unwrap_err()
            .to_string()
            .contains("resume failed"));
        runtime.shutdown().unwrap();
        assert_eq!(counts.finished.load(Ordering::SeqCst), 1);
    });
}
#[test]
fn startup_failure_is_reported_without_live_worker() {
    assert!(
        Runtime::start(|| Err(AppError::ModelNotFound("missing.bin".into())))
            .unwrap_err()
            .to_string()
            .contains("missing.bin")
    );
}
#[test]
fn processing_failure_closes_full_capture_channel_and_reports_error() {
    with_deadline(|| {
        let counts = Arc::new(Counts::default());
        let factory_counts = counts.clone();
        let runtime = Runtime::start(move || {
            Ok(Box::new(TestBackend {
                counts: factory_counts,
                fail_process: true,
                fail_resume: false,
            }))
        })
        .unwrap();
        // Either the stop request or the queued chunk observes the processing failure.
        assert!(runtime.shutdown().is_err());
        assert_eq!(counts.failed.load(Ordering::SeqCst), 1);
    });
}

struct FatalBackend {
    inner: TestBackend,
    panic: bool,
    report: mpsc::Sender<String>,
}
impl Backend for FatalBackend {
    fn capture(&mut self) -> AppResult<CaptureStream> {
        if self.panic {
            self.inner.capture()
        } else {
            let (tx, receiver) = tokio::sync::mpsc::channel(1);
            drop(tx);
            struct Empty;
            impl CaptureControl for Empty {
                fn request_stop(&self) {}
            }
            Ok(CaptureStream {
                receiver,
                control: Box::new(Empty),
            })
        }
    }
    fn process(&mut self, _: AudioChunk) -> AppResult<()> {
        panic!("test-only inference panic")
    }
    fn finish(&mut self) -> AppResult<()> {
        self.inner.finish()
    }
    fn failed(&mut self, error: &AppError) {
        let _ = self.report.send(error.to_string());
    }
}
#[test]
fn unexpected_capture_end_is_reported_and_worker_is_joinable() {
    with_deadline(|| {
        let (report, rx) = mpsc::channel();
        let runtime = Runtime::start(move || {
            Ok(Box::new(FatalBackend {
                inner: TestBackend {
                    counts: Arc::new(Counts::default()),
                    fail_process: false,
                    fail_resume: false,
                },
                panic: false,
                report,
            }))
        })
        .unwrap();
        assert!(rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .contains("unexpectedly"));
        assert!(runtime.shutdown().is_err());
    });
}
#[test]
fn worker_panic_releases_capture_and_reports_error() {
    with_deadline(|| {
        let (report, rx) = mpsc::channel();
        let runtime = Runtime::start(move || {
            Ok(Box::new(FatalBackend {
                inner: TestBackend {
                    counts: Arc::new(Counts::default()),
                    fail_process: false,
                    fail_resume: false,
                },
                panic: true,
                report,
            }))
        })
        .unwrap();
        assert!(rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .contains("panicked"));
        assert!(runtime.shutdown().is_err());
    });
}
