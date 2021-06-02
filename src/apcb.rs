use crate::types::{Error, FileSystemError, PtrMut, Result};

use crate::entry::EntryItemBody;
use crate::group::{GroupItem, GroupMutItem};
use crate::ondisk::GroupId;
use crate::ondisk::ENTRY_ALIGNMENT;
use crate::ondisk::ENTRY_HEADER;
use crate::ondisk::GROUP_HEADER;
use crate::ondisk::TOKEN_ENTRY;
use crate::ondisk::V2_HEADER;
use crate::ondisk::V3_HEADER_EXT;
use crate::ondisk::{
    take_body_from_collection, take_body_from_collection_mut,
    take_header_from_collection, take_header_from_collection_mut,
    HeaderWithTail, ParameterAttributes, SequenceElementAsBytes,
};
pub use crate::ondisk::{
    BoardInstances, ContextFormat, ContextType, EntryCompatible, EntryId,
    Parameter, PriorityLevels,
};
use crate::token_accessors::{Tokens, TokensMut};
use core::convert::TryInto;
use core::default::Default;
use core::mem::size_of;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use pre::pre;
use static_assertions::const_assert;
use zerocopy::AsBytes;
use zerocopy::LayoutVerified;

// The following imports are only used for std enviroments and serde.
#[cfg(feature = "std")]
extern crate std;
#[cfg(feature = "serde")]
use crate::entry::{EntryItem, SerdeEntryItem};
#[cfg(feature = "serde")]
use crate::group::SerdeGroupItem;
#[cfg(feature = "serde")]
use serde::de::{Deserialize, Deserializer};
#[cfg(feature = "serde")]
use serde::ser::{Serialize, SerializeStruct, Serializer};
#[cfg(feature = "serde")]
use std::borrow::Cow;

pub struct ApcbIoOptions {
    pub check_checksum: bool,
}

impl Default for ApcbIoOptions {
    fn default() -> Self {
        Self { check_checksum: true }
    }
}

pub struct Apcb<'a> {
    used_size: usize,
    pub backing_store: PtrMut<'a, [u8]>,
}

#[cfg(feature = "serde")]
#[cfg_attr(feature = "serde", derive(Default, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SerdeApcb {
    pub version: String,
    pub header: V2_HEADER,
    pub v3_header_ext: Option<V3_HEADER_EXT>,
    pub groups: Vec<SerdeGroupItem>,
    //#[cfg_attr(feature = "serde", serde(borrow))]
    pub entries: Vec<SerdeEntryItem>,
}

#[cfg(feature = "schemars")]
impl<'a> schemars::JsonSchema for Apcb<'a> {
    fn schema_name() -> std::string::String {
        SerdeApcb::schema_name()
    }
    fn json_schema(
        gen: &mut schemars::gen::SchemaGenerator,
    ) -> schemars::schema::Schema {
        SerdeApcb::json_schema(gen)
    }
    fn is_referenceable() -> bool {
        SerdeApcb::is_referenceable()
    }
}

#[cfg(feature = "serde")]
use core::convert::TryFrom;

#[cfg(feature = "serde")]
impl<'a> TryFrom<SerdeApcb> for Apcb<'a> {
    type Error = Error;
    fn try_from(serde_apcb: SerdeApcb) -> Result<Self> {
        let buf = Cow::from(vec![0xFFu8; Self::MAX_SIZE]);
        let mut apcb = Apcb::create(buf, 42, &ApcbIoOptions::default())?;
        *apcb.header_mut()? = serde_apcb.header;
        match serde_apcb.v3_header_ext {
            Some(v3) => {
                assert!(
                    size_of::<V3_HEADER_EXT>() + size_of::<V2_HEADER>() == 128
                );
                apcb.header_mut()?.header_size.set(128);
                if let Some(mut v) = apcb.v3_header_ext_mut()? {
                    *v = v3;
                }
            }
            None => {
                apcb.header_mut()?
                    .header_size
                    .set(size_of::<V2_HEADER>().try_into().unwrap());
            }
        }
        // We reset apcb_size to header_size as this is naturally extended as we
        // add groups and entries.
        let mut header = apcb.header_mut()?;
        let header_size = header.header_size.get();
        header.apcb_size.set(header_size.into());
        // These groups already exist: We've just successfully parsed them,
        // there's no reason the groupid should be invalid.
        for g in serde_apcb.groups {
            apcb.insert_group(
                GroupId::from_u16(g.header.group_id.get()).unwrap(),
                g.header.signature,
            )?;
        }
        for e in serde_apcb.entries {
            let buf = &e.body[..];
            match apcb.insert_entry(
                EntryId::decode(
                    e.header.group_id.get(),
                    e.header.entry_id.get(),
                ),
                e.header.instance_id.get(),
                BoardInstances::from(e.header.board_instance_mask.get()),
                ContextType::from_u8(e.header.context_type).unwrap(),
                PriorityLevels::from(e.header.priority_mask),
                buf,
            ) {
                Ok(_) => {}
                Err(err) => {
                    // print!("Deserializing entry {e:?} failed with {err:?}");
                    return Err(err);
                }
            };
        }
        apcb.update_checksum()?;
        Ok(apcb)
    }
}

