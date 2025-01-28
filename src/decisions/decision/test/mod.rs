#[cfg(test)]
mod decision_test {
    use crate::crypto::{
        get_partial_signature_for_message, AtlasTHCryptoProvider, CryptoInformationProvider,
    };
    use crate::decisions::decision::{
        DecisionPollResult, DecisionResult, DecisionState, HSDecision,
    };
    use crate::decisions::log::MsgLeaderDecisionLog;
    use crate::decisions::req_aggr::ReqAggregator;
    use crate::decisions::{DecisionHandler, DecisionNode, QCType, QC};
    use crate::messages::serialize::HotIronOxSer;
    use crate::messages::{
        HotFeOxMsg, HotFeOxMsgType, ProposalMessage, ProposalType, VoteMessage, VoteType,
    };
    use crate::view::leader_allocation::RoundRobinLA;
    use crate::view::View;
    use anyhow::anyhow;
    use atlas_common::collections::HashMap;
    use atlas_common::crypto::hash::Digest;
    use atlas_common::crypto::signature::{KeyPair, PublicKey};
    use atlas_common::crypto::threshold_crypto::{
        PrivateKeyPart, PrivateKeySet, PublicKeyPart, PublicKeySet,
    };
    use atlas_common::error::*;
    use atlas_common::node_id::{NodeId, NodeType};
    use atlas_common::ordering::{Orderable, SeqNo};
    use atlas_common::peer_addr::PeerAddr;
    use atlas_common::serialization_helper::SerMsg;
    use atlas_common::{InitConfig, InitGuard};
    use atlas_communication::lookup_table::MessageModule;
    use atlas_communication::message::{
        Buf, Header, SerializedMessage, StoredMessage, StoredSerializedMessage, WireMessage,
    };
    use atlas_communication::reconfiguration;
    use atlas_communication::reconfiguration::NetworkInformationProvider;
    use atlas_core::messages::{ForwardedRequestsMessage, StoredRequestMessage};
    use atlas_core::ordering_protocol::networking::serialize::NetworkView;
    use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
    use rand::random;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;
    use std::sync::Arc;

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
        fn calculate_threshold_for_node_count(node_count: usize) -> usize {
            // Get the value of 2f
            ((node_count - 1) / 3) * 2
        }

        fn new(node_count: usize) -> Result<Self> {
            let nodes = (0..node_count).map(NodeId::from).collect::<Vec<_>>();

            let private_key =
                PrivateKeySet::gen_random(Self::calculate_threshold_for_node_count(node_count));

            let public_key = private_key.public_key_set();

            Ok(CryptoInfoMockFactory {
                nodes,
                pkey_set: private_key,
                pub_key_set: public_key,
            })
        }

