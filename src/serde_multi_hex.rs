use std::{fmt, iter::FromIterator};

use bytes::Bytes;
use derive_more::Constructor;
use serde::{de, ser::SerializeStruct, Deserializer, Serializer};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct TWrapper {
    #[serde(with = "super::serde_hex")]
    inner: Bytes,
}

#[derive(Constructor, Serialize)]
struct VecT {
    inner: Vec<TWrapper>,
}

pub fn serialize<S>(val: &[Bytes], s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let inner = val
        .iter()
        .map(|t| TWrapper {
            inner: t.to_owned(),
        })
        .collect::<Vec<TWrapper>>();

    let vec_t = VecT { inner };

    let mut state = s.serialize_struct("VecT", 1)?;
    state.serialize_field("inner", &vec_t.inner)?;
    state.end()
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Bytes>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(field_identifier, rename_all = "lowercase")]
    enum Field {
        Inner,
    }

    struct VecTVisitor;

    impl<'de> de::Visitor<'de> for VecTVisitor {
        type Value = VecT;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("serde multi")
        }

        fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
        where
            V: de::SeqAccess<'de>,
        {
            let inner = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(0, &self))?;

            Ok(VecT::new(inner))
        }

        fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
        where
            V: de::MapAccess<'de>,
        {
            let mut inner = None;

            while let Some(key) = map.next_key()? {
                match key {
                    Field::Inner => {
                        if inner.is_some() {
                            return Err(de::Error::duplicate_field("inner"));
                        }
                        inner = Some(map.next_value()?);
                    }
                }
            }

            let inner = inner.ok_or_else(|| de::Error::missing_field("inner"))?;
            Ok(VecT::new(inner))
        }
    }

    const FIELDS: &[&str] = &["inner"];
    let vec_t = deserializer.deserialize_struct("VecT", FIELDS, VecTVisitor)?;

    Ok(Vec::from_iter(
        vec_t.inner.into_iter().map(|wrap_t| wrap_t.inner),
    ))
}
