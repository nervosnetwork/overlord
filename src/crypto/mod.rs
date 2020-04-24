use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::str::Utf8Error;

use bytes::{Bytes, BytesMut};
use derive_more::Display;
use hasher::{Hasher, HasherKeccak};
use hex::FromHexError;
use lazy_static::lazy_static;
use ophelia::{
    BlsSignatureVerify, Crypto as OphCrypto, Error as SigError, HashValue, PrivateKey, PublicKey,
    Signature, ToBlsPublicKey,
};
use ophelia_bls_amcl::{BlsCommonReference, BlsPrivateKey, BlsPublicKey, BlsSignature};
use ophelia_secp256k1::Secp256k1;
use rand::distributions::Alphanumeric;
use rand::Rng;
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use tentacle_secio::SecioKeyPair;

use crate::types::{Address, Hash, Signature as SigBytes};
use crate::{Crypto, TinyHex};

lazy_static! {
    static ref HASHER_INST: HasherKeccak = HasherKeccak::new();
}

pub type AddressHex = String;
pub type PriKeyHex = String;
pub type PubKeyHex = String;
pub type BlsPubKeyHex = String;
pub type CommonHex = String;

const HASH_LEN: usize = 32;
const ADDRESS_LEN: usize = 20;

pub struct DefaultCrypto;

impl Crypto for DefaultCrypto {
    fn hash(msg: &Bytes) -> Hash {
        let mut out = [0u8; HASH_LEN];
        out.copy_from_slice(&HASHER_INST.digest(msg));
        BytesMut::from(out.as_ref()).freeze()
    }

    fn sign_msg(pri_key: PriKeyHex, hash: &Hash) -> Result<SigBytes, Box<dyn Error + Send>> {
        let pri_key = hex_decode(&pri_key)?;
        let signature =
            Secp256k1::sign_message(&hash, &pri_key).map_err(CryptoError::SignFailed)?;
        Ok(signature.to_bytes())
    }

    fn verify_signature(
        pub_key: PubKeyHex,
        address: &Address,
        hash: &Hash,
        signature: &SigBytes,
    ) -> Result<(), Box<dyn Error + Send>> {
        let pub_key = hex_decode(&pub_key)?;
        let expect_address = pub_key_to_address(Bytes::from(pub_key.clone()));
        if expect_address != address {
            return Err(Box::new(CryptoError::MismatchAddress(format!(
                "expect: {}, actual: {}",
                expect_address.tiny_hex(),
                address.tiny_hex()
            ))));
        }
        Secp256k1::verify_signature(hash, signature, &pub_key)
            .map_err(CryptoError::VerifyFailed)?;
        Ok(())
    }

    fn party_sign_msg(pri_key: PriKeyHex, hash: &Hash) -> Result<SigBytes, Box<dyn Error + Send>> {
        let pri_key = hex_to_bls_pri_key(&pri_key)?;
        let hash_value =
            HashValue::try_from(hash.as_ref()).map_err(|_| CryptoError::TryIntoHashValueFailed)?;
        let sig = pri_key.sign_message(&hash_value);
        Ok(sig.to_bytes())
    }

    fn party_verify_signature(
        common_ref: CommonHex,
        pub_key: BlsPubKeyHex,
        hash: &Hash,
        signature: &SigBytes,
    ) -> Result<(), Box<dyn Error + Send>> {
        let pub_key = hex_to_bls_pub_key(&pub_key)?;
        let common_ref = hex_to_common_ref(&common_ref)?;
        let hash =
            HashValue::try_from(hash.as_ref()).map_err(|_| CryptoError::TryIntoHashValueFailed)?;

        let signature = BlsSignature::try_from(signature.as_ref())
            .map_err(CryptoError::TryIntoBlsSignatureFailed)?;
        signature
            .verify(&hash, &pub_key, &common_ref)
            .map_err(CryptoError::VerifyFailed)?;
        Ok(())
    }

    fn aggregate(
        signatures: HashMap<&BlsPubKeyHex, &SigBytes>,
    ) -> Result<SigBytes, Box<dyn Error + Send>> {
        let mut combine = Vec::with_capacity(signatures.len());

        for (pub_key, signature) in signatures.into_iter() {
            let signature = BlsSignature::try_from(signature.as_ref())
                .map_err(CryptoError::TryIntoBlsSignatureFailed)?;
            let pub_key = hex_to_bls_pub_key(pub_key)?;
            combine.push((signature, pub_key));
        }

        Ok(BlsSignature::combine(combine).to_bytes())
    }

