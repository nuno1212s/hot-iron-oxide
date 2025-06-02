use crate::chained::messages::IronChainMessage;
use atlas_core::ordering_protocol::ShareableMessage;
use std::collections::VecDeque;

pub struct ChainedHotStuffMsgQueue<D> {
    get_queue: bool,
    pending_messages: VecDeque<ShareableMessage<IronChainMessage<D>>>,
}

impl<D> ChainedHotStuffMsgQueue<D> {
    pub fn should_poll(&self) -> bool {
        self.get_queue
    }

    pub fn queue_message(&mut self, message: ShareableMessage<IronChainMessage<D>>) {
        self.get_queue = true;
        self.pending_messages.push_back(message);
    }

    pub fn pop_message(&mut self) -> Option<ShareableMessage<IronChainMessage<D>>> {
        let popped_message = self.pending_messages.pop_front();

        if popped_message.is_none() {
            self.get_queue = false;
        }

        popped_message
    }
}

impl<D> Default for ChainedHotStuffMsgQueue<D> {
    fn default() -> Self {
        Self {
            get_queue: false,
            pending_messages: VecDeque::default(),
        }
    }
}