#[cfg(feature = "serde")]
impl<'a> Serialize for Apcb<'a> {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // In a better world we would implement From<Apcb> for SerdeApcb
        // however we can't do that as we'd be returning borrowed data from
        // Apcb.
        let mut state = serializer.serialize_struct("Apcb", 5)?;
        state.serialize_field("version", env!("CARGO_PKG_VERSION"))?;
        let groups = self
            .groups()
            .map_err(|e| serde::ser::Error::custom(format!("{e:?}")))?
            .collect::<Vec<_>>();
        let mut entries: Vec<EntryItem<'_>> = Vec::new();
        for g in &groups {
            entries.extend((g.entries()).collect::<Vec<_>>());
        }
        state.serialize_field(
            "header",
            &*self
                .header()
                .map_err(|_| serde::ser::Error::custom("invalid V2_HEADER"))?,
        )?;
        let v3_header_ext: Option<V3_HEADER_EXT> = self
            .v3_header_ext()
            .map_err(|e| serde::ser::Error::custom(format!("{e:?}")))?
            .as_ref()
            .map(|h| **h);
        state.serialize_field("v3_header_ext", &v3_header_ext)?;
        state.serialize_field("groups", &groups)?;
        state.serialize_field("entries", &entries)?;
        state.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Apcb<'_> {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let sa: SerdeApcb = SerdeApcb::deserialize(deserializer)?;
        Apcb::try_from(sa)
            .map_err(|e| serde::de::Error::custom(format!("{e:?}")))
    }
}

pub struct ApcbIterMut<'a> {
    buf: &'a mut [u8],
    remaining_used_size: usize,
}

pub struct ApcbIter<'a> {
    buf: &'a [u8],
    remaining_used_size: usize,
}

impl<'a> ApcbIterMut<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what
    /// this function does. Note: The caller needs to manually decrease
    /// remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut &'b mut [u8]) -> Result<GroupMutItem<'b>> {
        if buf.is_empty() {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "GROUP_HEADER",
            ));
        }
        let header =
            match take_header_from_collection_mut::<GROUP_HEADER>(&mut *buf) {
                Some(item) => item,
                None => {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "GROUP_HEADER",
                    ));
                }
            };
        let group_size = header.group_size.get() as usize;
        let payload_size = group_size
            .checked_sub(size_of::<GROUP_HEADER>())
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "GROUP_HEADER::group_size",
            ))?;
        let body =
            match take_body_from_collection_mut(&mut *buf, payload_size, 1) {
                Some(item) => item,
                None => {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "GROUP_HEADER",
                    ));
                }
            };
        let body_len = body.len();

        Ok(GroupMutItem { header, buf: body, used_size: body_len })
    }

    /// Moves the point to the group with the given GROUP_ID.  Returns (offset,
    /// group_size) of it.
    pub(crate) fn move_point_to(
        &'_ mut self,
        group_id: GroupId,
    ) -> Result<(usize, usize)> {
        let group_id = group_id.to_u16().unwrap();
        let mut remaining_used_size = self.remaining_used_size;
        let mut offset = 0usize;
        loop {
            let mut buf = &mut self.buf[..remaining_used_size];
            if buf.is_empty() {
                break;
            }
            let group = ApcbIterMut::next_item(&mut buf)?;
            let group_size = group.header.group_size.get();
            if group.header.group_id.get() == group_id {
                return Ok((offset, group_size as usize));
            } else {
                let group = ApcbIterMut::next_item(&mut self.buf)?;
                let group_size = group.header.group_size.get() as usize;
                offset = offset
                    .checked_add(group_size)
                    .ok_or(Error::ArithmeticOverflow)?;
                remaining_used_size = remaining_used_size
                    .checked_sub(group_size)
                    .ok_or(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "GROUP_HEADER::group_size",
                    ))?;
            }
        }
        Err(Error::GroupNotFound)
    }

    pub(crate) fn next1(&mut self) -> Result<GroupMutItem<'a>> {
        if self.remaining_used_size == 0 {
            panic!("Internal error");
        }
        match Self::next_item(&mut self.buf) {
            Ok(e) => {
                let group_size = e.header.group_size.get() as usize;
                if self.remaining_used_size >= group_size {
                } else {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "GROUP_HEADER::group_size",
                    ));
                }
                self.remaining_used_size -= group_size;
                Ok(e)
            }
            Err(e) => Err(e),
        }
    }
}

impl<'a> Iterator for ApcbIterMut<'a> {
    type Item = GroupMutItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match self.next1() {
            Ok(item) => Some(item),
            Err(_) => None,
        }
    }
}

