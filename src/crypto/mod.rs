mod test;

use crate::messages::VoteType;
use atlas_common::crypto::threshold_crypto::{CombineSignatureError, CombinedSignature, PartialSignature, PrivateKeyPart, PublicKeyPart, PublicKeySet, VerifySignatureError};
use atlas_common::ordering::SeqNo;
use getset::Getters;
use std::error::Error;
use thiserror::Error;
use atlas_common::node_id::NodeId;
use crate::messages::serialize::serialize_vote_message;

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

    fn get_public_key_for_index(&self, index: usize) -> PublicKeyPart;

    fn get_public_key_set(&self) -> &PublicKeySet;
}

pub trait CryptoProvider: CryptoSignatureCombiner + CryptoPartialSigProvider {}

pub trait CryptoPartialSigProvider: Sync {
    fn sign_message<CR>(crypto_info: &CR, message: &[u8]) -> PartialSignature
    where
        CR: CryptoInformationProvider;
    
    fn verify_partial_signature_by_node<CR>(
        crypto_info: &CR,
        node_index: usize,
        signature: &PartialSignature,
        message: &[u8],
    ) -> Result<(), VerifySignatureError>
    where
        CR: CryptoInformationProvider;
}

pub trait CryptoSignatureCombiner: Sync {
    type CombinationError: Error + Send + Sync;
    
    type VerificationError: Error + Send + Sync;

    fn combine_signatures<CR>(
        crypto_info: &CR,
        signature: &[(NodeId, PartialSignature)],
    ) -> Result<CombinedSignature, Self::CombinationError>
    where
        CR: CryptoInformationProvider;

    fn verify_combined_signature<CR>(
        crypto_info: &CR,
        signature: &CombinedSignature,
        message: &[u8],
    ) -> Result<(), Self::VerificationError>
    where
        CR: CryptoInformationProvider;

}

pub fn get_partial_signature_for_message<CR, CP>(
    crypto_info: &CR,
    view: SeqNo,
    vote_msg: &VoteType,
) -> PartialSignature
where
    CR: CryptoInformationProvider,
    CP: CryptoPartialSigProvider,
{
    let bytes = serialize_vote_message(view, vote_msg);
    
    CP::sign_message(crypto_info, &bytes)
}

pub(crate) fn combine_partial_signatures<CR, CP>(
    crypto_info: &CR,
    signatures: &[(NodeId, PartialSignature)],
) -> Result<CombinedSignature, CP::CombinationError>
where
    CR: CryptoInformationProvider,
    CP: CryptoSignatureCombiner,
{
    CP::combine_signatures(crypto_info, signatures)
}

pub struct AtlasTHCryptoProvider;

impl CryptoProvider for AtlasTHCryptoProvider { }

impl CryptoPartialSigProvider for AtlasTHCryptoProvider {
    fn sign_message<CR>(crypto_info: &CR, message: &[u8]) -> PartialSignature
    where
        CR: CryptoInformationProvider,
    {
        crypto_info.get_own_private_key().partially_sign(message)
    }
    
    fn verify_partial_signature_by_node<CR>(
        crypto_info: &CR,
        node_index: usize,
        signature: &PartialSignature,
        message: &[u8],
    ) -> Result<(), VerifySignatureError>
    where
        CR: CryptoInformationProvider,
    {
        crypto_info.get_public_key_for_index(node_index).verify(message, signature)
    }
}

impl CryptoSignatureCombiner for AtlasTHCryptoProvider {
    type CombinationError = CombineSignatureError;
    
    type VerificationError = VerifySignatureError;

    fn combine_signatures<CR>(
        crypto_info: &CR,
        signature: &[(NodeId, PartialSignature)],
    ) -> Result<CombinedSignature, Self::CombinationError>
    where
        CR: CryptoInformationProvider,
    {
        crypto_info.get_public_key_set().combine_signatures(signature.iter().map(|(id, sig)| (id.0 as u64, sig)))
    }

    fn verify_combined_signature<CR>(crypto_info: &CR, signature: &CombinedSignature, message: &[u8]) -> Result<(), Self::VerificationError>
    where
        CR: CryptoInformationProvider
    {
        crypto_info.get_public_key_set().verify(message, signature)
    }
}

#[derive(Debug, Error)]
pub enum CombinationError {
    #[error("Failed to combine partial signatures")]
    FailedToCombinePartialSignatures,
}

impl CryptoInformationProvider for QuorumInfo {
    fn get_own_private_key(&self) -> &PrivateKeyPart {
        &self.our_priv_key
    }

    fn get_own_public_key(&self) -> &PublicKeyPart {
        &self.our_pub_key
    }

    fn get_public_key_for_index(&self, index: usize) -> PublicKeyPart {
        self.pub_key.public_key_share(index)
    }

    fn get_public_key_set(&self) -> &PublicKeySet {
        &self.pub_key
    }
}