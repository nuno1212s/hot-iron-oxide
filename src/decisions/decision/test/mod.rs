#[cfg(test)]
mod decision_test {
    use atlas_common::error::*;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use anyhow::anyhow;
    use serde::{Deserialize, Serialize};
    use atlas_common::collections::HashMap;
    use atlas_common::crypto::hash::Digest;
    use atlas_common::crypto::signature::{KeyPair, PublicKey};
    use atlas_common::crypto::threshold_crypto::{PrivateKeyPart, PrivateKeySet, PublicKeyPart, PublicKeySet};
    use atlas_common::node_id::{NodeId, NodeType};
    use atlas_common::ordering::{Orderable, SeqNo};
    use atlas_common::peer_addr::PeerAddr;
    use atlas_common::serialization_helper::SerType;
    use atlas_communication::lookup_table::MessageModule;
    use atlas_communication::message::{Buf, Header, SerializedMessage, StoredMessage, StoredSerializedMessage, WireMessage};
    use atlas_communication::reconfiguration;
    use atlas_communication::reconfiguration::NetworkInformationProvider;
    use atlas_core::messages::{ForwardedRequestsMessage, StoredRequestMessage};
    use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
    use atlas_core::ordering_protocol::networking::serialize::NetworkView;
    use rand::random;
    use crate::crypto::{get_partial_signature_for_message, CryptoInformationProvider};
    use crate::decisions::decision::HSDecision;
    use crate::decisions::DecisionHandler;
    use crate::decisions::req_aggr::ReqAggregator;
    use crate::messages::{HotFeOxMsg, HotFeOxMsgType, VoteMessage, VoteType};
    use crate::messages::serialize::HotIronOxSer;
    use crate::view::leader_allocation::RoundRobinLA;
    use crate::view::View;

    #[derive(Clone, Serialize, Deserialize)]
    struct BlankProtocol;

    #[derive(Clone)]
    pub struct NodeInfo<K> {
        node_info: reconfiguration::NodeInfo,
        key: K,
    }

    struct MockNetworkInfo {
        own_node: NodeInfo<Arc<KeyPair>>,
        other_nodes: BTreeMap<NodeId, NodeInfo<PublicKey>>,
    }

    impl NetworkInformationProvider for MockNetworkInfo {
        fn own_node_info(&self) -> &reconfiguration::NodeInfo {
            &self.own_node.node_info
        }

        fn get_key_pair(&self) -> &Arc<KeyPair> {
            &self.own_node.key
        }

        fn get_node_info(&self, node: &NodeId) -> Option<reconfiguration::NodeInfo> {
            self.other_nodes
                .get(node)
                .map(|info| info.node_info.clone())
        }
    }

    struct CryptoInfoMockFactory {
        nodes: Vec<NodeId>,
        pkey_set: PrivateKeySet,
        pub_key_set: PublicKeySet,
    }

    impl CryptoInfoMockFactory {

        fn new(node_count: usize) -> Result<Self> {

            let nodes = (0..node_count).map(NodeId::from).collect::<Vec<_>>();

            let private_key = PrivateKeySet::gen_random(node_count)?;

            let public_key = private_key.public_key_set();

            Ok(CryptoInfoMockFactory {
                nodes,
                pkey_set: private_key,
                pub_key_set: public_key,
            })
        }

        fn create_mock_for(&self, node_id: NodeId) -> Result<CryptoInfoMock> {

            let index =  node_id.into();
            let private_key_part = self.pkey_set.private_key_part(index)?;


            let public_key_parts = self.nodes.iter()
                .map(|node| {
                    let index = (*node).into();

                    let pub_key = self.pub_key_set.public_key_share(index).unwrap();

                    (*node, pub_key)
                })
                .collect::<HashMap<_, _>>();

            Ok(CryptoInfoMock {
                id: node_id,
                private_key_part,
                public_key_parts,
                pub_key_set: self.pub_key_set.clone(),
                node_list: self.nodes.clone(),
            })

        }

    }

