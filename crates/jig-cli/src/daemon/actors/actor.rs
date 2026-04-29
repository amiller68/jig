//! Actor trait and generic handle for daemon background workers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The actor struct IS the state. Define your struct, implement this trait,
/// and `ActorHandle<A>` manages channels, threading, and pending tracking.
///
/// Use interior mutability (`Mutex`, `AtomicBool`, etc.) for fields that
/// need mutation — `handle(&self)` takes a shared reference because it runs
/// on the background thread while the main thread may read actor state.
pub trait Actor: Default + Send + Sync + 'static {
    type Request: Send + 'static;
    type Response: Send + 'static;

    const NAME: &'static str;
    const QUEUE_SIZE: usize;

    fn handle(&self, req: Self::Request) -> Self::Response;
}

/// Generic wrapper that owns channels, the background thread, and a shared
/// reference to the actor. The main thread uses `send()` for fire-and-forget
/// dispatch, `drain()` to collect responses, and `.actor()` to read actor
/// state directly.
pub struct ActorHandle<A: Actor> {
    tx: flume::Sender<A::Request>,
    rx: flume::Receiver<A::Response>,
    inner: Arc<A>,
    pending: Arc<AtomicBool>,
    _handle: std::thread::JoinHandle<()>,
}

impl<A: Actor> ActorHandle<A> {
    pub fn new() -> Self {
        let (req_tx, req_rx) = flume::bounded::<A::Request>(A::QUEUE_SIZE);
        let (resp_tx, resp_rx) = flume::bounded::<A::Response>(A::QUEUE_SIZE);
        let inner = Arc::new(A::default());
        let bg = Arc::clone(&inner);
        let pending = Arc::new(AtomicBool::new(false));
        let bg_pending = Arc::clone(&pending);
        let handle = std::thread::Builder::new()
            .name(A::NAME.into())
            .spawn(move || {
                while let Ok(req) = req_rx.recv() {
                    let resp = bg.handle(req);
                    bg_pending.store(false, Ordering::Relaxed);
                    if resp_tx.send(resp).is_err() {
                        break;
                    }
                }
            })
            .unwrap_or_else(|e| panic!("failed to spawn {} thread: {}", A::NAME, e));

        Self {
            tx: req_tx,
            rx: resp_rx,
            inner,
            pending,
            _handle: handle,
        }
    }

    pub fn send(&self, req: A::Request) -> bool {
        if self.pending.load(Ordering::Relaxed) {
            return false;
        }
        if self.tx.try_send(req).is_ok() {
            self.pending.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn drain(&self) -> Vec<A::Response> {
        let mut results = Vec::new();
        while let Ok(resp) = self.rx.try_recv() {
            results.push(resp);
        }
        results
    }

    pub fn is_pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed)
    }

    pub fn actor(&self) -> &A {
        &self.inner
    }
}
