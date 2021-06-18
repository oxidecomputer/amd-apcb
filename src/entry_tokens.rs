
use crate::types::{Buffer, ReadOnlyBuffer, Result};

#[derive(Debug)]
pub struct TokensEntryBodyItem<BufferType> {
    type_id: u16,
    buf: BufferType,
}

impl<BufferType> TokensEntryBodyItem<BufferType> {
    pub(crate) fn new(type_id: u16, buf: BufferType) -> Result<Self> {
        Ok(Self {
            type_id,
            buf
        })
    }
}
