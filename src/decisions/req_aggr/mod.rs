use atlas_common::channel::RecvError;
use atlas_common::crypto::hash::{Context, Digest};
use atlas_common::serialization_helper::SerMsg;
use atlas_communication::message::{Header, StoredMessage};
use atlas_core::messages::{RequestMessage, StoredRequestMessage};
use atlas_core::request_pre_processing::BatchOutput;
use std::sync::{Arc, Mutex};
use tracing::error;

///
/// The aggregator for requests
///
pub trait ReqAggregator<RQ>: Send + Sync {
    fn take_pool_requests(&self) -> (Vec<StoredMessage<RQ>>, Digest);
}

pub struct RequestAggr<RQ> {
    pre_processor_output: BatchOutput<RQ>,
    current_pool: Mutex<(Vec<StoredMessage<RQ>>, Digest)>,
}

impl<RQ> RequestAggr<RQ> {
    pub fn new(pre_processor_output: BatchOutput<RQ>) -> Arc<Self>
    where
        RQ: SerMsg,
    {
        let arc = Arc::new(Self {
            pre_processor_output,
            current_pool: Mutex::new((Vec::default(), Digest::default())),
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
            pool_guard.0.append(&mut result.into());

            let digest = Self::calculate_digest_for(&pool_guard.0);
            pool_guard.1 = digest;
        }
    }

    fn calculate_digest_for(stored: &[StoredMessage<RQ>]) -> Digest {
        let digest = stored
            .iter()
            .map(StoredMessage::header)
            .map(Header::digest)
            .fold(Context::new(), |mut context, digest| {
                context.update(digest.as_ref());

                context
            })
            .finish();

        digest
    }
}

impl<RQ> ReqAggregator<RQ> for RequestAggr<RQ>
where
    RQ: Send + Sync,
{
    fn take_pool_requests(&self) -> (Vec<StoredMessage<RQ>>, Digest) {
        let mut guard = self.current_pool.lock().unwrap();

        std::mem::take(&mut guard)
    }
}