    fn verify_aggregates(
        common_ref: CommonHex,
        hash: &Hash,
        pub_keys: &[BlsPubKeyHex],
        signature: &SigBytes,
    ) -> Result<(), Box<dyn Error + Send>> {
        let mut list = Vec::new();
        for pub_key in pub_keys.iter() {
            let pub_key = hex_to_bls_pub_key(pub_key)?;
            list.push(pub_key);
        }

        let aggregate_key = BlsPublicKey::aggregate(list.iter().collect());
        let aggregated_signature = BlsSignature::try_from(signature.as_ref())
            .map_err(CryptoError::TryIntoBlsSignatureFailed)?;
        let hash =
            HashValue::try_from(hash.as_ref()).map_err(|_| CryptoError::TryIntoHashValueFailed)?;
        let common_ref = hex_to_common_ref(&common_ref)?;

        aggregated_signature
            .verify(&hash, &aggregate_key, &common_ref)
            .map_err(CryptoError::VerifyAggregateFailed)?;
        Ok(())
    }
}

#[derive(Clone, Default, Serialize, Debug, PartialEq, Eq)]
pub struct KeyPair {
    pub private_key:    PriKeyHex,
    pub public_key:     PubKeyHex,
    pub address:        AddressHex,
    pub bls_public_key: BlsPubKeyHex,
}

#[derive(Clone, Default, Serialize, Debug, PartialEq, Eq)]
pub struct KeyPairs {
    pub common_ref: CommonHex,
    pub key_pairs:  Vec<KeyPair>,
}

impl KeyPairs {
    pub fn get_address_list(&self) -> Vec<Address> {
        self.key_pairs
            .iter()
            .map(|key_pair| Bytes::from(hex_decode(&key_pair.address).unwrap()))
            .collect()
    }
}

pub fn gen_key_pairs(
    number: usize,
    pri_keys: Vec<PriKeyHex>,
    common_ref: Option<CommonHex>,
) -> KeyPairs {
    if pri_keys.len() > number {
        panic!("private keys length cannot be larger than number");
    }

    let common_ref_str: CommonHex = if let Some(common_ref) = common_ref {
        CommonHex::from_utf8(hex_decode(&common_ref).expect("hex decode error"))
            .expect("common_ref should be a valid utf8 string")
    } else {
        gen_random_common_ref()
    };

    let mut key_pairs = KeyPairs {
        common_ref: add_0x(hex::encode(common_ref_str.as_str())),
        key_pairs:  vec![],
    };

    for i in 0..number {
        let key_pair = gen_keypair(pri_keys.get(i), common_ref_str.as_str().into());
        key_pairs.key_pairs.push(key_pair);
    }

    println!("{}", serde_json::to_string_pretty(&key_pairs).unwrap());
    key_pairs
}

pub fn hex_to_bls_pri_key(hex_str: &str) -> Result<BlsPrivateKey, CryptoError> {
    let mut pri_key = Vec::new();
    pri_key.extend_from_slice(&[0u8; 16]);
    pri_key.append(&mut hex_decode(hex_str)?);
    BlsPrivateKey::try_from(pri_key.as_ref()).map_err(CryptoError::TryIntoBlsPriKeyFailed)
}

pub fn hex_to_bls_pub_key(hex_str: &str) -> Result<BlsPublicKey, CryptoError> {
    let bls_pub_key = hex_decode(hex_str)?;
    BlsPublicKey::try_from(bls_pub_key.as_ref()).map_err(CryptoError::TryIntoBlsPubKeyFailed)
}

pub fn hex_to_common_ref(hex_str: &str) -> Result<BlsCommonReference, CryptoError> {
    let common_ref = hex_decode(hex_str)?;
    std::str::from_utf8(common_ref.as_ref())
        .map(|str| str.into())
        .map_err(CryptoError::TryIntoCommonRefFailed)
}

pub fn hex_to_address(hex_str: &str) -> Result<Address, CryptoError> {
    Ok(Address::from(hex_decode(hex_str)?))
}

fn add_0x(s: String) -> String {
    "0x".to_owned() + &s
}

fn gen_random_common_ref() -> CommonHex {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .collect::<CommonHex>()
}

fn gen_random_pri_key() -> Bytes {
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    DefaultCrypto::hash(&BytesMut::from(seed.as_ref()).freeze())
}

