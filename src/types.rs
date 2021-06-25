pub(crate) type
    Buffer<'a> = &'a mut [u8];

pub(crate) type
    ReadOnlyBuffer<'a> = &'a [u8];

#[derive(Debug)]
pub enum Error {
    FileSystem(&'static str, &'static str), // message, field name
    OutOfSpace,
    GroupNotFound,
    EntryNotFound,
    EntryTypeMismatch,
    TokenNotFound,
}

pub type Result<Q> = core::result::Result<Q, Error>;
