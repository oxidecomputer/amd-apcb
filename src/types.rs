pub(crate) type
    Buffer<'a> = &'a mut [u8];

pub(crate) type
    ReadOnlyBuffer<'a> = &'a [u8];

#[derive(Debug)]
pub enum Error {
    FileSystemError(&'static str, &'static str), // message, field name
    OutOfSpaceError,
    GroupNotFoundError,
    EntryNotFoundError,
    EntryTypeMismatchError,
    TokenNotFoundError,
}

pub type Result<Q> = core::result::Result<Q, Error>;
