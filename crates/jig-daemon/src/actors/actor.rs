//! Actor trait — defines the contract for daemon background workers.

/// A background worker that processes requests on a dedicated thread.
///
/// Each actor owns its channel pair and any internal state (caches, timers,
/// pending flags). The daemon calls `send()` to enqueue work and `drain()`
/// to collect results each tick.
pub trait Actor: Sized {
    type Request: Send + 'static;
    type Response: Send + 'static;

    const NAME: &'static str;
    const QUEUE_SIZE: usize;

    /// Process a single request and produce a response.
    /// Runs on the background thread.
    fn handle(req: Self::Request) -> Self::Response;

    /// Try to enqueue a request (non-blocking).
    fn send(&mut self, req: Self::Request) -> bool;

    /// Drain all completed responses (non-blocking).
    fn drain(&mut self) -> Vec<Self::Response>;

    /// Construct the actor from pre-built channels.
    /// Implement this instead of `new()` — the thread spawn is handled for you.
    fn from_channels(
        tx: flume::Sender<Self::Request>,
        rx: flume::Receiver<Self::Response>,
    ) -> Self;

    /// Spawn the background thread and return (actor, join_handle).
    /// Default implementation creates bounded channels, spawns the recv loop,
    /// and calls `from_channels`.
    fn new() -> (Self, std::thread::JoinHandle<()>) {
        let (req_tx, req_rx) = flume::bounded(Self::QUEUE_SIZE);
        let (resp_tx, resp_rx) = flume::bounded(Self::QUEUE_SIZE);
        let handle = std::thread::Builder::new()
            .name(Self::NAME.into())
            .spawn(move || {
                while let Ok(req) = req_rx.recv() {
                    let resp = Self::handle(req);
                    if resp_tx.send(resp).is_err() {
                        break;
                    }
                }
            })
            .unwrap_or_else(|e| panic!("failed to spawn {} thread: {}", Self::NAME, e));

        (Self::from_channels(req_tx, resp_rx), handle)
    }
}