        fn create_mock_for(&self, node_id: NodeId) -> CryptoInfoMock {
            let index = node_id.into();
            let private_key_part = self.pkey_set.private_key_part(index);

            let public_key_parts = self
                .nodes
                .iter()
                .map(|node| {
                    let index = (*node).into();

                    let pub_key = self.pub_key_set.public_key_share(index);

                    (*node, pub_key)
                })
                .collect::<HashMap<_, _>>();

            CryptoInfoMock {
                id: node_id,
                private_key_part,
                public_key_parts,
                pub_key_set: self.pub_key_set.clone(),
                node_list: self.nodes.clone(),
            }
        }
    }

    struct CryptoInfoMock {
        id: NodeId,
        private_key_part: PrivateKeyPart,
        public_key_parts: HashMap<NodeId, PublicKeyPart>,
        pub_key_set: PublicKeySet,
        node_list: Vec<NodeId>,
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
    where
        RQ: SerMsg,
    {
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
        where
            T: Iterator<Item = NodeId>,
        {
            Ok(())
        }

        fn send(&self, message: HotFeOxMsg<RQ>, target: NodeId, flush: bool) -> Result<()> {
            Ok(())
        }

        fn send_signed(&self, message: HotFeOxMsg<RQ>, target: NodeId, flush: bool) -> Result<()> {
            Ok(())
        }

        fn broadcast<I>(
            &self,
            message: HotFeOxMsg<RQ>,
            targets: I,
        ) -> std::result::Result<(), Vec<NodeId>>
        where
            I: Iterator<Item = NodeId>,
        {
            Ok(())
        }

        fn broadcast_signed<I>(
            &self,
            message: HotFeOxMsg<RQ>,
            targets: I,
        ) -> std::result::Result<(), Vec<NodeId>>
        where
            I: Iterator<Item = NodeId>,
        {
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
        D: SerMsg,
    {
        const NODE_COUNT: u32 = 4;

        let quorum = (0..NODE_COUNT).map(NodeId::from).collect::<Vec<_>>();

        let our_node_id = node_id.unwrap_or(quorum[0]);

        let view = View::new_from_quorum::<RoundRobinLA>(seq_no.unwrap_or(SeqNo::ZERO), quorum);

        HSDecision::new(view, our_node_id)
    }

    fn setup_network_node<RQ>(
        node: NodeId,
        mock_info_factory: &MockNetworkInfoFactory,
    ) -> Arc<NetworkNode>
    where
        RQ: SerMsg,
    {
        let info = Arc::new(mock_info_factory.generate_network_info_for(node).unwrap());

        Arc::new(NetworkNode::new(Some(node), info))
    }

    const NODE_COUNT: usize = 4;

    struct Scenario {
        nodes: Vec<NodeId>,
        rq_aggr: Arc<RQAggr>,
        mock_network_info_factory: MockNetworkInfoFactory,
        node: Arc<NetworkNode>,
        decision: HSDecision<BlankProtocol>,
        crypto_info_mock_factory: CryptoInfoMockFactory,
        init_guard: InitGuard,
    }

    fn setup_scenario(node_id: NodeId) -> Scenario {
        let init_guard = unsafe {
            atlas_common::init(InitConfig {
                async_threads: 1,
                threadpool_threads: 1,
            })
            .unwrap()
        }
        .unwrap();

        let nodes = (0..NODE_COUNT).map(NodeId::from).collect::<Vec<_>>();

        let rq_aggr = Arc::new(RQAggr);

        let mock_info_factory = MockNetworkInfoFactory::initialize_for(NODE_COUNT).unwrap();

        let node = setup_network_node::<BlankProtocol>(node_id, &mock_info_factory);

        let decision = setup_decision::<BlankProtocol>(None, Some(node_id));

        let threshold_crypto = CryptoInfoMockFactory::new(NODE_COUNT).unwrap();

        Scenario {
            nodes,
            rq_aggr,
            mock_network_info_factory: mock_info_factory,
            node,
            decision,
            crypto_info_mock_factory: threshold_crypto,
            init_guard,
        }
    }

    fn build_vote_hotstuff_message<RQ>(
        crypto: &CryptoInfoMock,
        seq_no: SeqNo,
        vote: VoteType,
    ) -> HotFeOxMsg<RQ> {
        let msg_signature =
            get_partial_signature_for_message::<_, AtlasTHCryptoProvider>(crypto, seq_no, &vote);

        HotFeOxMsg::new(
            seq_no,
            HotFeOxMsgType::Vote(VoteMessage::new(vote, msg_signature)),
        )
    }

    fn build_proposal_hotstuff_message<RQ>(
        crypto: &CryptoInfoMock,
        seq_no: SeqNo,
        proposal: ProposalType<RQ>,
    ) -> HotFeOxMsg<RQ> {
        let proposal_message = ProposalMessage::new(proposal);

        HotFeOxMsg::new(seq_no, HotFeOxMsgType::Proposal(proposal_message))
    }

    fn deliver_message_to_node<F>(
        scenario: &mut Scenario,
        create_message: F,
        target: NodeId,
        from: &[NodeId],
        cryptos_for: &HashMap<NodeId, Arc<CryptoInfoMock>>,
        decision_handler: &DecisionHandler<BlankProtocol>,
    ) -> Vec<DecisionResult<BlankProtocol>>
    where
        F: Fn(NodeId) -> HotFeOxMsg<BlankProtocol>,
    {
        from.iter()
            .map(|node_id| {
                let crypto = cryptos_for.get(node_id).unwrap();

                let message = create_message(*node_id);

                let stored_message = WireMessage::new(
                    *node_id,
                    target,
                    MessageModule::Protocol,
                    Buf::new(),
                    0,
                    None,
                    None,
                );

                let header = stored_message.into_inner().0;

                let arc = Arc::new(StoredMessage::new(header, message.clone()));

                scenario
                    .decision
                    .process_message::<_, _, AtlasTHCryptoProvider, _>(
                        arc,
                        &scenario.node,
                        decision_handler,
                        crypto,
                        &scenario.rq_aggr,
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>()
    }

    #[test]
    fn assert_queued_when_init() {
        let mut scenario = setup_scenario(NodeId(0));

        let cryptos_for = scenario
            .nodes
            .iter()
            .map(|node| {
                (
                    node.clone(),
                    Arc::new(scenario.crypto_info_mock_factory.create_mock_for(*node)),
                )
            })
            .collect::<HashMap<_, _>>();

        let leader = scenario.decision.view().primary();
        let quorum = scenario.decision.view().quorum();

        let decision_handler = DecisionHandler::default();

        let sequence_num = scenario.decision.view().sequence_number();

        let create_msg_fn = |node_id| {
            build_vote_hotstuff_message::<BlankProtocol>(
                cryptos_for.get(&node_id).unwrap(),
                sequence_num,
                VoteType::NewView(None),
            )
        };

        let targets = scenario
            .nodes
            .iter()
            .copied()
            .filter(|node| *node != leader)
            .collect::<Vec<_>>();

        let results = deliver_message_to_node(
            &mut scenario,
            create_msg_fn,
            leader,
            &targets[..],
            &cryptos_for,
            &decision_handler,
        );

        assert!(results
            .iter()
            .all(|decision| { matches!(decision, DecisionResult::MessageQueued) }))
    }

    fn new_view_decision_messages(
        scenario: &mut Scenario,
        cryptos_for: &HashMap<NodeId, Arc<CryptoInfoMock>>,
        decision_handler: &DecisionHandler<BlankProtocol>,
    ) -> Vec<DecisionResult<BlankProtocol>> {
        let leader = scenario.decision.view().primary();

        let seq_no = scenario.decision.sequence_number();

        let create_msg_fn = |node_id| {
            build_vote_hotstuff_message::<BlankProtocol>(
                cryptos_for.get(&node_id).unwrap(),
                seq_no,
                VoteType::NewView(None),
            )
        };

        let targets = scenario
            .nodes
            .iter()
            .copied()
            .filter(|node| *node != leader)
            .collect::<Vec<_>>();

        deliver_message_to_node(
            scenario,
            create_msg_fn,
            leader,
            &targets[..],
            cryptos_for,
            &decision_handler,
        )
    }

    #[test]
    fn test_new_view_leader() {
        let mut scenario = setup_scenario(NodeId(0));

        let cryptos_for = scenario
            .nodes
            .iter()
            .map(|node| {
                (
                    *node,
                    Arc::new(scenario.crypto_info_mock_factory.create_mock_for(*node)),
                )
            })
            .collect::<HashMap<_, _>>();

        let leader = scenario.decision.view().primary();
        let quorum = scenario.decision.view().quorum();

        let decision_handler = DecisionHandler::default();

        scenario
            .decision
            .set_current_state(DecisionState::Prepare(0));

        let results = new_view_decision_messages(&mut scenario, &cryptos_for, &decision_handler);

        assert_eq!(
            quorum,
            results
                .iter()
                .filter(|decision| { matches!(decision, DecisionResult::DecisionProgressed(_)) })
                .count()
        );

        assert!(matches!(
            scenario.decision.current_state,
            DecisionState::PreCommit(_)
        ));
    }

    fn prepare_decision_messages(
        scenario: &mut Scenario,
        cryptos_for: &HashMap<NodeId, Arc<CryptoInfoMock>>,
        decision_handler: &DecisionHandler<BlankProtocol>,
    ) -> Vec<DecisionResult<BlankProtocol>> {
        let leader = scenario.decision.view().primary();

        let seq_no = scenario.decision.sequence_number();

        let decision = scenario
            .decision
            .decision_log()
            .current_proposal()
            .as_ref()
            .map(|proposal| proposal.decision_header())
            .unwrap();

        let create_msg_fn = |node_id| {
            build_vote_hotstuff_message::<BlankProtocol>(
                cryptos_for.get(&node_id).unwrap(),
                seq_no,
                VoteType::PrepareVote(decision),
            )
        };

        let targets = scenario
            .nodes
            .iter()
            .copied()
            .filter(|node| *node != leader)
            .collect::<Vec<_>>();

        deliver_message_to_node(
            scenario,
            create_msg_fn,
            leader,
            &targets[..],
            cryptos_for,
            decision_handler,
        )
    }

    #[test]
    fn test_pre_commit_phase() {
        let mut scenario = setup_scenario(NodeId(0));

        let cryptos_for = scenario
            .nodes
            .iter()
            .map(|node| {
                (
                    *node,
                    Arc::new(scenario.crypto_info_mock_factory.create_mock_for(*node)),
                )
            })
            .collect::<HashMap<_, _>>();

        let leader = scenario.decision.view().primary();
        let quorum = scenario.decision.view().quorum();

        let decision_handler = DecisionHandler::default();

        scenario
            .decision
            .set_current_state(DecisionState::Prepare(0));

        let seq_no = scenario.decision.sequence_number();

        let _ = new_view_decision_messages(&mut scenario, &cryptos_for, &decision_handler);

        let results = prepare_decision_messages(&mut scenario, &cryptos_for, &decision_handler);

        assert_eq!(
            quorum - 1,
            results
                .iter()
                .filter(|decision| { matches!(decision, DecisionResult::DecisionProgressed(_)) })
                .count()
        );

        assert_eq!(
            1,
            results
                .iter()
                .filter(|decision| { matches!(decision, DecisionResult::PrepareQC(_, _)) })
                .count()
        );

        assert!(matches!(
            scenario.decision.current_state,
            DecisionState::Commit(_)
        ));
    }

    fn commit_decision_messages(
        scenario: &mut Scenario,
        cryptos_for: &HashMap<NodeId, Arc<CryptoInfoMock>>,
        decision_handler: &DecisionHandler<BlankProtocol>,
    ) -> Vec<DecisionResult<BlankProtocol>> {
        let leader = scenario.decision.view().primary();

        let seq_no = scenario.decision.sequence_number();

        let decision = scenario
            .decision
            .decision_log()
            .current_proposal()
            .as_ref()
            .map(|proposal| proposal.decision_header())
            .unwrap();

        let create_msg_fn = |node_id| {
            build_vote_hotstuff_message::<BlankProtocol>(
                cryptos_for.get(&node_id).unwrap(),
                seq_no,
                VoteType::PreCommitVote(decision),
            )
        };

        let targets = scenario
            .nodes
            .iter()
            .copied()
            .filter(|node| *node != leader)
            .collect::<Vec<_>>();

        deliver_message_to_node(
            scenario,
            create_msg_fn,
            leader,
            &targets[..],
            cryptos_for,
            decision_handler,
        )
    }

    #[test]
    fn test_commit_phase() {
        let mut scenario = setup_scenario(NodeId(0));

        let cryptos_for = scenario
            .nodes
            .iter()
            .map(|node| {
                (
                    *node,
                    Arc::new(scenario.crypto_info_mock_factory.create_mock_for(*node)),
                )
            })
            .collect::<HashMap<_, _>>();

        let leader = scenario.decision.view().primary();
        let quorum = scenario.decision.view().quorum();

        let decision_handler = DecisionHandler::default();

        scenario
            .decision
            .set_current_state(DecisionState::Prepare(0));

        let seq_no = scenario.decision.sequence_number();

        let _ = new_view_decision_messages(&mut scenario, &cryptos_for, &decision_handler);

        let _ = prepare_decision_messages(&mut scenario, &cryptos_for, &decision_handler);

        let results = commit_decision_messages(&mut scenario, &cryptos_for, &decision_handler);

        assert_eq!(
            quorum - 1,
            results
                .iter()
                .filter(|decision| { matches!(decision, DecisionResult::DecisionProgressed(_)) })
                .count()
        );

        assert_eq!(
            1,
            results
                .iter()
                .filter(|decision| { matches!(decision, DecisionResult::LockedQC(_, _)) })
                .count()
        );

        assert!(matches!(
            scenario.decision.current_state,
            DecisionState::Decide(_)
        ));
    }

    fn decide_decision_messages(
        scenario: &mut Scenario,
        cryptos_for: &HashMap<NodeId, Arc<CryptoInfoMock>>,
        decision_handler: &DecisionHandler<BlankProtocol>,
    ) -> Vec<DecisionResult<BlankProtocol>> {
        let leader = scenario.decision.view().primary();

        let seq_no = scenario.decision.sequence_number();

        let decision = scenario
            .decision
            .decision_log()
            .current_proposal()
            .as_ref()
            .map(|proposal| proposal.decision_header())
            .unwrap();

        let create_msg_fn = |node_id| {
            build_vote_hotstuff_message::<BlankProtocol>(
                cryptos_for.get(&node_id).unwrap(),
                seq_no,
                VoteType::CommitVote(decision),
            )
        };

        let targets = scenario
            .nodes
            .iter()
            .copied()
            .filter(|node| *node != leader)
            .collect::<Vec<_>>();

        deliver_message_to_node(
            scenario,
            create_msg_fn,
            leader,
            &targets[..],
            cryptos_for,
            decision_handler,
        )
    }

    #[test]
    fn test_decide_phase() {
        let mut scenario = setup_scenario(NodeId(0));

        let cryptos_for = scenario
            .nodes
            .iter()
            .map(|node| {
                (
                    *node,
                    Arc::new(scenario.crypto_info_mock_factory.create_mock_for(*node)),
                )
            })
            .collect::<HashMap<_, _>>();

        let leader = scenario.decision.view().primary();
        let quorum = scenario.decision.view().quorum();

        let decision_handler = DecisionHandler::default();

        scenario
            .decision
            .set_current_state(DecisionState::Prepare(0));

        let seq_no = scenario.decision.sequence_number();

        let _ = new_view_decision_messages(&mut scenario, &cryptos_for, &decision_handler);

        let _ = prepare_decision_messages(&mut scenario, &cryptos_for, &decision_handler);

        let _ = commit_decision_messages(&mut scenario, &cryptos_for, &decision_handler);

        let results = decide_decision_messages(&mut scenario, &cryptos_for, &decision_handler);

        assert_eq!(
            quorum - 1,
            results
                .iter()
                .filter(|decision| { matches!(decision, DecisionResult::DecisionProgressed(_)) })
                .count()
        );

        assert_eq!(
            1,
            results
                .iter()
                .filter(|decision| { matches!(decision, DecisionResult::Decided(_)) })
                .count()
        );

        assert!(matches!(
            scenario.decision.current_state,
            DecisionState::Finally
        ));
    }

    fn prepare_proposal_messages<RQ>(
        scenario: &mut Scenario,
        cryptos_for: &HashMap<NodeId, Arc<CryptoInfoMock>>,
        decision_handler: &DecisionHandler<BlankProtocol>,
        decision_log: &mut MsgLeaderDecisionLog,
    ) -> Vec<DecisionResult<BlankProtocol>> {
        let leader = scenario.decision.view().primary();

        let seq_no = scenario.decision.sequence_number();

        let (rqs, digest): (Vec<StoredRequestMessage<RQ>>, _) =
            scenario.rq_aggr.take_pool_requests();

        let (node, qc) = mock_leader_decision_log_new_view(scenario, cryptos_for, decision_log);

        scenario
            .decision
            .set_current_state(DecisionState::Prepare(0));

        let create_message = |node_id| {
            build_proposal_hotstuff_message::<BlankProtocol>(
                cryptos_for.get(&node_id).unwrap(),
                seq_no,
                ProposalType::Prepare(node.clone(), qc.clone()),
            )
        };

        deliver_message_to_node(
            scenario,
            create_message,
            NodeId(2),
            &[leader],
            cryptos_for,
            decision_handler,
        )
    }

    fn mock_leader_decision_log_new_view(
        scenario: &Scenario,
        crypto: &HashMap<NodeId, Arc<CryptoInfoMock>>,
        decision_log: &mut MsgLeaderDecisionLog,
    ) -> (DecisionNode<BlankProtocol>, QC) {
        let (rqs, digest) = scenario.rq_aggr.take_pool_requests();

        let decision = DecisionNode::create_root_leaf(scenario.decision.view(), digest, rqs);

        let sequence = scenario.decision.sequence_number();

        let leader = scenario.decision.view().primary();

        scenario.nodes.iter().for_each(|node| {
            let crypto = crypto.get(node).unwrap();

            let vote = VoteType::NewView(None);

            let msg_signature = get_partial_signature_for_message::<_, AtlasTHCryptoProvider>(
                &**crypto, sequence, &vote,
            );

            let msg = VoteMessage::new(vote, msg_signature);

            decision_log
                .new_view_store()
                .accept_new_view(*node, msg)
                .unwrap();
        });

        let qc = decision_log
            .new_view_store()
            .create_new_qc::<_, AtlasTHCryptoProvider>(
                &**crypto.get(&leader).unwrap(),
                &decision.decision_header(),
            )
            .unwrap();

        (decision, qc)
    }

    #[test]
    fn test_replica_new_view() {
        let mut scenario = setup_scenario(NodeId(2));

        let seq_no = scenario.decision.sequence_number();

        let mut msg_decision_log = MsgLeaderDecisionLog::default();
        
        let cryptos_for = scenario
            .nodes
            .iter()
            .map(|node| {
                (
                    *node,
                    Arc::new(scenario.crypto_info_mock_factory.create_mock_for(*node)),
                )
            })
            .collect::<HashMap<_, _>>();

        let leader = scenario.decision.view().primary();
        let quorum = scenario.decision.view().quorum();

        let results = prepare_proposal_messages::<BlankProtocol>(
            &mut scenario,
            &cryptos_for,
            &DecisionHandler::default(),
            &mut msg_decision_log,
        );

        assert_eq!(
            1,
            results
                .iter()
                .filter(|decision| { matches!(decision, DecisionResult::DecisionProgressed(_)) })
                .count()
        );

        assert!(matches!(
            scenario.decision.current_state,
            DecisionState::PreCommit(_)
        ));
    }

    fn mock_pre_commit(
        scenario: &Scenario,
        dec_log: &mut MsgLeaderDecisionLog,
        crypto: &HashMap<NodeId, Arc<CryptoInfoMock>>,
    ) -> QC {
        let sequence = scenario.decision.sequence_number();

        let leader = scenario.decision.view().primary();

        let decision = scenario
            .decision
            .decision_log()
            .current_proposal()
            .as_ref()
            .unwrap();

        scenario
            .nodes
            .iter()
            .filter(|node| **node != leader)
            .for_each(|node| {
                let crypto = crypto.get(node).unwrap();

                let vote = VoteType::PrepareVote(decision.decision_header());

                let msg_signature = get_partial_signature_for_message::<_, AtlasTHCryptoProvider>(
                    &**crypto, sequence, &vote,
                );

                let msg = VoteMessage::new(vote, msg_signature);

                dec_log.accept_vote(*node, msg).unwrap();
            });

        let crypto_info = crypto.get(&leader).unwrap();

        dec_log
            .generate_qc::<_, AtlasTHCryptoProvider>(
                &**crypto_info,
                scenario.decision.view(),
                QCType::PrepareVote,
            )
            .unwrap()
    }

    fn pre_commit_proposal_messages<RQ>(
        scenario: &mut Scenario,
        cryptos_for: &HashMap<NodeId, Arc<CryptoInfoMock>>,
        decision_handler: &DecisionHandler<BlankProtocol>,
        decision_log: &mut MsgLeaderDecisionLog,
    ) -> Vec<DecisionResult<BlankProtocol>> {
        let leader = scenario.decision.view().primary();

        let seq_no = scenario.decision.sequence_number();

        let (rqs, digest): (Vec<StoredRequestMessage<RQ>>, _) =
            scenario.rq_aggr.take_pool_requests();

        let mocked_qc = mock_pre_commit(scenario, decision_log, cryptos_for);

        let create_message = |node_id| {
            build_proposal_hotstuff_message::<BlankProtocol>(
                cryptos_for.get(&node_id).unwrap(),
                seq_no,
                ProposalType::PreCommit(mocked_qc.clone()),
            )
        };

        deliver_message_to_node(
            scenario,
            create_message,
            NodeId(2),
            &[leader],
            cryptos_for,
            decision_handler,
        )
    }

    #[test]
    fn test_prepare_proposal() {
        let mut scenario = setup_scenario(NodeId(2));

        let seq_no = scenario.decision.sequence_number();
        
        let mut msg_decision_log = MsgLeaderDecisionLog::default();

        let cryptos_for = scenario
            .nodes
            .iter()
            .map(|node| {
                (
                    *node,
                    Arc::new(scenario.crypto_info_mock_factory.create_mock_for(*node)),
                )
            })
            .collect::<HashMap<_, _>>();

        let leader = scenario.decision.view().primary();
        let quorum = scenario.decision.view().quorum();

        prepare_proposal_messages::<BlankProtocol>(
            &mut scenario,
            &cryptos_for,
            &DecisionHandler::default(),
            &mut msg_decision_log,
        );
        
        let results = pre_commit_proposal_messages::<BlankProtocol>(
            &mut scenario,
            &cryptos_for,
            &DecisionHandler::default(),
            &mut msg_decision_log,
        );
        
        assert_eq!(
            1,
            results
                .iter()
                .filter(|decision| { matches!(decision, DecisionResult::PrepareQC(_, _)) })
                .count()
        );

        assert!(matches!(
            scenario.decision.current_state,
            DecisionState::Commit(_)
        ));
        
    }
}
