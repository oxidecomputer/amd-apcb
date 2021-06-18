
use crate::types::{Buffer, ReadOnlyBuffer};

#[derive(Debug)]
pub struct TokensEntryBodyItem<BufferType> {
    buf: BufferType,
}

impl<BufferType> TokensEntryBodyItem<BufferType> {
    pub(crate) fn new(buf: BufferType) -> Self {
        Self {
            buf
        }
    }
}
