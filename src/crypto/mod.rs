mod test;

use atlas_common::crypto::threshold_crypto::{
    CombineSignatureError, CombinedSignature, PartialSignature, PrivateKeyPart, PrivateKeySet,
    PublicKeyPart, PublicKeySet, VerifySignatureError,
};
use atlas_common::node_id::NodeId;
use atlas_common::ordering::SeqNo;
use atlas_common::serialization_helper::SerMsg;
use getset::Getters;
use std::error::Error;
use std::time::Instant;
use atlas_metrics::metrics::metric_duration;
use thiserror::Error;
use crate::metric::{SIGNATURE_PROPOSAL_LATENCY_ID, SIGNATURE_VOTE_LATENCY_ID};
use crate::serialize::serialize_vote_message;

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

    /// # [Errors]
    /// Will throw errors related to the signature verification
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

    /// # [Errors]
    /// Will throw errors related to the signature combination
    fn combine_signatures<CR>(
        crypto_info: &CR,
        signature: &[(NodeId, PartialSignature)],
    ) -> Result<CombinedSignature, Self::CombinationError>
    where
        CR: CryptoInformationProvider;

    /// # [Errors]
    /// Will throw errors related to the signature verification
    fn verify_combined_signature<CR>(
        crypto_info: &CR,
        signature: &CombinedSignature,
        message: &[u8],
    ) -> Result<(), Self::VerificationError>
    where
        CR: CryptoInformationProvider;
}

pub fn get_partial_signature_for_message<CR, CP, VT>(
    crypto_info: &CR,
    view: SeqNo,
    vote_msg: &VT,
) -> PartialSignature
where
    CR: CryptoInformationProvider,
    CP: CryptoPartialSigProvider,
    VT: SerMsg,
{
    let start = Instant::now();
    
    let bytes = serialize_vote_message(view, vote_msg);

    let result = CP::sign_message(crypto_info, &bytes);
    
    metric_duration(SIGNATURE_VOTE_LATENCY_ID, start.elapsed());
    
    result
}

pub(crate) fn combine_partial_signatures<CR, CP>(
    crypto_info: &CR,
    signatures: &[(NodeId, PartialSignature)],
) -> Result<CombinedSignature, CP::CombinationError>
where
    CR: CryptoInformationProvider,
    CP: CryptoSignatureCombiner,
{
    let start = Instant::now();
    
    let result = CP::combine_signatures(crypto_info, signatures);
    
    metric_duration(SIGNATURE_PROPOSAL_LATENCY_ID, start.elapsed());
        
    result
}

pub struct AtlasTHCryptoProvider;

impl CryptoProvider for AtlasTHCryptoProvider {}

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
        crypto_info
            .get_public_key_for_index(node_index)
            .verify(message, signature)
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
        crypto_info
            .get_public_key_set()
            .combine_signatures(signature.iter().map(|(id, sig)| (id.0 as u64, sig)))
    }

    fn verify_combined_signature<CR>(
        crypto_info: &CR,
        signature: &CombinedSignature,
        message: &[u8],
    ) -> Result<(), Self::VerificationError>
    where
        CR: CryptoInformationProvider,
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

impl QuorumInfo {
    #[must_use]
    pub fn initialize(f: usize) -> Vec<Self> {
        let key_set = PrivateKeySet::gen_random(2 * f);

        let n = 3 * f + 1;
        //TODO: Improve this

        (0..n)
            .map(|i| {
                let priv_key = key_set.private_key_part(i);
                let pub_key = key_set.public_key_set();

                let our_pub_key = pub_key.public_key_share(i);

                Self {
                    our_priv_key: priv_key,
                    our_pub_key,
                    pub_key,
                }
            })
            .collect()
    }

    #[must_use]
    pub fn new(
        private_key_part: PrivateKeyPart,
        public_key_part: PublicKeyPart,
        public_key_set: PublicKeySet,
    ) -> Self {
        Self {
            our_priv_key: private_key_part,
            our_pub_key: public_key_part,
            pub_key: public_key_set,
        }
    }
}
