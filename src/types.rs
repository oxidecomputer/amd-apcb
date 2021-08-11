
#[derive(Debug)]
#[non_exhaustive]
pub enum FileSystemError {
    InconsistentHeader,
    PayloadTooBig,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    ArithmeticOverflow,
    Internal,
    FileSystem(FileSystemError, &'static str), // message, field name
    OutOfSpace,
    GroupNotFound,
    EntryNotFound,
    EntryTypeMismatch,
    TokenNotFound,
}

pub type Result<Q> = core::result::Result<Q, Error>;
