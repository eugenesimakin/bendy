use std::{
    collections::{BTreeMap, HashMap, LinkedList, VecDeque},
    hash::Hash,
};

use crate::encoding::{Encoder, Error, SingleItemEncoder};

/// An object that can be encoded into a single bencode object
pub trait Encodable {
    /// The maximum depth that this object could encode to. Leaves do not consume a level, so an
    /// `i1e` has depth 0 and `li1ee` has depth 1.
    const MAX_DEPTH: usize;

    /// Encode this object into the bencode stream
    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error>;

    /// Encode this object to a byte string
    fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut encoder = Encoder::new().with_max_depth(Self::MAX_DEPTH);
        encoder.emit_with(|e| self.encode(e).map_err(Error::into))?;

        let bytes = encoder.get_output()?;
        Ok(bytes)
    }
}

/// Wrapper to allow `Vec<u8>` encoding as bencode string element.
#[derive(Clone, Copy, Debug, Default, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct AsString<I>(pub I);

// Forwarding impls
impl<'a, E: 'a + Encodable + Sized> Encodable for &'a E {
    const MAX_DEPTH: usize = E::MAX_DEPTH;

    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
        E::encode(self, encoder)
    }
}

impl<E: Encodable> Encodable for Box<E> {
    const MAX_DEPTH: usize = E::MAX_DEPTH;

    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
        E::encode(&*self, encoder)
    }
}

impl<E: Encodable> Encodable for ::std::rc::Rc<E> {
    const MAX_DEPTH: usize = E::MAX_DEPTH;

    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
        E::encode(&*self, encoder)
    }
}

impl<E: Encodable> Encodable for ::std::sync::Arc<E> {
    const MAX_DEPTH: usize = E::MAX_DEPTH;

    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
        E::encode(&*self, encoder)
    }
}

// Base type impls
impl<'a> Encodable for &'a str {
    const MAX_DEPTH: usize = 0;

    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
        encoder.emit_str(self).map_err(Error::from)
    }
}

impl Encodable for String {
    const MAX_DEPTH: usize = 0;

    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
        encoder.emit_str(self).map_err(Error::from)
    }
}

macro_rules! impl_encodable_integer {
    ($($type:ty)*) => {$(
        impl Encodable for $type {
            const MAX_DEPTH: usize = 1;

            fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
                encoder.emit_int(*self).map_err(Error::from)
            }
        }
    )*}
}

impl_encodable_integer!(u8 u16 u32 u64 usize i8 i16 i32 i64 isize);

macro_rules! impl_encodable_iterable {
    ($($type:ident)*) => {$(
        impl <ContentT> Encodable for $type<ContentT>
        where
            ContentT: Encodable
        {
            const MAX_DEPTH: usize = ContentT::MAX_DEPTH + 1;

            fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
                encoder.emit_list(|e| {
                    for item in self {
                        e.emit(item)?;
                    }
                    Ok(())
                })?;

                Ok(())
            }
        }
    )*}
}

impl_encodable_iterable!(Vec VecDeque LinkedList);

impl<'a, ContentT> Encodable for &'a [ContentT]
where
    ContentT: Encodable,
{
    const MAX_DEPTH: usize = ContentT::MAX_DEPTH + 1;

    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
        encoder.emit_list(|e| {
            for item in *self {
                e.emit(item)?;
            }
            Ok(())
        })?;

        Ok(())
    }
}

impl<K: AsRef<[u8]>, V: Encodable> Encodable for BTreeMap<K, V> {
    const MAX_DEPTH: usize = V::MAX_DEPTH + 1;

    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
        encoder.emit_dict(|mut e| {
            for (k, v) in self {
                e.emit_pair(k.as_ref(), v)?;
            }
            Ok(())
        })?;

        Ok(())
    }
}

impl<K, V, S> Encodable for HashMap<K, V, S>
where
    K: AsRef<[u8]> + Eq + Hash,
    V: Encodable,
    S: ::std::hash::BuildHasher,
{
    const MAX_DEPTH: usize = V::MAX_DEPTH + 1;

    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
        encoder.emit_dict(|mut e| {
            let mut pairs = self
                .iter()
                .map(|(k, v)| (k.as_ref(), v))
                .collect::<Vec<_>>();
            pairs.sort_by_key(|&(k, _)| k);
            for (k, v) in pairs {
                e.emit_pair(k, v)?;
            }
            Ok(())
        })?;

        Ok(())
    }
}

impl<I> Encodable for AsString<I>
where
    I: AsRef<[u8]>,
{
    const MAX_DEPTH: usize = 1;

    fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
        encoder.emit_bytes(self.0.as_ref())?;
        Ok(())
    }
}

impl<I> AsRef<[u8]> for AsString<I>
where
    I: AsRef<[u8]>,
{
    fn as_ref(&self) -> &'_ [u8] {
        self.0.as_ref()
    }
}

impl<'a, I> From<&'a [u8]> for AsString<I>
where
    I: From<&'a [u8]>,
{
    fn from(content: &'a [u8]) -> Self {
        AsString(I::from(content))
    }
}

#[cfg(test)]
mod test {

    use super::*;

    struct Foo {
        bar: u32,
        baz: Vec<String>,
        qux: Vec<u8>,
    }

    impl Encodable for Foo {
        const MAX_DEPTH: usize = 2;

        fn encode(&self, encoder: SingleItemEncoder) -> Result<(), Error> {
            encoder.emit_dict(|mut e| {
                e.emit_pair(b"bar", &self.bar)?;
                e.emit_pair(b"baz", &self.baz)?;
                e.emit_pair(b"qux", AsString(&self.qux))?;
                Ok(())
            })?;

            Ok(())
        }
    }

    #[test]
    fn simple_encodable_works() {
        let mut encoder = Encoder::new();
        encoder
            .emit(Foo {
                bar: 5,
                baz: vec!["foo".to_owned(), "bar".to_owned()],
                qux: b"qux".to_vec(),
            })
            .unwrap();
        assert_eq!(
            &encoder.get_output().unwrap()[..],
            &b"d3:bari5e3:bazl3:foo3:bare3:qux3:quxe"[..]
        );
    }
}