fn hex_decode(hex_str: &str) -> Result<Vec<u8>, CryptoError> {
    hex::decode(ensure_trim0x(hex_str)).map_err(CryptoError::HexDecodeFailed)
}

fn ensure_trim0x(str: &str) -> &str {
    if str.starts_with("0x") || str.starts_with("0X") {
        &str[2..]
    } else {
        str
    }
}

fn pub_key_to_address(pub_key: Bytes) -> Bytes {
    let mut address = DefaultCrypto::hash(&pub_key);
    address.truncate(ADDRESS_LEN);
    address
}

fn gen_keypair(pri_key: Option<&PriKeyHex>, common_ref_str: BlsCommonReference) -> KeyPair {
    let mut key_pair = KeyPair::default();
    let pri_key = if let Some(pri_key) = pri_key {
        Bytes::from(hex_decode(pri_key).expect("decode pri_key failed"))
    } else {
        gen_random_pri_key()
    };
    let sec_key_pair =
        SecioKeyPair::secp256k1_raw_key(pri_key.as_ref()).expect("build secp256k1 keypair failed");
    let pub_key = sec_key_pair.to_public_key().inner();
    let address = pub_key_to_address(Bytes::from(pub_key.clone()));

    key_pair.private_key = add_0x(hex::encode(pri_key.as_ref()));
    key_pair.public_key = add_0x(hex::encode(pub_key));
    key_pair.address = add_0x(hex::encode(address.as_ref()));

    let bls_pri_key =
        BlsPrivateKey::try_from([&[0u8; 16], pri_key.as_ref()].concat().as_ref()).unwrap();
    let bls_pub_key = bls_pri_key.pub_key(&common_ref_str);
    key_pair.bls_public_key = add_0x(hex::encode(bls_pub_key.to_bytes()));
    key_pair
}

#[derive(Debug, Display)]
pub enum CryptoError {
    #[display(fmt = "Try into HashValue failed")]
    TryIntoHashValueFailed,

    #[display(fmt = "Try into CommonRef failed, {:?}", _0)]
    TryIntoCommonRefFailed(Utf8Error),

    #[display(fmt = "Try into BlsSignature failed, {:?}", _0)]
    TryIntoBlsSignatureFailed(SigError),

    #[display(fmt = "Try into BlsPriKey failed, {:?}", _0)]
    TryIntoBlsPriKeyFailed(SigError),

    #[display(fmt = "Try into BlsPubKey failed, {:?}", _0)]
    TryIntoBlsPubKeyFailed(SigError),

    #[display(fmt = "Try into PubKey failed, {:?}", _0)]
    TryIntoPubKeyFailed(SigError),

    #[display(fmt = "Decode Hex failed, {:?}", _0)]
    HexDecodeFailed(FromHexError),

    #[display(fmt = "Sign message failed, {:?}", _0)]
    SignFailed(SigError),

    #[display(fmt = "Verify signature failed, {:?}", _0)]
    VerifyFailed(SigError),

    #[display(fmt = "Verify aggregated signature failed, {:?}", _0)]
    VerifyAggregateFailed(SigError),

