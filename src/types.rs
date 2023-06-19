// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::ondisk::TokenEntryId;

#[derive(Debug)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
#[non_exhaustive]
pub enum FileSystemError {
    #[cfg_attr(feature = "std", error("inconsistent header"))]
    InconsistentHeader,
    #[cfg_attr(feature = "std", error("payload too big"))]
    PayloadTooBig,
}

#[derive(Debug)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
#[non_exhaustive]
pub enum Error {
    #[cfg_attr(feature = "std", error("arithmetic overflow"))]
    ArithmeticOverflow,
    #[cfg_attr(feature = "std", error("file system error {0}: {1}"))]
    FileSystem(FileSystemError, &'static str), // message, field name
    #[cfg_attr(feature = "std", error("out of space"))]
    OutOfSpace,
    #[cfg_attr(feature = "std", error("group not found"))]
    GroupNotFound,
    #[cfg_attr(feature = "std", error("group unique key violation"))]
    GroupUniqueKeyViolation,
    #[cfg_attr(feature = "std", error("group type mismatch"))]
    GroupTypeMismatch,
    #[cfg_attr(feature = "std", error("entry not found"))]
    EntryNotFound,
    #[cfg_attr(feature = "std", error("entry unique key violation"))]
    EntryUniqueKeyViolation,
    #[cfg_attr(feature = "std", error("entry type mismatch"))]
    EntryTypeMismatch,
    #[cfg_attr(feature = "std", error("token not found"))]
    TokenNotFound,
    #[cfg_attr(feature = "std", error("token ordering violation"))]
    TokenOrderingViolation,
    #[cfg_attr(feature = "std", error("token unique key violation"))]
    TokenUniqueKeyViolation,
    #[cfg_attr(feature = "std", error("token range"))]
    TokenRange,
    #[cfg_attr(feature = "std", error("token entry {entry_id:?} token {token_id} is incompatible with ABL version {abl0_version}"))]
    TokenVersionMismatch {
        entry_id: TokenEntryId,
        token_id: u32,
        abl0_version: u32,
    },
    #[cfg_attr(feature = "std", error("parameter not found"))]
    ParameterNotFound,
    #[cfg_attr(feature = "std", error("parameter range"))]
    ParameterRange,
    // Errors used only for Serde
    #[cfg_attr(feature = "std", error("entry not extractable"))]
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
