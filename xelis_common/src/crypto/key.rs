use crate::api::DataElement;
use crate::utils::get_network;
use crate::serializer::{Reader, ReaderError, Serializer, Writer};
use super::address::{Address, AddressType};
use super::hash::Hash;
use std::cmp::Ordering;
use std::fmt::{Display, Error, Formatter};
use rand::{rngs::OsRng, RngCore};
use std::hash::Hasher;

pub const KEY_LENGTH: usize = 32;
pub const SIGNATURE_LENGTH: usize = 64;

#[derive(Clone, Eq, Debug)]
pub struct PublicKey(ed25519_dalek::PublicKey);
pub struct PrivateKey(ed25519_dalek::SecretKey);

#[derive(Clone, Debug)]
pub struct Signature(ed25519_dalek::Signature); // ([u8; SIGNATURE_LENGTH]);

pub struct KeyPair {
    public_key: PublicKey,
    private_key: PrivateKey
}

impl PublicKey {
    // Generate a random public key
    pub fn random() -> Self {
        KeyPair::new().public_key
    }

    // Verify the signature of the hash with the public key
    pub fn verify_signature(&self, hash: &Hash, signature: &Signature) -> bool {
        use ed25519_dalek::Verifier;
        self.0.verify(hash.as_bytes(), &signature.0).is_ok()
    }

    // Returns the bytes representig the public key
    pub fn as_bytes(&self) -> &[u8; KEY_LENGTH] {
        self.0.as_bytes()
    }

    // Convert the public key to human readable address with selected network
    pub fn to_address(&self, mainnet: bool) -> Address {
        Address::new(mainnet, AddressType::Normal, self.clone())
    }

    // Convert the public key to human readable address with selected network and data
    pub fn to_address_with(&self, mainnet: bool, data: DataElement) -> Address {
        Address::new(mainnet, AddressType::Data(data), self.clone())
    }
}

impl PrivateKey {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(ed25519_dalek::SecretKey::from_bytes(bytes).unwrap())
    }
}

impl Serializer for PublicKey {
    fn write(&self, writer: &mut Writer) {
        writer.write_bytes(self.as_bytes());
    }

    fn read(reader: &mut Reader) -> Result<Self, ReaderError> {
        match ed25519_dalek::PublicKey::from_bytes(&reader.read_bytes_32()?) {
            Ok(v) => Ok(PublicKey(v)),
            Err(_) => return Err(ReaderError::ErrorTryInto)
        }
    }
}

impl PartialEq for PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialOrd for PublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.as_bytes().cmp(other.as_bytes()))
    }
}

impl Ord for PublicKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl std::hash::Hash for PublicKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl serde::Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_address(get_network().is_mainnet()).to_string())
    }
}

impl<'de> serde::Deserialize<'de> for PublicKey {
    fn deserialize<D: serde::Deserializer<'de> >(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let address = Address::from_string(&s).map_err(serde::de::Error::custom)?;
        Ok(address.to_public_key())
    }
}

impl Display for PublicKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", &self.to_address(get_network().is_mainnet()))
    }
}

impl PrivateKey {
    pub fn sign(&self, data: &[u8], public_key: &PublicKey) -> Signature {
        let expanded_key: ed25519_dalek::ExpandedSecretKey = (&self.0).into();
        Signature(expanded_key.sign(data, &public_key.0))
    }
}

impl Serializer for PrivateKey {
    fn write(&self, writer: &mut Writer) {
        writer.write_bytes(self.0.as_bytes());
    }

    fn read(reader: &mut Reader) -> Result<Self, ReaderError> {
        let bytes: [u8; KEY_LENGTH] = reader.read_bytes(KEY_LENGTH)?;
        let secret_key = ed25519_dalek::SecretKey::from_bytes(&bytes).expect("invalid private key bytes");
        Ok(PrivateKey(secret_key))
    }
}

impl KeyPair {
    pub fn new() -> Self {
        let mut csprng = OsRng {};
        let mut bytes = [0u8; KEY_LENGTH];
        csprng.fill_bytes(&mut bytes);
        let secret_key: ed25519_dalek::SecretKey = ed25519_dalek::SecretKey::from_bytes(&bytes).expect("invalid secret key generated bytes");
        let public_key: ed25519_dalek::PublicKey = (&secret_key).into();

        KeyPair {
            public_key: PublicKey(public_key),
            private_key: PrivateKey(secret_key)
        }
    }

    pub fn from_private_key(private_key: PrivateKey) -> Self {
        let public_key: ed25519_dalek::PublicKey  = (&private_key.0).into();
        Self {
            public_key: PublicKey(public_key),
            private_key
        }
    }

    pub fn from_keys(public_key: PublicKey, private_key: PrivateKey) -> Self {
        KeyPair {
            public_key,
            private_key
        }
    }

    pub fn get_public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn get_private_key(&self) -> &PrivateKey {
        &self.private_key
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        self.private_key.sign(data, &self.public_key)
    }
}

impl Serializer for KeyPair {
    fn write(&self, writer: &mut Writer) {
        self.public_key.write(writer);
        self.private_key.write(writer);
    }

    fn read(reader: &mut Reader) -> Result<Self, ReaderError> {
        let public_key = PublicKey::read(reader)?;
        let private_key = PrivateKey::read(reader)?;

        Ok(Self::from_keys(public_key, private_key))
    }
}

impl Signature {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl Serializer for Signature {
    fn write(&self, writer: &mut Writer) {
        writer.write_bytes(&self.0.to_bytes());
    }

    fn read(reader: &mut Reader) -> Result<Self, ReaderError> {
        let signature = match ed25519_dalek::Signature::from_bytes(&reader.read_bytes_64()?) {
            Ok(v) => v,
            Err(_) => return Err(ReaderError::ErrorTryInto)
        };
        let signature = Signature(signature);
        Ok(signature)
    }
}

impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl std::hash::Hash for Signature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bytes().hash(state);
    }
}

impl serde::Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for Signature {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).and_then(|s| {
            let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
            let signature = ed25519_dalek::Signature::from_bytes(&bytes).map_err(serde::de::Error::custom)?;
            Ok(Signature(signature))
        })
    }
}

impl Display for Signature {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", &self.to_hex())
    }
}