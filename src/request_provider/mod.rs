use atlas_communication::message::StoredMessage;
use atlas_core::request_pre_processing::BatchOutput;

pub trait ClientRQProvider<RQ> {
    
    /// Receive requests from the client 
    fn receive_client_requests(&self) -> Vec<StoredMessage<RQ>>; 
    
}

pub struct BatchedRequestProvider<RQ> {
    request_channel: BatchOutput<RQ>
}

impl<RQ> ClientRQProvider<RQ> for BatchedRequestProvider<RQ> {
    
    fn receive_client_requests(&self) -> Vec<StoredMessage<RQ>> {
        
        let mut consumed_requests = Vec::new();
        
        while let Ok(requests) = self.request_channel.try_recv() {
            consumed_requests.append(&mut requests.into());
        }
        
        consumed_requests
    }
    
}