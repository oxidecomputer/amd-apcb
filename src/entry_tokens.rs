
use crate::types::{Buffer, ReadOnlyBuffer};

#[derive(Debug)]
pub struct TokensEntryBodyItem<BufferType> {
    type_id: u16,
    buf: BufferType,
}

impl<BufferType> TokensEntryBodyItem<BufferType> {
    pub(crate) fn new(type_id: u16, buf: BufferType) -> Self {
        Self {
            type_id,
            buf
        }
    }
}
