
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
    FileSystem(FileSystemError, &'static str), // message, field name
    OutOfSpace,
    GroupNotFound,
    GroupUniqueKeyViolation,
    EntryNotFound,
    EntryTypeMismatch,
    TokenNotFound,
    TokenRange,
}

pub type Result<Q> = core::result::Result<Q, Error>;

pub enum PriorityLevel {
    HardForce,
    High,
    Medium,
    EventLogging,
    Low,
    Default,
}
