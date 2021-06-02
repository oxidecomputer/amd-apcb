use crate::ondisk::TokenEntryId;

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
    GroupTypeMismatch,
    EntryNotFound,
    EntryUniqueKeyViolation,
    EntryTypeMismatch,
    TokenNotFound,
    TokenOrderingViolation,
    TokenUniqueKeyViolation,
    TokenRange,
    TokenVersionMismatch {
        entry_id: TokenEntryId,
        token_id: u32,
        abl0_version: u32,
    },
    ParameterNotFound,
    ParameterRange,
    // Errors used only for Serde
    EntryNotExtractable,
}

pub type Result<Q> = core::result::Result<Q, Error>;

pub enum PriorityLevel {
    HardForce,
    High,
    Medium,
    EventLogging,
    Low,
    Normal, // the default
}

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(feature = "std")]
pub(crate) type PtrMut<'a, T> = Cow<'a, T>;

#[cfg(not(feature = "std"))]
pub(crate) type PtrMut<'a, T> = &'a mut T;
