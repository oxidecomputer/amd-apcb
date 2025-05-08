// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::naples::ParameterTokenConfig;
use crate::ondisk::BoardInstances;
use crate::ondisk::EntryId;
use crate::ondisk::GroupId;
use crate::ondisk::TokenEntryId;

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
#[non_exhaustive]
pub enum FileSystemError {
    #[cfg_attr(feature = "std", error("inconsistent header"))]
    InconsistentHeader,
    #[cfg_attr(feature = "std", error("payload too big"))]
    PayloadTooBig,
}

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
#[non_exhaustive]
pub enum Error {
    #[cfg_attr(feature = "std", error("arithmetic overflow"))]
    ArithmeticOverflow,
    #[cfg_attr(feature = "std", error("file system error {0}: {1}"))]
    FileSystem(FileSystemError, &'static str), // message, field name
    #[cfg_attr(feature = "std", error("out of space"))]
    OutOfSpace,
    #[cfg_attr(feature = "std", error("group not found - group: {group_id:?}"))]
    #[non_exhaustive]
    GroupNotFound { group_id: GroupId },
    #[cfg_attr(
        feature = "std",
        error("group unique key violation - group: {group_id:?}")
    )]
    #[non_exhaustive]
    GroupUniqueKeyViolation { group_id: GroupId },
    #[cfg_attr(
        feature = "std",
        error(
            "group type mismatch - group: {group_id:?}, signature: {signature:?}"
        )
    )]
    #[non_exhaustive]
    GroupTypeMismatch { group_id: GroupId, signature: [u8; 4] },
    #[cfg_attr(
        feature = "std",
        error(
            "entry not found - entry: {entry_id:?}, instance: {instance_id:#04x}, board mask: {board_instance_mask:?}"
        )
    )]
    #[non_exhaustive]
    EntryNotFound {
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    },
    #[cfg_attr(
        feature = "std",
        error(
            "entry unique key violation - entry: {entry_id:?}, instance: {instance_id:#04x}, board mask: {board_instance_mask:?}"
        )
    )]
    #[non_exhaustive]
    EntryUniqueKeyViolation {
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    },
    #[cfg_attr(feature = "std", error("entry type mismatch"))]
    EntryTypeMismatch,
    #[cfg_attr(feature = "std", error("entry range"))]
    EntryRange,
    #[cfg_attr(feature = "std", error("token not found"))]
    #[non_exhaustive]
    TokenNotFound {
        token_id: u32,
        //entry_id: TokenEntryId,
        //instance_id: u16,
        //board_instance_mask: BoardInstances,
    },
    #[cfg_attr(feature = "std", error("token ordering violation"))]
    TokenOrderingViolation,
    #[cfg_attr(
        feature = "std",
        error(
            "token unique key violation - entry: {entry_id:?}, instance: {instance_id:#04x}, board mask: {board_instance_mask:?}, token: {token_id:#08x}"
        )
    )]
    #[non_exhaustive]
    TokenUniqueKeyViolation {
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        token_id: u32,
    },
    #[cfg_attr(feature = "std", error("token range - token {token_id:#08x}"))]
    #[non_exhaustive]
    TokenRange { token_id: u32 },
    #[cfg_attr(
        feature = "std",
        error(
            "token entry {entry_id:?} token {token_id:#08x} is incompatible with ABL version {abl0_version:#08x}"
        )
    )]
    #[non_exhaustive]
    TokenVersionMismatch {
        entry_id: TokenEntryId,
        token_id: u32,
        abl0_version: u32,
    },
    #[cfg_attr(feature = "std", error("parameter not found"))]
    #[non_exhaustive]
    ParameterNotFound { parameter_id: ParameterTokenConfig },
    #[cfg_attr(feature = "std", error("parameter range"))]
    ParameterRange,
    // Errors used only for Serde
    #[cfg_attr(feature = "std", error("entry not extractable"))]
    EntryNotExtractable,
    #[cfg_attr(feature = "std", error("context mismatch"))]
    ContextMismatch,
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

// Note: The integer is 0x100 * MemDfeSearchElement.header_size + 0x10000 *
// MemDfeSearchElement.payload_size + 0x1000000 *
// MemDfeSearchElement.payload_ext_size
// and is mostly for debugging
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq)] // FIXME: Remove Copy?
pub enum MemDfeSearchVersion {
    /// At least Genoa 1.0.0.? or lower.
    Genoa1 = 0x000c08,
    /// At least Genoa 1.0.0.8 or higher.
    Genoa2 = 0x0c0c08,
    /// At least Raphael AM5 1.7.0 or higher.
    /// At least Granite Ridge AM5 1.7.0 or higher.
    // At least Fire Range 0.0.6.0 or higher.
    Turin1 = 0x0c0c0c,
}

#[derive(Copy, Clone, Debug, Default)] // TODO: Remove Copy?
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ApcbContext {
    #[cfg_attr(feature = "serde", serde(default))]
    mem_dfe_search_version: Option<MemDfeSearchVersion>,
}

impl ApcbContext {
    pub fn builder() -> Self {
        Self::default()
    }
    pub fn mem_dfe_search_version(&self) -> Option<MemDfeSearchVersion> {
        self.mem_dfe_search_version
    }
    pub fn with_mem_dfe_search_version(
        &mut self,
        value: Option<MemDfeSearchVersion>,
    ) -> &mut Self {
        self.mem_dfe_search_version = value;
        self
    }
    pub fn build(&self) -> Self {
        *self
    }
}
