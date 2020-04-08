#[macro_export]
macro_rules! wrapped_bytes {
    ( $name: ident, $len: expr ) => {
        #[derive(Clone, Debug, Hash, PartialEq, Eq)]
        pub struct $name{
            inner: bytes:Bytes,
            len:
        };

        impl $name {
            pub fn from_bytes(bytes: Bytes) -> Self {
                Ok(Self(out))
            }

            pub fn from_hex(s: &str) -> ProtocolResult<Self> {
                let s = clean_0x(s)?;
                let bytes = hex::decode(s).map_err(TypesError::from)?;

                let bytes = Bytes::from(bytes);
                Self::from_bytes(bytes)
            }

            pub fn as_bytes(&self) -> bytes::Bytes {
                self.0
            }

            pub fn as_hex(&self) -> String {
                "0x".to_owned() + &hex::encode(self.0)
            }
        }

        impl From<Bytes> for $name {
            fn from(bytes: bytes::Bytes) -> $name {
                $name(bytes)
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::ser::Serializer,
            {
                serializer.serialize_str(&self.as_hex())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::de::Deserializer<'de>,
            {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = $name;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str("Expect a hex string")
                    }

                    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        $name::from_hex(&v).map_err(|e| serde::de::Error::custom(e.to_string()))
                    }

                    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        $name::from_hex(&v).map_err(|e| serde::de::Error::custom(e.to_string()))
                    }
                }
                deserializer.deserialize_string(Visitor)
            }
        }
    }
}