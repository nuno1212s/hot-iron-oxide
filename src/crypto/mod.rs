use std::error::Error;
use getset::Getters;
use atlas_common::crypto::threshold_crypto::{CombinedSignature, PartialSignature, PrivateKeyPart, PublicKeyPart, PublicKeySet};
use atlas_common::ordering::SeqNo;
use crate::decisions::DecisionNode;
use crate::messages::VoteType;

/// Threshold crypto related information storage
#[derive(Getters)]
#[get = "pub"]
pub struct QuorumInfo {
    #[get = "pub(crate)"]
    our_priv_key: PrivateKeyPart,
    our_pub_key: PublicKeyPart,
    pub_key: PublicKeySet,
}

pub trait CryptoInformationProvider: Sync + Send + 'static {
    
    fn get_own_private_key(&self) -> &PrivateKeyPart;

    fn get_own_public_key(&self) -> &PublicKeyPart;

    fn get_public_key_for_index(&self, index: usize) -> &PublicKeyPart;

    fn get_public_key_set(&self) -> &PublicKeySet;
}

pub trait CryptoSignerProvider: Sync {
    
    fn sign_message<CR>(crypto_info: &CR, message: &[u8]) -> PartialSignature
        where CR: CryptoInformationProvider;
    
}

pub trait CryptoSignatureCombiner: Sync {
    
    type VerificationError: Error + Send + Sync;
    
    fn combine_signatures<CR>(crypto_info: &CR, signature: &[PartialSignature]) -> Result<CombinedSignature, Self::VerificationError>
        where CR: CryptoInformationProvider;
    
}

pub fn get_partial_signature_for_message<CR, D>(crypto_info: &CR,
                                                view: SeqNo,
                                                vote_msg: &VoteType<D>) -> PartialSignature
    where CR: CryptoInformationProvider {
    todo!()
}


pub(crate) fn combine_partial_signatures<CR>(crypto_info: &CR,
                                             signatures: &[PartialSignature]) -> Result<CombinedSignature, ()>
    where CR: CryptoInformationProvider {
    
    todo!()
}