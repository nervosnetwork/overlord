use std::fmt;

use bytes::Bytes;
use hummer::coding::{hex_decode, hex_encode};
use serde::{de, Deserializer, Serializer};

/// serialize Bytes with hex
pub fn serialize<S>(val: &Bytes, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(&hex_encode(val))
}

struct StringVisit;

/// deserialize Bytes with hex
pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(StringVisit)
}

impl<'de> de::Visitor<'de> for StringVisit {
    type Value = Bytes;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("byte array")
    }

    #[inline]
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let value = hex_decode(v).map_err(de::Error::custom)?;
        Ok(Bytes::from(value))
    }
}
