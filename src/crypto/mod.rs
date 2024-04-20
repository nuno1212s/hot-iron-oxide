use getset::Getters;
use atlas_common::crypto::threshold_crypto::{PartialSignature, PrivateKeyPart, PublicKeyPart, PublicKeySet};
use atlas_common::ordering::SeqNo;
use crate::decisions::DecisionNode;

/// Threshold crypto related information storage
#[derive(Getters)]
#[get = "pub"]
pub struct QuorumInfo {
    #[get = "pub(crate)"]
    our_priv_key: PrivateKeyPart,
    our_pub_key: PublicKeyPart,
    pub_key: PublicKeySet,
}

pub trait CryptoInformationProvider: Sync {
    fn get_own_private_key(&self) -> &PrivateKeyPart;

    fn get_own_public_key(&self) -> &PublicKeyPart;

    fn get_public_key_for_index(&self, index: usize) -> &PublicKeyPart;

    fn get_public_key_set(&self) -> &PublicKeySet;
}

pub fn get_partial_signature_for_message<CR, D>(crypto_info: &CR,
                                                view: SeqNo,
                                                decision_node: Option<&DecisionNode<D>>) -> PartialSignature
    where CR: CryptoInformationProvider {
    
    
    todo!()
}