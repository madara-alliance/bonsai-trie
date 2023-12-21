// This file is there just to make Felts serializable with the parity-scale-codec crate.

use parity_scale_codec::{Decode, Encode, Error, Input, Output};
use starknet_types_core::felt::Felt;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FeltWrapper(pub Felt);

impl Encode for FeltWrapper {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        dest.write(&self.0.to_bytes_be());
    }
}

impl Decode for FeltWrapper {
    fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
        let mut buf: [u8; 32] = [0; 32];
        input.read(&mut buf)?;
        Ok(FeltWrapper(Felt::from_bytes_be(&buf)))
    }
}
