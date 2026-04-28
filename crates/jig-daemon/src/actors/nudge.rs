//! Nudge actor — delivers nudge messages to workers in a background thread.

use jig_core::prompt::Prompt;
use jig_core::worker::Worker;

use crate::actors::Actor;

pub struct NudgeRequest {
    pub worker: Worker,
    pub prompt: Prompt,
}

pub struct NudgeComplete {
    // TODO (draft): this return type sucks,
    //  what good is worker_key? can't we be more 
    //  specific?
    pub worker_key: String,
    // same with error
    pub error: Option<String>,
}

pub struct NudgeActor {
    tx: flume::Sender<NudgeRequest>,
    rx: flume::Receiver<NudgeComplete>,
}

impl Actor for NudgeActor {
    type Request = NudgeRequest;
    type Response = NudgeComplete;

    const NAME: &'static str = "jig-nudge";
    const QUEUE_SIZE: usize = 16;

    fn handle(req: NudgeRequest) -> NudgeComplete {
        let worker_key = format!("{}", req.worker.branch());
        let error = match req.worker.nudge(req.prompt) {
            Ok(()) => {
                tracing::info!(worker = %worker_key, "nudge delivered");
                None
            }
            // TODO (draft): this is dumb, we get more intelligent
            //  errors out of this
            Err(e) => {
                tracing::warn!(worker = %worker_key, "nudge delivery failed: {}", e);
                Some(e.to_string())
            }
        };

        NudgeComplete { worker_key, error }
    }

    fn send(&mut self, req: NudgeRequest) -> bool {
        self.tx.try_send(req).is_ok()
    }

    fn drain(&mut self) -> Vec<NudgeComplete> {
        let mut results = Vec::new();
        while let Ok(resp) = self.rx.try_recv() {
            results.push(resp);
        }
        results
    }

    fn from_channels(
        tx: flume::Sender<NudgeRequest>,
        rx: flume::Receiver<NudgeComplete>,
    ) -> Self {
        Self { tx, rx }
    }
}