    struct CryptoInfoMock {
        id: NodeId,
        private_key_part: PrivateKeyPart,
        public_key_parts: HashMap<NodeId, PublicKeyPart>,
        pub_key_set: PublicKeySet,
        node_list: Vec<NodeId>
    }

    impl CryptoInformationProvider for CryptoInfoMock {
        fn get_own_private_key(&self) -> &PrivateKeyPart {
            &self.private_key_part
        }

        fn get_own_public_key(&self) -> &PublicKeyPart {
            self.public_key_parts.get(&self.id).unwrap()
        }

        fn get_public_key_for_index(&self, index: usize) -> &PublicKeyPart {
            self.public_key_parts.get(&self.node_list[index]).unwrap()
        }

        fn get_public_key_set(&self) -> &PublicKeySet {
            &self.pub_key_set
        }
    }

    struct MockNetworkInfoFactory {
        nodes: BTreeMap<NodeId, NodeInfo<Arc<KeyPair>>>,
    }

    impl MockNetworkInfoFactory {
        const PORT: u32 = 10000;

        fn initialize_for(node_count: usize) -> atlas_common::error::Result<Self> {
            let buf = [0; 32];
            let mut map = BTreeMap::default();

            for node_id in 0..node_count {
                let key = KeyPair::from_bytes(buf.as_slice())?;

                let info = NodeInfo {
                    node_info: reconfiguration::NodeInfo::new(
                        NodeId::from(node_id as u32),
                        NodeType::Replica,
                        PublicKey::from(key.public_key()),
                        PeerAddr::new(
                            format!("127.0.0.1:{}", Self::PORT + (node_id as u32)).parse()?,
                            String::from("localhost"),
                        ),
                    ),
                    key: Arc::new(key),
                };

                map.insert(info.node_info.node_id(), info);
            }

            Ok(Self { nodes: map })
        }

        fn generate_network_info_for(
            &self,
            node_id: NodeId,
        ) -> atlas_common::error::Result<MockNetworkInfo> {
            let own_network_id = self
                .nodes
                .get(&node_id)
                .ok_or(anyhow!("Node not found"))?
                .clone();

            let other_nodes: BTreeMap<NodeId, NodeInfo<PublicKey>> = self
                .nodes
                .iter()
                .filter(|(id, _)| **id != node_id)
                .map(|(id, info)| {
                    (
                        *id,
                        NodeInfo {
                            node_info: info.node_info.clone(),
                            key: PublicKey::from(info.key.public_key()),
                        },
                    )
                })
                .collect();

            Ok(MockNetworkInfo {
                own_node: own_network_id,
                other_nodes,
            })
        }
    }

    struct NetworkNode {
        node: NodeId,
        network_info: Arc<MockNetworkInfo>,
    }

    impl NetworkNode {
        fn new(node_id: Option<NodeId>, network_info: Arc<MockNetworkInfo>) -> Self {
            NetworkNode {
                node: node_id.unwrap_or(NodeId(0)),
                network_info,
            }
        }
    }

