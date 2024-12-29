use crate::decisions::DecisionNode;
use crate::messages::VoteType;
use atlas_common::crypto::threshold_crypto::{
    CombinedSignature, PartialSignature, PrivateKeyPart, PublicKeyPart, PublicKeySet,
};
use atlas_common::ordering::SeqNo;
use getset::Getters;
use std::error::Error;
use thiserror::Error;

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

pub trait CryptoProvider: CryptoSignatureCombiner + CryptoPartialSigProvider {}

pub trait CryptoPartialSigProvider: Sync {
    fn sign_message<CR>(crypto_info: &CR, message: &[u8]) -> PartialSignature
    where
        CR: CryptoInformationProvider;
}

pub trait CryptoSignatureCombiner: Sync {
    type CombinationError: Error + Send + Sync;

    fn combine_signatures<CR>(
        crypto_info: &CR,
        signature: &[PartialSignature],
    ) -> Result<CombinedSignature, Self::CombinationError>
    where
        CR: CryptoInformationProvider;

    fn verify_combined_signature<CR>(
        crypto_info: &CR,
        signature: &CombinedSignature,
        message: &[u8],
    ) -> Result<(), Self::CombinationError>
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
    let private_key_part = crypto_info.get_own_private_key();

    todo!()
}

pub(crate) fn combine_partial_signatures<CR, CP>(
    crypto_info: &CR,
    signatures: &[Option<PartialSignature>],
) -> Result<CombinedSignature, ()>
where
    CR: CryptoInformationProvider,
    CP: CryptoSignatureCombiner,
{
    todo!()
}

struct AtlasTHCryptoProvider;

impl CryptoPartialSigProvider for AtlasTHCryptoProvider {
    fn sign_message<CR>(crypto_info: &CR, message: &[u8]) -> PartialSignature
    where
        CR: CryptoInformationProvider,
    {
        todo!()
    }
}

impl CryptoSignatureCombiner for AtlasTHCryptoProvider {
    type CombinationError = CombinationError;

    fn combine_signatures<CR>(
        crypto_info: &CR,
        signature: &[PartialSignature],
    ) -> Result<CombinedSignature, CombinationError>
    where
        CR: CryptoInformationProvider,
    {
        todo!()
    }

    fn verify_combined_signature<CR>(crypto_info: &CR, signature: &CombinedSignature, message: &[u8]) -> Result<(), Self::CombinationError>
    where
        CR: CryptoInformationProvider
    {
        todo!()
    }
}

#[derive(Debug, Error)]
pub enum CombinationError {
    #[error("Failed to combine partial signatures")]
    FailedToCombinePartialSignatures,
}