impl<'a> ApcbIter<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what
    /// this function does. Note: The caller needs to manually decrease
    /// remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut &'b [u8]) -> Result<GroupItem<'b>> {
        if buf.is_empty() {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "GROUP_HEADER",
            ));
        }
        let header =
            match take_header_from_collection::<GROUP_HEADER>(&mut *buf) {
                Some(item) => item,
                None => {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "GROUP_HEADER",
                    ));
                }
            };
        let group_size = header.group_size.get() as usize;
        let payload_size = group_size
            .checked_sub(size_of::<GROUP_HEADER>())
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "GROUP_HEADER",
            ))?;
        let body = match take_body_from_collection(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "GROUP_HEADER",
                ));
            }
        };
        let body_len = body.len();

        Ok(GroupItem { header, buf: body, used_size: body_len })
    }
    pub(crate) fn next1(&mut self) -> Result<GroupItem<'a>> {
        if self.remaining_used_size == 0 {
            panic!("Internal error");
        }
        match Self::next_item(&mut self.buf) {
            Ok(e) => {
                let group_size = e.header.group_size.get() as usize;
                if self.remaining_used_size >= group_size {
                } else {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "GROUP_HEADER::group_size",
                    ));
                }
                self.remaining_used_size -= group_size;
                Ok(e)
            }
            Err(e) => Err(e),
        }
    }
    /// Validates the entries (recursively).  Also consumes iterator.
    pub(crate) fn validate(mut self) -> Result<()> {
        while self.remaining_used_size > 0 {
            match self.next1() {
                Ok(item) => {
                    GroupId::from_u16(item.header.group_id.get()).ok_or(
                        Error::FileSystem(
                            FileSystemError::InconsistentHeader,
                            "GROUP_HEADER::group_id",
                        ),
                    )?;
                    item.entries().validate()?;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}

impl<'a> Iterator for ApcbIter<'a> {
    type Item = GroupItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match self.next1() {
            Ok(item) => Some(item),
            Err(_) => None,
        }
    }
}

impl<'a> Apcb<'a> {
    const NAPLES_VERSION: u16 = 0x20;
    const ROME_VERSION: u16 = 0x30;
    pub const MAX_SIZE: usize = 0x2400;

    pub fn header(&self) -> Result<LayoutVerified<&[u8], V2_HEADER>> {
        let (header, _) =
            LayoutVerified::<&[u8], V2_HEADER>::new_unaligned_from_prefix(
                &*self.backing_store,
            )
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER",
            ))?;
        Ok(header)
    }
    pub fn header_mut(
        &mut self,
    ) -> Result<LayoutVerified<&mut [u8], V2_HEADER>> {
        #[cfg(not(feature = "serde"))]
        let bs: &mut [u8] = self.backing_store;
        #[cfg(feature = "serde")]
        let bs: &mut [u8] = self.backing_store.to_mut();
        let (header, _) =
            LayoutVerified::<&mut [u8], V2_HEADER>::new_unaligned_from_prefix(
                bs,
            )
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER",
            ))?;
        Ok(header)
    }
    pub fn v3_header_ext(
        &self,
    ) -> Result<Option<LayoutVerified<&[u8], V3_HEADER_EXT>>> {
        let (header, rest) =
            LayoutVerified::<&[u8], V2_HEADER>::new_unaligned_from_prefix(
                &*self.backing_store,
            )
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER",
            ))?;
        let v3_header_ext = if usize::from(header.header_size)
            == size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>()
        {
            let (header_ext, _) = LayoutVerified::<&[u8], V3_HEADER_EXT>
                ::new_unaligned_from_prefix(rest)
                .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V3_HEADER_EXT",
            ))?;
            Some(header_ext)
        } else {
            None
        };
        Ok(v3_header_ext)
    }
    pub fn v3_header_ext_mut(
        &mut self,
    ) -> Result<Option<LayoutVerified<&mut [u8], V3_HEADER_EXT>>> {
        #[cfg(not(feature = "serde"))]
        let bs: &mut [u8] = self.backing_store;
        #[cfg(feature = "serde")]
        let bs: &mut [u8] = self.backing_store.to_mut();
        let (header, rest) =
            LayoutVerified::<&mut [u8], V2_HEADER>::new_unaligned_from_prefix(
                bs,
            )
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER",
            ))?;
        let v3_header_ext = if usize::from(header.header_size)
            == size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>()
        {
            let (header_ext, _) = LayoutVerified::<&mut [u8], V3_HEADER_EXT>
                ::new_unaligned_from_prefix(rest)
                .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V3_HEADER_EXT",
            ))?;
            Some(header_ext)
        } else {
            None
        };
        Ok(v3_header_ext)
    }
    pub fn beginning_of_groups(&self) -> Result<&'_ [u8]> {
        let mut offset = size_of::<V2_HEADER>();
        if self.v3_header_ext()?.is_some() {
            offset = size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>();
        }
        Ok(&self.backing_store[offset..])
    }
    pub fn beginning_of_groups_mut(&mut self) -> Result<&'_ mut [u8]> {
        let mut offset = size_of::<V2_HEADER>();
        if self.v3_header_ext()?.is_some() {
            offset = size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>();
        }
        #[cfg(feature = "serde")]
        return Ok(&mut self.backing_store.to_mut()[offset..]);
        #[cfg(not(feature = "serde"))]
        Ok(&mut self.backing_store[offset..])
    }
    pub fn groups(&self) -> Result<ApcbIter<'_>> {
        Ok(ApcbIter {
            buf: self.beginning_of_groups()?,
            remaining_used_size: self.used_size,
        })
    }
    pub fn group(&self, group_id: GroupId) -> Result<Option<GroupItem<'_>>> {
        for group in self.groups()? {
            if group.id() == group_id {
                return Ok(Some(group));
            }
        }
        Ok(None)
    }
    /// Validates the contents.
    /// If ABL0_VERSION is Some, also validates against that AGESA
    /// bootloader version.
    pub fn validate(&self, abl0_version: Option<u32>) -> Result<()> {
        self.groups()?.validate()?;
        self.ensure_abl0_compatibility(abl0_version)
    }
    pub fn groups_mut(&mut self) -> Result<ApcbIterMut<'_>> {
        let used_size = self.used_size;
        Ok(ApcbIterMut {
            buf: &mut *self.beginning_of_groups_mut()?,
            remaining_used_size: used_size,
        })
    }
    pub fn group_mut(
        &mut self,
        group_id: GroupId,
    ) -> Result<Option<GroupMutItem<'_>>> {
        for group in self.groups_mut()? {
            if group.id() == group_id {
                return Ok(Some(group));
            }
        }
        Ok(None)
    }
    /// Note: BOARD_INSTANCE_MASK needs to be exact.
    pub fn delete_entry(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    ) -> Result<()> {
        let group_id = entry_id.group_id();
        let mut group =
            self.group_mut(group_id)?.ok_or(Error::GroupNotFound)?;
        let size_diff =
            group.delete_entry(entry_id, instance_id, board_instance_mask)?;
        if size_diff > 0 {
            let size_diff = size_diff as i64;
            self.resize_group_by(group_id, -size_diff)?;
        }
        Ok(())
    }
    fn resize_group_by(
        &mut self,
        group_id: GroupId,
        size_diff: i64,
    ) -> Result<GroupMutItem<'_>> {
        let old_used_size = self.used_size;
        let apcb_size = self.header()?.apcb_size.get();
        if size_diff > 0 {
            let size_diff: u32 = (size_diff as u64)
                .try_into()
                .map_err(|_| Error::ArithmeticOverflow)?;
            self.header_mut()?.apcb_size.set(
                apcb_size.checked_add(size_diff).ok_or(Error::FileSystem(
                    FileSystemError::PayloadTooBig,
                    "HEADER_V2::apcb_size",
                ))?,
            );
        } else {
            let size_diff: u32 = ((-size_diff) as u64)
                .try_into()
                .map_err(|_| Error::ArithmeticOverflow)?;
            self.header_mut()?.apcb_size.set(
                apcb_size.checked_sub(size_diff).ok_or(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "HEADER_V2::apcb_size",
                ))?,
            );
        }

        let self_beginning_of_groups_len = self.beginning_of_groups()?.len();
        let mut groups = self.groups_mut()?;
        let (offset, old_group_size) = groups.move_point_to(group_id)?;
        #[allow(clippy::comparison_chain)]
        if size_diff > 0 {
            // Grow

            let old_group_size: u32 = old_group_size
                .try_into()
                .map_err(|_| Error::ArithmeticOverflow)?;
            let size_diff: u32 = (size_diff as u64)
                .try_into()
                .map_err(|_| Error::ArithmeticOverflow)?;
            let new_group_size = old_group_size.checked_add(size_diff).ok_or(
                Error::FileSystem(
                    FileSystemError::PayloadTooBig,
                    "GROUP_HEADER::group_size",
                ),
            )?;
            let new_used_size = old_used_size
                .checked_add(size_diff as usize)
                .ok_or(Error::FileSystem(
                FileSystemError::PayloadTooBig,
                "ENTRY_HEADER::entry_size",
            ))?;
            if new_used_size <= self_beginning_of_groups_len {
            } else {
                return Err(Error::OutOfSpace);
            }
            let group = groups.next().ok_or(Error::GroupNotFound)?;
            group.header.group_size.set(new_group_size);
            let buf = &mut self.beginning_of_groups_mut()?[offset..];
            if old_group_size as usize > old_used_size {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "GROUP_HEADER::group_size",
                ));
            }
            buf.copy_within(
                (old_group_size as usize)..(old_used_size - offset),
                new_group_size as usize,
            );
            self.used_size = new_used_size;
        } else if size_diff < 0 {
            let old_group_size: u32 = old_group_size
                .try_into()
                .map_err(|_| Error::ArithmeticOverflow)?;
            let size_diff: u32 = ((-size_diff) as u64)
                .try_into()
                .map_err(|_| Error::ArithmeticOverflow)?;
            let new_group_size = old_group_size.checked_sub(size_diff).ok_or(
                Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "GROUP_HEADER::group_size",
                ),
            )?;
            let new_used_size = old_used_size
                .checked_sub(size_diff as usize)
                .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::entry_size",
            ))?;
            let group = groups.next().ok_or(Error::GroupNotFound)?;
            group.header.group_size.set(new_group_size);
            let buf = &mut self.beginning_of_groups_mut()?[offset..];
            buf.copy_within(
                (old_group_size as usize)..old_used_size,
                new_group_size as usize,
            );
            self.used_size = new_used_size;
        }
        self.group_mut(group_id)?.ok_or(Error::GroupNotFound)
    }
    /// Note: board_instance_mask needs to be exact.
    #[allow(clippy::too_many_arguments)]
    #[pre]
    fn internal_insert_entry(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        context_type: ContextType,
        payload_size: usize,
        priority_mask: PriorityLevels,
        payload_initializer: impl Fn(&mut [u8]),
    ) -> Result<()> {
        let group_id = entry_id.group_id();
        let mut group =
            self.group_mut(group_id)?.ok_or(Error::GroupNotFound)?;
        match group.entry_exact_mut(entry_id, instance_id, board_instance_mask)
        {
            None => {}
            _ => {
                return Err(Error::EntryUniqueKeyViolation);
            }
        }

        let mut entry_allocation: u16 = (size_of::<ENTRY_HEADER>() as u16)
            .checked_add(
                payload_size
                    .try_into()
                    .map_err(|_| Error::ArithmeticOverflow)?,
            )
            .ok_or(Error::OutOfSpace)?;
        while entry_allocation % (ENTRY_ALIGNMENT as u16) != 0 {
            entry_allocation += 1;
        }
        let mut group =
            self.resize_group_by(group_id, entry_allocation.into())?;
        let mut entries = group.entries_mut();
        // Note: On some errors, group.used_size will be reduced by insert_entry
        // again!
        match #[assure(
            "Caller already grew the group by `payload_size + size_of::<ENTRY_HEADER>()`",
            reason = "See above"
        )]
        entries.insert_entry(
            entry_id,
            instance_id,
            board_instance_mask,
            entry_allocation,
            context_type,
            payload_size,
            payload_initializer,
            priority_mask,
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    // Security--and it would be nicer if the person using this would instead
    // contribute a struct layout so we can use it normally
    #[pre]
    pub(crate) fn insert_entry(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        context_type: ContextType,
        priority_mask: PriorityLevels,
        payload: &[u8],
    ) -> Result<()> {
        let payload_size = payload.len();
        self.internal_insert_entry(
            entry_id,
            instance_id,
            board_instance_mask,
            context_type,
            payload_size,
            priority_mask,
            |body: &mut [u8]| {
                body.copy_from_slice(payload);
            },
        )
    }

    /// Inserts a new entry (see insert_entry), puts PAYLOAD into it.  Usually
    /// that's for platform_specific_override or platform_tuning structs.
    /// Note: Currently, INSTANCE_ID is always supposed to be 0.
    pub fn insert_struct_sequence_as_entry(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        priority_mask: PriorityLevels,
        payload: &[&dyn SequenceElementAsBytes],
    ) -> Result<()> {
        let mut payload_size: usize = 0;
        for item in payload {
            let blob = item
                .checked_as_bytes(entry_id)
                .ok_or(Error::EntryTypeMismatch)?;
            payload_size = payload_size
                .checked_add(blob.len())
                .ok_or(Error::ArithmeticOverflow)?;
        }
        self.internal_insert_entry(
            entry_id,
            instance_id,
            board_instance_mask,
            ContextType::Struct,
            payload_size,
            priority_mask,
            |body: &mut [u8]| {
                let mut body = body;
                for item in payload {
                    let source = item.checked_as_bytes(entry_id).unwrap();
                    let (a, rest) = body.split_at_mut(source.len());
                    a.copy_from_slice(source);
                    body = rest;
                }
            },
        )
    }

    /// Inserts a new entry (see insert_entry), puts PAYLOAD into it.  T can be
    /// a enum of struct refs (PlatformSpecificElementRef,
    /// PlatformTuningElementRef) or just one struct. Note: Currently,
    /// INSTANCE_ID is always supposed to be 0.
    pub fn insert_struct_array_as_entry<T: EntryCompatible + AsBytes>(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        priority_mask: PriorityLevels,
        payload: &[T],
    ) -> Result<()> {
        let mut payload_size: usize = 0;
        for item in payload {
            let blob = item.as_bytes();
            if !T::is_entry_compatible(entry_id, blob) {
                return Err(Error::EntryTypeMismatch);
            }
            payload_size = payload_size
                .checked_add(blob.len())
                .ok_or(Error::ArithmeticOverflow)?;
        }
        self.internal_insert_entry(
            entry_id,
            instance_id,
            board_instance_mask,
            ContextType::Struct,
            payload_size,
            priority_mask,
            |body: &mut [u8]| {
                let mut body = body;
                for item in payload {
                    let source = item.as_bytes();
                    let (a, rest) = body.split_at_mut(source.len());
                    a.copy_from_slice(source);
                    body = rest;
                }
            },
        )
    }

    /// Inserts a new entry (see insert_entry), puts HEADER and then TAIL into
    /// it.  TAIL is allowed to be &[], and often has to be.
    /// Note: Currently, INSTANCE_ID is always supposed to be 0.
    pub fn insert_struct_entry<
        H: EntryCompatible + AsBytes + HeaderWithTail,
    >(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        priority_mask: PriorityLevels,
        header: &H,
        tail: &[H::TailArrayItemType<'_>],
    ) -> Result<()> {
        let blob = header.as_bytes();
        if H::is_entry_compatible(entry_id, blob) {
            let payload_size = size_of::<H>()
                .checked_add(
                    size_of::<H::TailArrayItemType<'_>>()
                        .checked_mul(tail.len())
                        .ok_or(Error::ArithmeticOverflow)?,
                )
                .ok_or(Error::ArithmeticOverflow)?;
            self.internal_insert_entry(
                entry_id,
                instance_id,
                board_instance_mask,
                ContextType::Struct,
                payload_size,
                priority_mask,
                |body: &mut [u8]| {
                    let mut body = body;
                    let (a, rest) = body.split_at_mut(blob.len());
                    a.copy_from_slice(blob);
                    body = rest;
                    for item in tail {
                        let source = item.as_bytes();
                        let (a, rest) = body.split_at_mut(source.len());
                        a.copy_from_slice(source);
                        body = rest;
                    }
                },
            )
        } else {
            Err(Error::EntryTypeMismatch)
        }
    }

    /// This inserts a Naples-style Parameters entry.
    /// Note: Keep in sync with new_tail_from_vec.
    pub fn insert_parameters_entry(
        &mut self,
        entry_id: EntryId,
        items: &[Parameter],
    ) -> Result<()> {
        let mut payload_size = size_of::<u32>() + size_of::<u8>(); // terminator attribute and its value
        for parameter in items {
            payload_size = payload_size
                .checked_add(size_of::<ParameterAttributes>())
                .ok_or(Error::ArithmeticOverflow)?;
            let value_size = usize::from(parameter.value_size()?);
            let value = parameter.value()?;
            payload_size = payload_size
                .checked_add(value_size)
                .ok_or(Error::ArithmeticOverflow)?;
            if value_size > 8 || value >= (8u64 << value_size) {
                return Err(Error::ParameterRange);
            }
        }
        self.internal_insert_entry(
            entry_id,
            0,
            BoardInstances::new(),
            ContextType::Struct,
            payload_size,
            PriorityLevels::new(),
            |body: &mut [u8]| {
                let mut body = body;
                for parameter in items {
                    let raw_key =
                        parameter.attributes().unwrap().to_u32().unwrap();
                    let (a, rest) = body.split_at_mut(size_of::<u32>());
                    a.copy_from_slice(raw_key.as_bytes());
                    body = rest;
                }
                let key = ParameterAttributes::terminator();
                let raw_key = key.to_u32().unwrap();
                let (a, rest) = body.split_at_mut(size_of::<u32>());
                a.copy_from_slice(raw_key.as_bytes());
                body = rest;

                for parameter in items {
                    let size = usize::from(parameter.value_size().unwrap());
                    let (a, rest) = body.split_at_mut(size);
                    let value = parameter.value().unwrap();
                    let raw_value = value.as_bytes();
                    a.copy_from_slice(&raw_value[0..size]);
                    body = rest;
                }
                let (a, _rest) = body.split_at_mut(size_of::<u8>());
                a.copy_from_slice(&[0xffu8]);
            },
        )
    }

    /// Note: INSTANCE_ID is sometimes != 0.
    #[pre]
    pub fn insert_token(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        token_id: u32,
        token_value: u32,
    ) -> Result<()> {
        let group_id = entry_id.group_id();
        // Make sure that the entry exists before resizing the group
        let group = self.group(group_id)?.ok_or(Error::GroupNotFound)?;
        let entry = group
            .entry_exact(entry_id, instance_id, board_instance_mask)
            .ok_or(Error::EntryNotFound)?;
        match &entry.body {
            EntryItemBody::<_>::Tokens(a) => match a.token(token_id) {
                None => {}
                _ => {
                    return Err(Error::TokenUniqueKeyViolation);
                }
            },
            _ => {
                return Err(Error::EntryTypeMismatch); // it's just not a Token
                                                      // Entry.
            }
        }
        // Tokens that destroy the alignment in the container have not been
        // tested, are impossible right now anyway and have never been seen.  So
        // disallow those.
        const TOKEN_SIZE: u16 = size_of::<TOKEN_ENTRY>() as u16;
        const_assert!(TOKEN_SIZE % (ENTRY_ALIGNMENT as u16) == 0);
        let mut group = self.resize_group_by(group_id, TOKEN_SIZE.into())?;
        // Now, GroupMutItem.buf includes space for the token, claimed by no
        // entry so far.  group.insert_token has special logic in order to
        // survive that.
        match #[assure(
            "Caller already grew the group by `size_of::<TOKEN_ENTRY>()`",
            reason = "See a few lines above here"
        )]
        group.insert_token(
            entry_id,
            instance_id,
            board_instance_mask,
            token_id,
            token_value,
        ) {
            Err(Error::EntryNotFound) => {
                panic!("Internal error: Entry (entry_id = {:?}, instance_id = {:?}, board_instance_mask = {:?}) was found before resizing group but is not found after resizing group", entry_id, instance_id, board_instance_mask);
            }
            x => x,
        }
    }
    pub fn delete_token(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        token_id: u32,
    ) -> Result<()> {
        let group_id = entry_id.group_id();
        // Make sure that the entry exists before resizing the group
        let mut group =
            self.group_mut(group_id)?.ok_or(Error::GroupNotFound)?;
        let token_diff = group.delete_token(
            entry_id,
            instance_id,
            board_instance_mask,
            token_id,
        )?;
        self.resize_group_by(group_id, token_diff)?;
        Ok(())
    }

    pub fn delete_group(&mut self, group_id: GroupId) -> Result<()> {
        let apcb_size = self.header()?.apcb_size.get();
        let mut groups = self.groups_mut()?;
        let (offset, group_size) = groups.move_point_to(group_id)?;
        self.header_mut()?.apcb_size.set(
            apcb_size.checked_sub(group_size as u32).ok_or(
                Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "HEADER_V2::apcb_size",
                ),
            )?,
        );
        let buf = &mut self.beginning_of_groups_mut()?[offset..];
        buf.copy_within(group_size..(apcb_size as usize), 0);
        self.used_size =
            self.used_size.checked_sub(group_size).ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "GROUP_HEADER::group_size",
            ))?;
        Ok(())
    }

    pub fn insert_group(
        &mut self,
        group_id: GroupId,
        signature: [u8; 4],
    ) -> Result<GroupMutItem<'_>> {
        // TODO: insert sorted.

        if !match group_id {
            GroupId::Psp => signature == *b"PSPG",
            GroupId::Ccx => signature == *b"CCXG",
            GroupId::Df => signature == *b"DFG ",
            GroupId::Memory => signature == *b"MEMG",
            GroupId::Gnb => signature == *b"GNBG",
            GroupId::Fch => signature == *b"FCHG",
            GroupId::Cbs => signature == *b"CBSG",
            GroupId::Oem => signature == *b"OEMG",
            GroupId::Token => signature == *b"TOKN",
            GroupId::Unknown(_) => true,
        } {
            return Err(Error::GroupTypeMismatch);
        }

        let mut groups = self.groups_mut()?;
        match groups.move_point_to(group_id) {
            Err(Error::GroupNotFound) => {}
            Err(x) => {
                return Err(x);
            }
            _ => {
                return Err(Error::GroupUniqueKeyViolation);
            }
        }

        let size = size_of::<GROUP_HEADER>();
        let old_apcb_size = self.header()?.apcb_size.get();
        let new_apcb_size =
            old_apcb_size.checked_add(size as u32).ok_or(Error::OutOfSpace)?;
        let old_used_size = self.used_size;
        let new_used_size =
            old_used_size.checked_add(size).ok_or(Error::OutOfSpace)?;
        if self.beginning_of_groups()?.len() < new_used_size {
            return Err(Error::OutOfSpace);
        }
        self.header_mut()?.apcb_size.set(new_apcb_size);
        self.used_size = new_used_size;

        let mut beginning_of_group =
            &mut self.beginning_of_groups_mut()?[old_used_size..new_used_size];

        let mut header = take_header_from_collection_mut::<GROUP_HEADER>(
            &mut beginning_of_group,
        )
        .ok_or(Error::FileSystem(
            FileSystemError::InconsistentHeader,
            "GROUP_HEADER",
        ))?;
        *header = GROUP_HEADER::default();
        header.signature = signature;
        header.group_id = group_id.to_u16().unwrap().into();
        let body = take_body_from_collection_mut(&mut beginning_of_group, 0, 1)
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "GROUP_HEADER",
            ))?;
        let body_len = body.len();

        Ok(GroupMutItem { header, buf: body, used_size: body_len })
    }

    pub(crate) fn calculate_checksum(
        header: &LayoutVerified<&'_ [u8], V2_HEADER>,
        v3_header_ext: &Option<LayoutVerified<&'_ [u8], V3_HEADER_EXT>>,
        beginning_of_groups: &[u8],
    ) -> Result<u8> {
        let mut checksum_byte = 0u8;
        let stored_checksum_byte = header.checksum_byte;
        for c in header.bytes() {
            checksum_byte = checksum_byte.wrapping_add(*c);
        }
        let mut offset = header.bytes().len();
        if let Some(v3_header_ext) = &v3_header_ext {
            for c in v3_header_ext.bytes() {
                checksum_byte = checksum_byte.wrapping_add(*c);
            }
            offset = offset
                .checked_add(v3_header_ext.bytes().len())
                .ok_or(Error::OutOfSpace)?;
        }
        let apcb_size = header.apcb_size.get();
        let beginning_of_groups_used_size = (apcb_size as usize)
            .checked_sub(offset)
            .ok_or(Error::OutOfSpace)?;
        let beginning_of_groups = &beginning_of_groups;
        if beginning_of_groups.len() < beginning_of_groups_used_size {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER::apcb_size",
            ));
        }
        for c in &beginning_of_groups[0..beginning_of_groups_used_size] {
            checksum_byte = checksum_byte.wrapping_add(*c);
        }
        // Correct for stored_checksum_byte
        checksum_byte = checksum_byte.wrapping_sub(stored_checksum_byte);
        Ok((0x100u16 - u16::from(checksum_byte)) as u8) // Note: This can
                                                        // overflow
    }

    /// Note: for OPTIONS, try ApcbIoOptions::default()
    pub fn load(
        #[allow(unused_mut)] mut bs: PtrMut<'a, [u8]>,
        options: &ApcbIoOptions,
    ) -> Result<Self> {
        let backing_store_len = bs.len();

        #[cfg(not(feature = "serde"))]
        let backing_store: &mut [u8] = bs;
        #[cfg(feature = "serde")]
        let backing_store: &mut [u8] = bs.to_mut();

        let (header, mut rest) =
            LayoutVerified::<&[u8], V2_HEADER>::new_unaligned_from_prefix(
                &*backing_store,
            )
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER",
            ))?;

        if header.signature != *b"APCB" {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER::signature",
            ));
        }

        if usize::from(header.header_size) >= size_of::<V2_HEADER>() {
        } else {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER::header_size",
            ));
        }
        let version = header.version.get();
        if version == Self::ROME_VERSION || version == Self::NAPLES_VERSION {
        } else {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER::version",
            ));
        }
        let apcb_size = header.apcb_size.get();

        let v3_header_ext = if usize::from(header.header_size)
            == size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>()
        {
            let (header_ext, restb) = LayoutVerified::<&[u8], V3_HEADER_EXT>
                ::new_unaligned_from_prefix(rest)
                .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V3_HEADER_EXT",
            ))?;
            let value = &*header_ext;
            rest = restb;
            if value.signature == *b"ECB2" {
            } else {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "V3_HEADER_EXT::signature",
                ));
            }
            if value.struct_version.get() == 0x12 {
            } else {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "V3_HEADER_EXT::struct_version",
                ));
            }
            if value.data_version.get() == 0x100 {
            } else {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "V3_HEADER_EXT::data_version",
                ));
            }
            if value.ext_header_size.get() == 96 {
            } else {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "V3_HEADER_EXT::ext_header_size",
                ));
            }
            if u32::from(value.data_offset.get()) == 88 {
            } else {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "V3_HEADER_EXT::data_offset",
                ));
            }
            if value.signature_ending == *b"BCBA" {
            } else {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "V3_HEADER_EXT::signature_ending",
                ));
            }
            Some(header_ext)
        } else {
            //// TODO: Maybe skip weird header
            None
        };

        let used_size = apcb_size
            .checked_sub(u32::from(header.header_size.get()))
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER::header_size",
            ))? as usize;
        if used_size <= backing_store_len {
        } else {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER::header_size",
            ));
        }

        let checksum_byte = if options.check_checksum {
            Self::calculate_checksum(&header, &v3_header_ext, rest)?
        } else {
            0
        };
        if options.check_checksum {
            if header.checksum_byte != checksum_byte {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "V2_HEADER::checksum_byte",
                ));
            }
        }
        let result = Self { backing_store: bs, used_size };

        match result.groups()?.validate() {
            Ok(_) => {}
            Err(e) => {
                return Err(e);
            }
        }
        Ok(result)
    }

    pub fn update_checksum(&mut self) -> Result<()> {
        self.header_mut()?.checksum_byte = 0; // make calculate_checksum's job easier
        let checksum_byte = Self::calculate_checksum(
            &self.header()?,
            &self.v3_header_ext()?,
            self.beginning_of_groups()?,
        )?;
        self.header_mut()?.checksum_byte = checksum_byte;
        Ok(())
    }

    /// This function does not increment the unique_apcb_instance, and thus
    /// should only be used during an initial build of the APCB. In cases where
    /// one is updating an existing apcb binary, one should always call save()
    pub fn save_no_inc(mut self) -> Result<PtrMut<'a, [u8]>> {
        self.update_checksum()?;
        Ok(self.backing_store)
    }

    /// User is expected to call this once after modifying anything in the apcb
    /// (including insertions and deletions). We update both the checksum
    /// and the unique_apcb_instance.
    pub fn save(mut self) -> Result<PtrMut<'a, [u8]>> {
        let unique_apcb_instance = self.unique_apcb_instance()?;
        self.header_mut()?
            .unique_apcb_instance
            .set(unique_apcb_instance.wrapping_add(1));
        self.update_checksum()?;
        Ok(self.backing_store)
    }

    pub fn create(
        #[allow(unused_mut)] mut bs: PtrMut<'a, [u8]>,
        initial_unique_apcb_instance: u32,
        options: &ApcbIoOptions,
    ) -> Result<Self> {
        #[cfg(not(feature = "serde"))]
        let backing_store: &mut [u8] = bs;
        #[cfg(feature = "serde")]
        let backing_store: &mut [u8] = bs.to_mut();

        backing_store.fill(0xFF);
        {
            let mut backing_store = &mut *backing_store;
            let mut header = take_header_from_collection_mut::<V2_HEADER>(
                &mut backing_store,
            )
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER",
            ))?;
            *header = Default::default();
            header.unique_apcb_instance.set(initial_unique_apcb_instance);

            let v3_header_ext =
                take_header_from_collection_mut::<V3_HEADER_EXT>(
                    &mut backing_store,
                )
                .ok_or(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "V3_HEADER_EXT",
                ))?;
            *v3_header_ext = Default::default();

            header.header_size.set(
                (size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>()) as u16,
            );
            header.apcb_size = (header.header_size.get() as u32).into();
        }
        let (header, rest) =
            LayoutVerified::<&'_ [u8], V2_HEADER>::new_unaligned_from_prefix(
                &*backing_store,
            )
            .unwrap();
        let (v3_header_ext, rest) = LayoutVerified::<&'_ [u8], V3_HEADER_EXT>::new_unaligned_from_prefix(rest).unwrap();
        let checksum_byte =
            Self::calculate_checksum(&header, &Some(v3_header_ext), rest)?;

        let (mut header, _) = LayoutVerified::<&'_ mut [u8], V2_HEADER>::new_unaligned_from_prefix(backing_store).unwrap();
        header.checksum_byte = checksum_byte;
        Self::load(bs, options)
    }
    /// Note: Each modification in the APCB causes the value of
    /// unique_apcb_instance to change.
    pub fn unique_apcb_instance(&self) -> Result<u32> {
        Ok(self.header()?.unique_apcb_instance.get())
    }
    /// Constructs a attribute accessor proxy for the given combination of
    /// (INSTANCE_ID, BOARD_INSTANCE_MASK).  ENTRY_ID is inferred on access.
    /// PRIORITY_MASK is used if the entry needs to be created.
    /// The proxy takes care of creating the group, entry and token as
    /// necessary.  It does not delete stuff.
    /// ABL0_VERSION is the version of the AGESA bootloader used
    pub fn tokens_mut<'b>(
        &'b mut self,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        priority_mask: PriorityLevels,
        abl0_version: Option<u32>,
    ) -> Result<TokensMut<'a, 'b>> {
        TokensMut::new(
            self,
            instance_id,
            board_instance_mask,
            priority_mask,
            abl0_version,
        )
    }
    /// Constructs a attribute accessor proxy for the given combination of
    /// (INSTANCE_ID, BOARD_INSTANCE_MASK).  ENTRY_ID is inferred on access.
    pub fn tokens<'b>(
        &'b self,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    ) -> Result<Tokens<'a, 'b>> {
        Tokens::new(self, instance_id, board_instance_mask)
    }
    /// Ensures that the APCB is compatible with the ABL0_VERSION given
    /// (which is supposed to be the version extracted from the Abl0 blob
    /// file--or None if it could not be found).
    pub(crate) fn ensure_abl0_compatibility(
        &self,
        abl0_version: Option<u32>,
    ) -> Result<()> {
        if let Some(abl0_version) = abl0_version {
            if let Ok(Some(group)) = self.group(GroupId::Token) {
                for entry in group.entries() {
                    let entry_id = entry.id();
                    let tokens = match &entry.body {
                        EntryItemBody::<_>::Tokens(tokens) => tokens,
                        _ => return Err(Error::EntryTypeMismatch),
                    };
                    if let EntryId::Token(token_entry_id) = entry_id {
                        token_entry_id
                            .ensure_abl0_compatibility(abl0_version, tokens)?;
                    } else {
                        return Err(Error::EntryTypeMismatch);
                    }
                }
            }
        }
        Ok(())
    }
}
