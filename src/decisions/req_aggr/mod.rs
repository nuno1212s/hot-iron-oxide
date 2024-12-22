use atlas_common::channel::RecvError;
use atlas_common::serialization_helper::SerType;
use atlas_core::messages::{RequestMessage, StoredRequestMessage};
use atlas_core::request_pre_processing::{BatchOutput};
use std::sync::{Arc, Mutex};
use tracing::error;

pub struct RequestAggr<RQ> {
    pre_processor_output: BatchOutput<RequestMessage<RQ>>,
    current_pool: Mutex<Vec<StoredRequestMessage<RQ>>>,
}

impl<RQ> RequestAggr<RQ> {
    pub fn new(pre_processor_output: BatchOutput<RequestMessage<RQ>>) -> Arc<Self>
    where
        RQ: SerType,
    {
        let arc = Arc::new(Self {
            pre_processor_output,
            current_pool: Mutex::new(Vec::new()),
        });

        std::thread::spawn({
            let self_clone = arc.clone();
            move || {
                self_clone.run();
            }
        });

        arc
    }

    fn run(&self) {
        loop {
            let result = {
                let mut current_result;

                match self.pre_processor_output.recv() {
                    Ok(res) => {
                        current_result = res;
                    }
                    Err(err) => match err {
                        RecvError::ChannelDc => {
                            error!("Batch output channel has been closed, shutting down request aggregator");
                            break;
                        }
                    },
                };

                while let Ok(res) = self.pre_processor_output.try_recv() {
                    current_result.append(&mut res.into());
                }

                current_result
            };

            let mut pool_guard = self.current_pool.lock().unwrap();

            pool_guard.append(&mut result.into())
        }
    }

    pub fn take_pool_requests(&self) -> Vec<StoredRequestMessage<RQ>> {
        let mut guard = self.current_pool.lock().unwrap();

        std::mem::take(&mut guard)
    }
}
