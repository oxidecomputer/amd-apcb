
use crate::types::{Buffer, ReadOnlyBuffer};

#[derive(Debug)]
pub struct TokensEntryBodyItem<BufferType> {
    buf: BufferType,
}
