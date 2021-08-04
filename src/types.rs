#[derive(Debug)]
pub enum Error {
    ArithmeticOverflow,
    Internal,
    FileSystem(&'static str, &'static str), // message, field name
    OutOfSpace,
    GroupNotFound,
    EntryNotFound,
    EntryTypeMismatch,
    TokenNotFound,
}

pub type Result<Q> = core::result::Result<Q, Error>;