    #[display(fmt = "mismatch address, {}", _0)]
    MismatchAddress(String),
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
    fn test_gen_key_pairs() {
        // test from scratch
        gen_key_pairs(4, vec![], None);

        // test with giving common_ref and pri_keys
        let common_ref = "0x414d41716d37634a4333".to_owned();
        let pri_keys = vec![
            "eba570f6b2cabd67aede56941baa6cc66729bfbf4e15bfd7df805ae0e4f66596".to_owned(),
            "0x7f67fb6443ae9e1a94a9fbefb1e773c0154d6b691d7702972ead9f9b3d0e92aa".to_owned(),
            "727aae83188cdbcf2de2dd7aa05ec6d7cd2e2c07e9a7d5610737ff31b4517f1d".to_owned(),
            "0xcd69a204925cf5d74213a9e6bc8bdd714775f5a991eac115b65b25f306588336".to_owned(),
        ];

        let key_pairs = gen_key_pairs(4, pri_keys, Some(common_ref.clone()));

        let key_pairs_cmp = KeyPairs{
            common_ref,
            key_pairs: vec![
                KeyPair{
                    private_key:    "0xeba570f6b2cabd67aede56941baa6cc66729bfbf4e15bfd7df805ae0e4f66596".to_owned(),
                    public_key:     "0x02146d3d0a180bd1c1bce9b63caa549dc444d7dc3c875f1a0f171b63059c60bd9d".to_owned(),
                    address:        "0x77667feeaccdc991f0f21182bd04ba7277c881c1".to_owned(),
                    bls_public_key: "0x0405b1319fe6a8d06be2f892b26ccdee27373447be077a0d847f6b14b356e6cf2d7d2fe5efdde912f527275663cf8e511719268f490b55853bc30bbc6657367e5a33a0a1bb63777325f52deeaae0f06a577fd9764c56520b9c5ffde542a4696f69".to_owned(),
                },
                KeyPair{
                    private_key:    "0x7f67fb6443ae9e1a94a9fbefb1e773c0154d6b691d7702972ead9f9b3d0e92aa".to_owned(),
                    public_key:     "0x037dea2a2e9a6852c294302558c1e81fce6e1dcb431be6c5bf410719300a4f988c".to_owned(),
                    address:        "0x82fa6a3978aae4e7527c6a10e9cff9c4b018053e".to_owned(),
                    bls_public_key: "0x0417a5230fe8b508277492f9e6c649aa2e484b9477e9fdb6ea02f111b57e5eb883ede95e8974d7de98f5de25a50ce2777515274e7d9bd7344a53ba36cd217262d5f7791ed9a69237c5af932dcb6ebaf7195c5142119152b4b1b83776635b2dafc5".to_owned(),
                },
                KeyPair{
                    private_key:    "0x727aae83188cdbcf2de2dd7aa05ec6d7cd2e2c07e9a7d5610737ff31b4517f1d".to_owned(),
                    public_key:     "0x0390e439270d16c125e597939f5cfeb5caa5a528024c698ab788e6d58da815bc0b".to_owned(),
                    address:        "0x5dc3a5d4246d0468e1f0ac3776607df40481bbf6".to_owned(),
                    bls_public_key: "0x0412479e9f33ecc92d986395e9e4b239b517fd6099398237fb710e23f9425da0c50ed18cdc2d3160bebb6b23bec84ac0261682dc2d03361b172af380b16ccf2b2ce49b82a9d91372bbe9d498a2bedbd90a86ea5de8c3781d25f7f59865ac5d3b51".to_owned(),
                },
                KeyPair{
                    private_key:    "0xcd69a204925cf5d74213a9e6bc8bdd714775f5a991eac115b65b25f306588336".to_owned(),
                    public_key:     "0x02937231ec88e88a149131ebbf1aec1201bcb7de1022e9ec25507a8cecccfaeca2".to_owned(),
                    address:        "0xfd6d62572ec57829485c78f9febe2cb18438332c".to_owned(),
                    bls_public_key: "0x041a004fe6b2913dd165ee595e6884c4faf1431284406034c6a2906995801cbf677e96892a0ad0b2d63264d7c5b3a28dc20ada9dee72cd425e44ff77a1f2f7b413bcf346c0a3bc5595e9f18dd4d96c151c632d45d7c01d15c2c5991dd7719f2a38".to_owned(),
                },
            ],
        };

        assert_eq!(key_pairs, key_pairs_cmp);
    }

