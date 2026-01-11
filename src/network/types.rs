use reticulum::destination::{DestinationDesc, DestinationName};
use reticulum::hash::AddressHash;
use reticulum::identity::Identity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityInfo {
    #[serde(with = "hex_bytes_32")]
    pub public_key: [u8; 32],
    #[serde(with = "hex_bytes_32")]
    pub verifying_key: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    #[serde(with = "hex_bytes_16")]
    pub hash: [u8; 16],
    pub name: String,
    pub identity: IdentityInfo,
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub hash: [u8; 16],
    pub name: Option<String>,
    pub identity: IdentityInfo,
}

impl NodeInfo {
    pub fn to_destination_desc(&self) -> DestinationDesc {
        let identity =
            Identity::new_from_slices(&self.identity.public_key, &self.identity.verifying_key);
        let address_hash = AddressHash::from_bytes(&self.hash);
        let name = DestinationName::new("nomadnetwork", "node");

        DestinationDesc {
            identity,
            address_hash,
            name,
        }
    }

    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash)
    }
}

impl PeerInfo {
    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash)
    }
}

mod hex_bytes_16 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 16], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 16], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let mut arr = [0u8; 16];
        if bytes.len() != 16 {
            return Err(serde::de::Error::custom("expected 16 bytes"));
        }
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}

mod hex_bytes_32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let mut arr = [0u8; 32];
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("expected 32 bytes"));
        }
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}
