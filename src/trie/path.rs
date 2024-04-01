use bitvec::{order::Msb0, vec::BitVec};
use parity_scale_codec::{Decode, Encode, Error, Input, Output};

use super::merkle_node::Direction;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(all(feature = "std", test))]
use rstest::rstest;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path(pub BitVec<u8, Msb0>);

impl Encode for Path {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        // Copied from scale_bits crate (https://github.com/paritytech/scale-bits/blob/820a3e8e0c9db18ef6acfa2a9a19f738400b0637/src/scale/encode_iter.rs#L28)
        // but don't use it directly to avoid copy and u32 length encoding
        // How it works ?
        // 1. We encode the number of bits in the bitvec as a u8
        // 2. We build elements of a size of u8 using bit shifting
        // 3. A last element, not full, is created if there is a remainder of bits
        let iter = self.0.iter();
        let len = iter.len();
        // SAFETY: len is <= 251
        dest.push_byte(len as u8);
        let mut next_store: u8 = 0;
        let mut pos_in_next_store: u8 = 7;
        for b in iter {
            let bit = match *b {
                true => 1,
                false => 0,
            };
            next_store |= bit << pos_in_next_store;

            if pos_in_next_store == 0 {
                pos_in_next_store = 8;
                dest.push_byte(next_store);
                next_store = 0;
            }
            pos_in_next_store -= 1;
        }

        if pos_in_next_store < 7 {
            dest.push_byte(next_store);
        }
    }

    fn size_hint(&self) -> usize {
        // Inspired from scale_bits crate but don't use it to avoid copy and u32 length encoding
        1 + (self.0.len() + 7) / 8
    }
}

impl Decode for Path {
    fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
        // Inspired from scale_bits crate but don't use it to avoid copy and u32 length encoding
        // SAFETY: len is <= 251
        let len: u8 = input.read_byte()?;
        let mut remaining_bits = len as usize;
        let mut current_byte = None;
        let mut bit = 7;
        let mut bits = BitVec::<u8, Msb0>::new();
        // No bits left to decode; we're done.
        while remaining_bits != 0 {
            // Get the next store entry to pull from:
            let store = match current_byte {
                Some(store) => store,
                None => {
                    let store = match input.read_byte() {
                        Ok(s) => s,
                        Err(e) => return Err(e),
                    };
                    current_byte = Some(store);
                    store
                }
            };

            // Extract a bit:
            let res = match (store >> bit) & 1 {
                0 => false,
                1 => true,
                _ => unreachable!("Can only be 0 or 1 owing to &1"),
            };
            bits.push(res);

            // Update records for next bit:
            remaining_bits -= 1;
            if bit == 0 {
                current_byte = None;
                bit = 8;
            }
            bit -= 1;
        }
        Ok(Self(bits))
    }
}

impl Path {
    pub(crate) fn new_with_direction(&self, direction: Direction) -> Path {
        let mut path = self.0.clone();
        path.push(direction.into());
        Path(path)
    }
}

/// Convert Path to Vec<u8> can be used, for example, to create keys for the database
impl From<Path> for Vec<u8> {
    fn from(path: Path) -> Self {
        let key = if path.0.is_empty() {
            Vec::new()
        } else {
            [&[path.0.len() as u8], path.0.as_raw_slice()].concat()
        };
        key
    }
}

impl From<&Path> for Vec<u8> {
    fn from(path: &Path) -> Self {
        let key = if path.0.is_empty() {
            Vec::new()
        } else {
            [&[path.0.len() as u8], path.0.as_raw_slice()].concat()
        };
        key
    }
}

#[cfg(all(feature = "std", test))]
#[rstest]
#[case(&[0b10101010, 0b10101010])]
#[case(&[])]
#[case(&[0b10101010])]
#[case(&[0b00000000])]
#[case(&[0b11111111])]
#[case(&[0b11111111, 0b00000000, 0b10101010, 0b10101010, 0b11111111, 0b00000000, 0b10101010, 0b10101010, 0b11111111, 0b00000000, 0b10101010, 0b10101010])]
fn test_shared_path_encode_decode(#[case] input: &[u8]) {
    let path = Path(BitVec::<u8, Msb0>::from_slice(input));
    let mut encoded = Vec::new();
    path.encode_to(&mut encoded);

    let decoded = Path::decode(&mut &encoded[..]).unwrap();
    assert_eq!(path, decoded);
}
