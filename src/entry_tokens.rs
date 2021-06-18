
use crate::types::{Buffer, ReadOnlyBuffer, Result, Error};
use crate::ondisk::TokenType;
use num_traits::FromPrimitive;

#[derive(Debug)]
pub struct TokensEntryBodyItem<BufferType> {
    type_id: TokenType,
    buf: BufferType,
}

impl<BufferType> TokensEntryBodyItem<BufferType> {
    pub(crate) fn new(type_id: u16, buf: BufferType) -> Result<Self> {
        Ok(Self {
            type_id: TokenType::from_u16(type_id).ok_or_else(|| Error::FileSystemError("type_id of token is unknown", "APCB_TYPE_HEADER::type_id"))?,
            buf
        })
    }
}
