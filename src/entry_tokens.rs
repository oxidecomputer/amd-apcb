
use crate::types::{Buffer, ReadOnlyBuffer, Result, Error};
use crate::ondisk::TokenType;
use num_traits::FromPrimitive;

#[derive(Debug)]
pub struct TokensEntryBodyItem<BufferType> {
    unit_size: u8,
    type_id: TokenType,
    buf: BufferType,
}

impl<BufferType> TokensEntryBodyItem<BufferType> {
    pub(crate) fn new(unit_size: u8, type_id: u16, buf: BufferType) -> Result<Self> {
        if unit_size != 8 {
            return Err(Error::FileSystemError("unit_size of token is unknown", "APCB_TYPE_HEADER::unit_size"));
        }
        Ok(Self {
            unit_size,
            type_id: TokenType::from_u16(type_id).ok_or_else(|| Error::FileSystemError("type_id of token is unknown", "APCB_TYPE_HEADER::type_id"))?,
            buf
        })
    }
}