    #[test]
    fn test_default_crypto() {
        let common_ref = "0x414d41716d37634a4333".to_owned();

        let pri_keys = vec![
            "eba570f6b2cabd67aede56941baa6cc66729bfbf4e15bfd7df805ae0e4f66596".to_owned(),
            "7f67fb6443ae9e1a94a9fbefb1e773c0154d6b691d7702972ead9f9b3d0e92aa".to_owned(),
            "727aae83188cdbcf2de2dd7aa05ec6d7cd2e2c07e9a7d5610737ff31b4517f1d".to_owned(),
            "cd69a204925cf5d74213a9e6bc8bdd714775f5a991eac115b65b25f306588336".to_owned(),
        ];

        let addresses: Vec<Bytes> = vec![
            "0x77667feeaccdc991f0f21182bd04ba7277c881c1".to_owned(),
            "0x82fa6a3978aae4e7527c6a10e9cff9c4b018053e".to_owned(),
            "0x5dc3a5d4246d0468e1f0ac3776607df40481bbf6".to_owned(),
            "0xfd6d62572ec57829485c78f9febe2cb18438332c".to_owned(),
        ]
        .iter()
        .map(|address| Bytes::from(hex_decode(address).unwrap()))
        .collect();

        let auth_list: Vec<(Bytes, BlsPubKeyHex)> = vec![
            (Bytes::from(hex::decode("77667feeaccdc991f0f21182bd04ba7277c881c1").unwrap()),
             "0x0405b1319fe6a8d06be2f892b26ccdee27373447be077a0d847f6b14b356e6cf2d7d2fe5efdde912f527275663cf8e511719268f490b55853bc30bbc6657367e5a33a0a1bb63777325f52deeaae0f06a577fd9764c56520b9c5ffde542a4696f69".to_owned()),
            (Bytes::from(hex::decode("82fa6a3978aae4e7527c6a10e9cff9c4b018053e").unwrap()),
             "0x0417a5230fe8b508277492f9e6c649aa2e484b9477e9fdb6ea02f111b57e5eb883ede95e8974d7de98f5de25a50ce2777515274e7d9bd7344a53ba36cd217262d5f7791ed9a69237c5af932dcb6ebaf7195c5142119152b4b1b83776635b2dafc5".to_owned()),
            (Bytes::from(hex::decode("5dc3a5d4246d0468e1f0ac3776607df40481bbf6").unwrap()),
             "0x0412479e9f33ecc92d986395e9e4b239b517fd6099398237fb710e23f9425da0c50ed18cdc2d3160bebb6b23bec84ac0261682dc2d03361b172af380b16ccf2b2ce49b82a9d91372bbe9d498a2bedbd90a86ea5de8c3781d25f7f59865ac5d3b51".to_owned()),
            (Bytes::from(hex::decode("fd6d62572ec57829485c78f9febe2cb18438332c").unwrap()),
             "0x041a004fe6b2913dd165ee595e6884c4faf1431284406034c6a2906995801cbf677e96892a0ad0b2d63264d7c5b3a28dc20ada9dee72cd425e44ff77a1f2f7b413bcf346c0a3bc5595e9f18dd4d96c151c632d45d7c01d15c2c5991dd7719f2a38".to_owned()),
        ].into_iter().collect();

        let msg = Bytes::from("test_default_crypto");
        let hash = DefaultCrypto::hash(&msg);

        // test sign and party_verify_signature
        let sig_0 = DefaultCrypto::party_sign_msg(pri_keys[0].clone(), &hash).unwrap();
        DefaultCrypto::party_verify_signature(
            common_ref.clone(),
            auth_list[0].1.clone(),
            &hash,
            &sig_0,
        )
        .unwrap();

        let sig_1 = DefaultCrypto::party_sign_msg(pri_keys[1].clone(), &hash).unwrap();
        DefaultCrypto::party_verify_signature(
            common_ref.clone(),
            auth_list[1].1.clone(),
            &hash,
            &sig_1,
        )
        .unwrap();

        assert!(DefaultCrypto::party_verify_signature(
            common_ref.clone(),
            auth_list[2].1.clone(),
            &hash,
            &sig_1
        )
        .is_err());

        // test aggregate_sign and verify_aggregated_signature
        let mut map = HashMap::new();
        map.insert(&auth_list[0].1, &sig_0);
        map.insert(&auth_list[1].1, &sig_1);
        let mut signers = Vec::new();
        signers.push(&addresses[1]);
        signers.push(&addresses[0]);
        let agg_sig = DefaultCrypto::aggregate(map).unwrap();
        DefaultCrypto::verify_aggregates(
            common_ref,
            &hash,
            vec![auth_list[0].1.clone(), auth_list[1].1.clone()].as_slice(),
            &agg_sig,
        )
        .unwrap();
    }

    #[test]
    fn test_crypto() {
        let message = Bytes::from("cedfrtrrt");
        let hash = DefaultCrypto::hash(&message);

        let pri_key = "0xeba570f6b2cabd67aede56941baa6cc66729bfbf4e15bfd7df805ae0e4f66596";
        let pub_key = "0x02146d3d0a180bd1c1bce9b63caa549dc444d7dc3c875f1a0f171b63059c60bd9d";
        let pri_key = hex_decode(pri_key).unwrap();
        let pub_key = hex_decode(pub_key).unwrap();

        let signature = Secp256k1::sign_message(&hash, &pri_key).unwrap().to_bytes();
        Secp256k1::verify_signature(&hash, &signature, &pub_key).unwrap();
    }
}
