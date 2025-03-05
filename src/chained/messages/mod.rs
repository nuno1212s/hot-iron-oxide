use crate::messages::HotFeOxMsg;

/// The hot stuff chained messages
pub struct HotStuffChainMessage<D> {
    
    messages: Vec<HotFeOxMsg<D>>
    
}

impl<D> HotStuffChainMessage<D> {
    /// Create a new hot stuff chain message
    #[must_use]
    pub fn new(messages: Vec<HotFeOxMsg<D>>) -> Self {
        Self {
            messages
        }
    }
}