    impl<RQ> OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> for NetworkNode
    where RQ: SerType {
        type NetworkInfoProvider = MockNetworkInfo;

        fn id(&self) -> NodeId {
            self.node
        }

        fn network_info_provider(&self) -> &Arc<Self::NetworkInfoProvider> {
            &self.network_info
        }

        fn forward_requests<T>(
            &self,
            fwd_requests: ForwardedRequestsMessage<RQ>,
            targets: T,
        ) -> std::result::Result<(), Vec<NodeId>>
        where T: Iterator<Item = NodeId> {
            Ok(())
        }

        fn send(&self, message: HotFeOxMsg<RQ>, target: NodeId, flush: bool) -> Result<()> {
            Ok(())
        }

        fn send_signed(&self, message: HotFeOxMsg<RQ>, target: NodeId, flush: bool)
                       -> Result<()> {
            Ok(())
        }

        fn broadcast<I>(
            &self,
            message: HotFeOxMsg<RQ>,
            targets: I,
        ) -> std::result::Result<(), Vec<NodeId>>
        where I: Iterator<Item = NodeId> {
            Ok(())
        }

        fn broadcast_signed<I>(
            &self,
            message: HotFeOxMsg<RQ>,
            targets: I,
        ) -> std::result::Result<(), Vec<NodeId>>
        where I: Iterator<Item = NodeId> {
            Ok(())
        }

        fn serialize_digest_message(
            &self,
            message: HotFeOxMsg<RQ>,
        ) -> Result<(SerializedMessage<HotFeOxMsg<RQ>>, Digest)> {
            unimplemented!()
        }

        fn broadcast_serialized(
            &self,
            messages: BTreeMap<NodeId, StoredSerializedMessage<HotFeOxMsg<RQ>>>,
        ) -> std::result::Result<(), Vec<NodeId>> {
            Ok(())
        }
    }

    struct RQAggr;

    impl<RQ> ReqAggregator<RQ> for RQAggr {
        fn take_pool_requests(&self) -> (Vec<StoredRequestMessage<RQ>>, Digest) {
            (vec![], Digest::blank())
        }
    }

    fn setup_decision<D>(seq_no: Option<SeqNo>, node_id: Option<NodeId>) -> HSDecision<D>
    where
        D: SerType,
    {

        const NODE_COUNT: u32 = 4;

        let quorum = (0..NODE_COUNT).map(NodeId::from).collect::<Vec<_>>();

        let our_node_id = node_id.unwrap_or(quorum[0]);

        let view = View::new_from_quorum::<RoundRobinLA>(seq_no.unwrap_or(SeqNo::ZERO), quorum);

        HSDecision::new(view, our_node_id)
    }

    fn setup_network_node< RQ>(node: NodeId, mock_info_factory: &MockNetworkInfoFactory) -> Arc<NetworkNode>
    where
        RQ: SerType,
    {
        let info = Arc::new(mock_info_factory.generate_network_info_for(node).unwrap());

        Arc::new(NetworkNode::new(Some(node), info))
    }

    const NODE_COUNT: usize = 4;

    #[test]
    fn test_new_view_leader() {
        let nodes = (0..NODE_COUNT).map(NodeId::from).collect::<Vec<_>>();

        let rq_aggr = Arc::new(RQAggr);

        let mock_info_factory = MockNetworkInfoFactory::initialize_for(NODE_COUNT).unwrap();

        let node = setup_network_node::<BlankProtocol>(NodeId(0), &mock_info_factory);

        let mut decision = setup_decision::<BlankProtocol>(None, None);

        let threshold_crypto = CryptoInfoMockFactory::new(NODE_COUNT).unwrap();

        let cryptos_for = nodes.iter()
            .map(|node| (node.clone(), Arc::new(threshold_crypto.create_mock_for(*node).unwrap())))
            .collect::<HashMap<_, _>>();

        let leader = decision.view().primary();

        let decision_handler = DecisionHandler::default();

        let results = decision.view().quorum_members()
            .clone().iter()
            .filter(|node| **node != leader)
            .map(|node_id| {
                let crypto = cryptos_for.get(node_id).unwrap();

                let vote_type = VoteType::<BlankProtocol>::NewView(None);

                let msg_signature = get_partial_signature_for_message(&**crypto, decision.view().sequence_number(), &vote_type);

                let new_view_message = HotFeOxMsg::new(decision.view().sequence_number(), HotFeOxMsgType::Vote(VoteMessage::new(vote_type, msg_signature)));

                let stored_message = WireMessage::new(*node_id, leader, MessageModule::Protocol, Buf::new(), 0, None, None);
                
                let header = stored_message.into_inner().0;

                decision.process_message(Arc::new(StoredMessage::new(header, new_view_message)), &node, &decision_handler, crypto, &rq_aggr).unwrap()
            }).collect::<Vec<_>>();
        
        
    }
}