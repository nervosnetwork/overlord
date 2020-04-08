use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;

use bytes::{Bytes, BytesMut};
use derive_more::Display;
use hasher::{Hasher, HasherKeccak};
use lazy_static::lazy_static;
use ophelia::{BlsSignatureVerify, Error as SigError, HashValue, PrivateKey, Signature};
use ophelia_bls_amcl::{BlsCommonReference, BlsPrivateKey, BlsPublicKey, BlsSignature};
use parking_lot::RwLock;

use crate::types::{Address, Hash, Signature as SigBytes};
use crate::Crypto;

lazy_static! {
    static ref HASHER_INST: HasherKeccak = HasherKeccak::new();
}

const HASH_LEN: usize = 32;

pub struct DefaultCrypto {
    pri_key:    BlsPrivateKey,
    pub_keys:   RwLock<HashMap<Address, BlsPublicKey>>,
    common_ref: BlsCommonReference,
}

impl Crypto for DefaultCrypto {
    fn hash(&self, msg: Bytes) -> Hash{
        let mut out = [0u8; HASH_LEN];
        out.copy_from_slice(&HASHER_INST.digest(&msg));
        BytesMut::from(out.as_ref()).freeze()
    }

    fn sign(&self, hash: Hash) -> Result<SigBytes, Box<dyn Error + Send>> {
        let hash_value =
            HashValue::try_from(hash.as_ref()).map_err(|_| CryptoError::TryInfoHashValueFailed)?;
        let sig = self.pri_key.sign_message(&hash_value);
        Ok(sig.to_bytes())
    }

    fn verify_signature(
        &self,
        signature: SigBytes,
        hash: Hash,
        signer: Address,
    ) -> Result<(), Box<dyn Error + Send>> {
        let pub_keys = self.pub_keys.read();
        let hash =
            HashValue::try_from(hash.as_ref()).map_err(|_| CryptoError::TryInfoHashValueFailed)?;
        let pub_key = pub_keys
            .get(&signer)
            .ok_or_else(|| CryptoError::UnauthorizedAddress(hex::encode(signer)))?;
        let signature = BlsSignature::try_from(signature.as_ref())
            .map_err(|e| CryptoError::TryInfoBlsSignatureFailed(e))?;

        signature
            .verify(&hash, &pub_key, &self.common_ref)
            .map_err(|e| CryptoError::VerifyFailed(e))?;
        Ok(())
    }

    fn aggregate_signatures(
        &self,
        signatures: Vec<SigBytes>,
        signers: Vec<Address>,
    ) -> Result<SigBytes, Box<dyn Error + Send>> {
        if signatures.len() != signers.len() {
            return Err(CryptoError::MisMatchNumber(signatures.len(), signers.len()).into());
        }

        let pub_keys = self.pub_keys.read();
        let mut map = Vec::with_capacity(signatures.len());
        for (sig, addr) in signatures.iter().zip(signers.iter()) {
            let pub_key = pub_keys
                .get(addr)
                .ok_or_else(|| CryptoError::UnauthorizedAddress(hex::encode(addr)))?;
            let signature = BlsSignature::try_from(sig.as_ref())
                .map_err(|e| CryptoError::TryInfoBlsSignatureFailed(e))?;

            map.push((signature, pub_key.to_owned()));
        }

        let sig = BlsSignature::combine(map);
        Ok(sig.to_bytes())
    }

    fn verify_aggregated_signature(
        &self,
        aggregated_signature: SigBytes,
        hash: Hash,
        signers: Vec<Address>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let pub_keys = self.pub_keys.read();
        let mut map = Vec::new();
        for addr in signers.iter() {
            let pub_key = pub_keys
                .get(addr)
                .ok_or_else(|| CryptoError::UnauthorizedAddress(hex::encode(addr)))?;
            map.push(pub_key);
        }

        let aggregate_key = BlsPublicKey::aggregate(map);
        let aggregated_signature = BlsSignature::try_from(aggregated_signature.as_ref())
            .map_err(|e| CryptoError::TryInfoBlsSignatureFailed(e))?;
        let hash =
            HashValue::try_from(hash.as_ref()).map_err(|_| CryptoError::TryInfoHashValueFailed)?;

        aggregated_signature
            .verify(&hash, &aggregate_key, &self.common_ref)
            .map_err(|e| CryptoError::VerifyAggregateFailed(e))?;
        Ok(())
    }
}

