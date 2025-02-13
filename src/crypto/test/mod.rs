#[cfg(test)]
mod crypto_tests {
    use crate::crypto::{
        AtlasTHCryptoProvider, CryptoInformationProvider, CryptoPartialSigProvider,
        CryptoSignatureCombiner,
    };
    use atlas_common::collections::HashMap;
    use atlas_common::crypto::threshold_crypto::{
        PrivateKeyPart, PrivateKeySet, PublicKeyPart, PublicKeySet,
    };
    use atlas_common::node_id::NodeId;

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

        fn new(node_count: usize) -> atlas_common::error::Result<Self> {
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

        fn get_public_key_for_index(&self, index: usize) -> PublicKeyPart {
            self.public_key_parts
                .get(&self.node_list[index])
                .unwrap()
                .clone()
        }

        fn get_public_key_set(&self) -> &PublicKeySet {
            &self.pub_key_set
        }
    }

    const NODE_COUNT: usize = 4;

    #[test]
    fn test_partial_signature_verification() {
        let threshold_crypto = CryptoInfoMockFactory::new(NODE_COUNT).unwrap();

        let to_sign = b"Hello, World!";

        let nodes = (0..NODE_COUNT).map(NodeId::from).collect::<Vec<_>>();

        let cryptos = nodes
            .iter()
            .map(|node_id| (*node_id, threshold_crypto.create_mock_for(*node_id)))
            .collect::<HashMap<NodeId, _>>();

        nodes.iter().for_each(|signer| {
            let signature =
                AtlasTHCryptoProvider::sign_message(cryptos.get(signer).unwrap(), to_sign);

            nodes.iter().for_each(|other_node_id| {
                AtlasTHCryptoProvider::verify_partial_signature_by_node(
                    cryptos.get(other_node_id).unwrap(),
                    signer.0 as usize,
                    &signature,
                    to_sign,
                )
                .unwrap();
            });
        });
    }

    #[test]
    fn test_partial_signature_combination() {
        let threshold_crypto = CryptoInfoMockFactory::new(NODE_COUNT).unwrap();

        let to_sign = b"Hello, World!";

        let cryptos = (0..NODE_COUNT)
            .map(NodeId::from)
            .map(|node_id| (node_id, threshold_crypto.create_mock_for(node_id)))
            .collect::<HashMap<NodeId, _>>();

        let signatures = cryptos
            .iter()
            .map(|(node_id, crypto)| {
                let sig = AtlasTHCryptoProvider::sign_message(crypto, to_sign);

                (*node_id, sig)
            })
            .collect::<Vec<_>>();

        signatures.iter().for_each(|(node_id, signature)| {
            let pub_key = cryptos
                .get(node_id)
                .unwrap()
                .get_public_key_for_index(node_id.0 as usize);

            assert!(pub_key.verify(to_sign, signature).is_ok());
        });

        cryptos.values().for_each(|crypto| {
            let combined_signature =
                AtlasTHCryptoProvider::combine_signatures(crypto, &signatures[..]).unwrap();

            AtlasTHCryptoProvider::verify_combined_signature(crypto, &combined_signature, to_sign)
                .unwrap();
        });
    }
}