impl DefaultCrypto {
    pub fn new(
        pri_key_hex_str: &str,
        pub_key_hex_strs: HashMap<&str, &str>,
        common_ref_hex_str: &str,
    ) -> Self {
        let common_ref = hex::decode(common_ref_hex_str).unwrap();
        let common_ref: BlsCommonReference = std::str::from_utf8(common_ref.as_ref()).unwrap().into();

        let pri_key = hex::decode(pri_key_hex_str).unwrap();
        let pri_key = BlsPrivateKey::try_from(pri_key.as_ref()).unwrap();

        let pub_keys: HashMap<Address, BlsPublicKey> = pub_key_hex_strs.iter().map(|(address, pub_key_hex_str)| {
            let address = Bytes::from(hex::decode(address).unwrap());
            let pub_key = hex::decode(pub_key_hex_str).unwrap();
            let pub_key = BlsPublicKey::try_from(pub_key.as_ref()).unwrap();
            (address, pub_key)
        }).collect();

        let pub_keys = RwLock::new(pub_keys);

        DefaultCrypto {
            pri_key,
            pub_keys,
            common_ref,
        }
    }

    pub fn update(&self, new_pub_keys: HashMap<Address, BlsPublicKey>) {
        let mut pub_keys = self.pub_keys.write();
        *pub_keys = new_pub_keys;
    }
}

pub struct KeyPairGenerator {
    number: usize,
    common_ref: Option<String>,
}

pub struct KeyPairs {
    common_ref: String,
    key_pairs: HashMap<String, String>,
}

// pub fn gen_key_pairs(generator: KeyPairGenerator) -> KeyPairs {
//
// }

#[derive(Debug, Display)]
pub enum CryptoError {
    #[display(fmt = "Try into HashValue failed")]
    TryInfoHashValueFailed,

    #[display(fmt = "Try into BlsSignature failed, {:?}", _0)]
    TryInfoBlsSignatureFailed(SigError),

    #[display(fmt = "Unauthorized address {}", _0)]
    UnauthorizedAddress(String),

    #[display(fmt = "Verify signature failed, {:?}", _0)]
    VerifyFailed(SigError),

    #[display(fmt = "Verify aggregated signature failed, {:?}", _0)]
    VerifyAggregateFailed(SigError),

    #[display(fmt = "Mismatch number, signature number {} address number {}", _0, _1)]
    MisMatchNumber(usize, usize),
}

impl From<CryptoError> for Box<dyn Error + Send> {
    fn from(error: CryptoError) -> Self {
        Box::new(error) as Box<dyn Error + Send>
    }
}

impl Error for CryptoError {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_default_crypto() {
        let common_ref = "7446645045376b553041";

        let pri_keys = vec![
            "00000000000000000000000000000000592d6f62cd5c3464d4956ea585ec7007bcf5217eb89cc50bf14eea95f3b09706",
            "000000000000000000000000000000008b41630934fc7df92a016af88aae477bd173118fb72113f31db8a950230b029f",
            "0000000000000000000000000000000028f53779b60e261ba68cdccefcc6a152136df8cb794e067ec76dc5a02c8f2ccf",
            "000000000000000000000000000000008e065679aa8b1185406c7514343073cd8c1695218925c9b2d5e98c3483d71d81",
        ];

        let pub_keys: HashMap<&str, &str> = vec![
            ("12d8baf8c4efb32a7983efac2d8535fe57deb756",
             "040386a8ac1cce6fd90c31effa628bc8513cbd625c752ca76ade6ff37b97edbdfb97d94caeddd261d9e2fd6b5456aecc100ea730ddee3c94f040a54152ded330a4e409f39bfbc34b286536790fef8bbaf734431679ba6a8d5d6994e557e82306df"),
            ("a55e1261a73116c755291140e427caa0cbb5309e",
             "040e7b00b59d37d4d735041ea1b69a55cd7fd80e920b5d70d85d051af6b847c3aec5b412b128f85ad8b4c6bac0561105a80fa8dd5f60cd42c3a2da0fd0b946fa3d761b1d21c569e0958b847da22dec14a132121027006df8c5d4ccf7caf8535f70"),
            ("78ef0eff2fb9f569d86d75d22b69ea8407f6f092",
             "0413584a15f1dec552bb12233bf73a886ed49a3f56c68eda080743577005417635c9ac72a528a961a0e14a2df3a50a5c660641f446f629788486d7935d4ad4918035ce884a98bbaaa4c96307a2428729cba694329a693ce60c02e13b039c6a8978"),
            ("103252cad4e0380fe57a0c73f549f1ee2c9ea8e8",
             "041611b7da94a7fb7a8ff1c802bbf61da689f8d6f974d99466adeb1f47bcaff70470b6f279763abeb0cec5565abcfcb4ce13e79b8c310f0d1b26605b61ac2c04e0efcedbae18e763a86adb7a0e8ed0fcb1dc11fded12583972403815a7aa3dc300"),
        ].iter().collect();

        let crypto_0 = DefaultCrypto::new(pri_keys[0], pub_keys.clone(), common_ref.clone());
        let crypto_1 = DefaultCrypto::new(pri_keys[1], pub_keys.clone(), common_ref.clone());
        let crypto_2 = DefaultCrypto::new(pri_keys[2], pub_keys.clone(), common_ref.clone());
        let crypto_3 = DefaultCrypto::new(pri_keys[3], pub_keys.clone(), common_ref.clone());

        let msg = (Bytes::from("test_crypto"));
    }
}
