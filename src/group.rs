// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::types::{ApcbContext, Error, FileSystemError, Result};

use crate::entry::{EntryItem, EntryItemBody, EntryMutItem};
use crate::ondisk::GroupId;
use crate::ondisk::ENTRY_ALIGNMENT;
use crate::ondisk::ENTRY_HEADER;
use crate::ondisk::GROUP_HEADER;
use crate::ondisk::TOKEN_ENTRY;
use crate::ondisk::{
    take_body_from_collection, take_body_from_collection_mut,
    take_header_from_collection, take_header_from_collection_mut,
};
pub use crate::ondisk::{
    BoardInstances, ContextFormat, ContextType, EntryId, PriorityLevels,
};
use core::convert::TryInto;
use core::mem::size_of;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use pre::pre;

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GroupItem<'a> {
    pub(crate) context: ApcbContext,
    pub(crate) header: &'a GROUP_HEADER,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) buf: &'a [u8],
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) used_size: usize,
}

#[cfg(feature = "serde")]
#[cfg_attr(
    feature = "serde",
    derive(Default, serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SerdeGroupItem {
    pub header: GROUP_HEADER,
}

#[cfg(feature = "schemars")]
impl<'a> schemars::JsonSchema for GroupItem<'a> {
    fn schema_name() -> std::string::String {
        SerdeGroupItem::schema_name()
    }
    fn json_schema(
        gen: &mut schemars::gen::SchemaGenerator,
    ) -> schemars::schema::Schema {
        SerdeGroupItem::json_schema(gen)
    }
    fn is_referenceable() -> bool {
        SerdeGroupItem::is_referenceable()
    }
}

#[derive(Debug)]
pub struct GroupIter<'a> {
    pub(crate) context: ApcbContext,
    pub(crate) header: &'a GROUP_HEADER,
    buf: &'a [u8],
    remaining_used_size: usize,
}

impl<'a> Iterator for GroupIter<'a> {
    type Item = EntryItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match self.next1() {
            Ok(e) => Some(e),
            Err(_) => None,
        }
    }
}
impl<'a> GroupIter<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what
    /// this function does. Note: The caller needs to manually decrease
    /// remaining_used_size for each call if desired.
    fn next_item<'b>(
        context: ApcbContext,
        buf: &mut &'b [u8],
    ) -> Result<EntryItem<'b>> {
        if buf.is_empty() {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER",
            ));
        }
        let header =
            match take_header_from_collection::<ENTRY_HEADER>(&mut *buf) {
                Some(item) => item,
                None => {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "ENTRY_HEADER",
                    ));
                }
            };
        GroupId::from_u16(header.group_id.get()).ok_or(Error::FileSystem(
            FileSystemError::InconsistentHeader,
            "ENTRY_HEADER::group_id",
        ))?;
        let entry_size = header.entry_size.get() as usize;

        let payload_size = entry_size
            .checked_sub(size_of::<ENTRY_HEADER>())
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER",
            ))?;
        let body = match take_body_from_collection(
            &mut *buf,
            payload_size,
            ENTRY_ALIGNMENT,
        ) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "ENTRY_HEADER",
                ));
            }
        };

        let body = EntryItemBody::<&[u8]>::from_slice(header, body, context)?;

        Ok(EntryItem { context, header, body })
    }

    pub(crate) fn next1(&mut self) -> Result<EntryItem<'a>> {
        if self.remaining_used_size == 0 {
            panic!("Internal error");
        }
        match Self::next_item(self.context, &mut self.buf) {
            Ok(e) => {
                if e.header.group_id.get() == self.header.group_id.get() {
                } else {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "ENTRY_HEADER::group_id",
                    ));
                }
                let entry_size = e.header.entry_size.get() as usize;
                if self.remaining_used_size >= entry_size {
                } else {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "ENTRY_HEADER::entry_size",
                    ));
                }
                self.remaining_used_size -= entry_size;
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
                    item.validate()?;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}

impl GroupItem<'_> {
    /// Note: ASCII
    pub fn signature(&self) -> [u8; 4] {
        self.header.signature
    }
    /// Note: See ondisk::GroupId
    pub fn id(&self) -> GroupId {
        GroupId::from_u16(self.header.group_id.get()).unwrap()
    }

    /// This finds the entry with the given ID, INSTANCE_ID and compatible
    /// BOARD_INSTANCE_MASK, if any.  If you have a board_id,
    /// BOARD_INSTANCE_MASK = 1 << board_id
    pub fn entry_compatible(
        &self,
        id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    ) -> Option<EntryItem<'_>> {
        self.entries().find(|entry| {
            entry.id() == id
                && entry.instance_id() == instance_id
                && u16::from(entry.board_instance_mask())
                    & u16::from(board_instance_mask)
                    != 0
        })
    }

    /// This finds the entry with the given ID, INSTANCE_ID and exact
    /// BOARD_INSTANCE_MASK, if any.
    pub fn entry_exact(
        &self,
        id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    ) -> Option<EntryItem<'_>> {
        self.entries().find(|entry| {
            entry.id() == id
                && entry.instance_id() == instance_id
                && entry.board_instance_mask() == board_instance_mask
        })
    }

    pub fn entries(&self) -> GroupIter<'_> {
        GroupIter {
            context: self.context,
            header: self.header,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }
}

impl core::fmt::Debug for GroupItem<'_> {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Note: Elides BODY--so, technically, it's not a 1:1 representation
        let id = self.id();
        let signature = self.signature();
        fmt.debug_struct("GroupItem")
            .field("signature", &signature)
            .field("id", &id)
            .field("group_size", &self.header.group_size)
            .finish()
    }
}

#[derive(Debug)]
pub struct GroupMutIter<'a> {
    pub(crate) context: ApcbContext,
    pub(crate) header: &'a mut GROUP_HEADER,
    buf: &'a mut [u8],
    remaining_used_size: usize,
}

impl<'a> GroupMutIter<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what
    /// this function does. Note: The caller needs to manually decrease
    /// remaining_used_size for each call if desired.
    fn next_item<'b>(
        context: ApcbContext,
        buf: &mut &'b mut [u8],
    ) -> Result<EntryMutItem<'b>> {
        if buf.is_empty() {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER",
            ));
        }
        let header =
            match take_header_from_collection_mut::<ENTRY_HEADER>(&mut *buf) {
                Some(item) => item,
                None => {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "ENTRY_HEADER",
                    ));
                }
            };
        let entry_size = header.entry_size.get() as usize;

        let payload_size = entry_size
            .checked_sub(size_of::<ENTRY_HEADER>())
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER",
            ))?;
        let body = match take_body_from_collection_mut(
            &mut *buf,
            payload_size,
            ENTRY_ALIGNMENT,
        ) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "ENTRY_HEADER",
                ));
            }
        };

        let body =
            EntryItemBody::<&mut [u8]>::from_slice(header, body, context)?;
        Ok(EntryMutItem { context, header, body })
    }

    /// Find the place BEFORE which the entry (GROUP_ID, ENTRY_ID, INSTANCE_ID,
    /// BOARD_INSTANCE_MASK) is supposed to go.
    pub(crate) fn move_insertion_point_before(
        &mut self,
        group_id: u16,
        type_id: u16,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    ) -> Result<()> {
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.is_empty() {
                break;
            }
            match Self::next_item(self.context, &mut buf) {
                Ok(e) => {
                    if (
                        e.group_id(),
                        e.type_id(),
                        e.instance_id(),
                        u16::from(e.board_instance_mask()),
                    ) < (
                        group_id,
                        type_id,
                        instance_id,
                        u16::from(board_instance_mask),
                    ) {
                        self.next().unwrap();
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(())
    }
    /// Find the place BEFORE which the entry (GROUP_ID, ENTRY_ID, INSTANCE_ID,
    /// BOARD_INSTANCE_MASK) is. Returns how much it moved the point, and
    /// the size of the entry.
    pub(crate) fn move_point_to(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        size_diff: i64,
    ) -> Result<(usize, usize)> {
        let mut offset = 0usize;
        loop {
            let remaining_used_size = self.remaining_used_size
                - if size_diff > 0 {
                    // Make it not see the new, uninitialized, entry.
                    size_diff as usize
                } else {
                    0
                };
            let mut buf = &mut self.buf[..remaining_used_size];
            if buf.is_empty() {
                return Err(Error::EntryNotFound {
                    entry_id,
                    instance_id,
                    board_instance_mask,
                });
            }
            match Self::next_item(self.context, &mut buf) {
                Ok(e) => {
                    let entry_size = e.header.entry_size.get();
                    if (e.id(), e.instance_id(), e.board_instance_mask())
                        != (entry_id, instance_id, board_instance_mask)
                    {
                        self.next().unwrap();
                        offset = offset
                            .checked_add(entry_size.into())
                            .ok_or(Error::ArithmeticOverflow)?;
                        while offset % ENTRY_ALIGNMENT != 0 {
                            offset = offset
                                .checked_add(1)
                                .ok_or(Error::ArithmeticOverflow)?;
                        }
                    } else {
                        return Ok((offset, entry_size.into()));
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }
    /// Inserts the given entry data at the right spot.
    #[pre("Caller already grew the group by `payload_size + size_of::<ENTRY_HEADER>()`")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_entry(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        entry_allocation: u16,
        context_type: ContextType,
        payload_size: usize,
        payload_initializer: impl Fn(&mut [u8]),
        priority_mask: PriorityLevels,
    ) -> Result<()> {
        let group_id = entry_id.group_id();
        let entry_id = entry_id.type_id();

        // Make sure that entry_allocation is large enough for the header and
        // data
        let padding_size = (entry_allocation as usize)
            .checked_sub(size_of::<ENTRY_HEADER>())
            .ok_or(Error::FileSystem(
                FileSystemError::PayloadTooBig,
                "ENTRY_HEADER:entry_size",
            ))?;
        let padding_size =
            padding_size.checked_sub(payload_size).ok_or(Error::FileSystem(
                FileSystemError::PayloadTooBig,
                "ENTRY_HEADER:entry_size",
            ))?;

        // Make sure that move_insertion_point_before does not notice the new
        // uninitialized entry
        self.remaining_used_size = self
            .remaining_used_size
            .checked_sub(entry_allocation as usize)
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::entry_size",
            ))?;
        self.move_insertion_point_before(
            group_id.to_u16().unwrap(),
            entry_id,
            instance_id,
            board_instance_mask,
        )?;

        let mut buf = &mut *self.buf; // already done: offset
                                      // Move the entries from after the insertion point to the right (in
                                      // order to make room before for our new entry).
        buf.copy_within(0..self.remaining_used_size, entry_allocation as usize);

        //let mut buf = &mut self.buf[..(self.remaining_used_size +
        // entry_allocation as usize)];
        let header = take_header_from_collection_mut::<ENTRY_HEADER>(&mut buf)
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER",
            ))?;
        *header = ENTRY_HEADER::default();
        header.group_id.set(group_id.to_u16().unwrap());
        header.entry_id.set(entry_id);
        header.entry_size.set(entry_allocation);
        header.instance_id.set(instance_id);
        header.context_type = context_type as u8;
        header.context_format = ContextFormat::Raw as u8;
        header.unit_size = 0;
        header.key_size = 0;
        header.key_pos = 0;
        if context_type == ContextType::Tokens {
            header.context_format = ContextFormat::SortAscending as u8;
            header.unit_size = 8;
            header.key_size = 4;
            header.key_pos = 0;
        }
        header.set_priority_mask(priority_mask);

        // Note: The following is settable by the user via EntryMutItem
        // set-accessors: context_type, context_format, unit_size,
        // priority_mask, key_size, key_pos
        header.board_instance_mask.set(u16::from(board_instance_mask));
        let body = take_body_from_collection_mut(&mut buf, payload_size, 1)
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER",
            ))?;
        payload_initializer(body);

        let padding = take_body_from_collection_mut(&mut buf, padding_size, 1)
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "padding",
            ))?;
        // We pad this with 0s instead of 0xFFs because that's what AMD does,
        // even though the erase polarity of most flash systems nowadays are
        // 0xFF.
        padding.iter_mut().for_each(|b| *b = 0u8);
        self.remaining_used_size = self
            .remaining_used_size
            .checked_add(entry_allocation as usize)
            .ok_or(Error::OutOfSpace)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct GroupMutItem<'a> {
    pub(crate) context: ApcbContext,
    pub(crate) header: &'a mut GROUP_HEADER,
    pub(crate) buf: &'a mut [u8],
    pub(crate) used_size: usize,
}

impl<'a> GroupMutItem<'a> {
    /// Note: ASCII
    pub fn signature(&self) -> [u8; 4] {
        self.header.signature
    }
    /// Note: See ondisk::GroupId
    pub fn id(&self) -> GroupId {
        GroupId::from_u16(self.header.group_id.get()).unwrap()
    }

    /// This finds the entry with the given ID, INSTANCE_ID and compatible
    /// BOARD_INSTANCE_MASK, if any.  If you have a board_id,
    /// BOARD_INSTANCE_MASK = 1 << board_id
    pub fn entry_compatible_mut(
        &mut self,
        id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    ) -> Option<EntryMutItem<'_>> {
        self.entries_mut().find(|entry| {
            entry.id() == id
                && entry.instance_id() == instance_id
                && u16::from(entry.board_instance_mask())
                    & u16::from(board_instance_mask)
                    != 0
        })
    }

    /// Note: BOARD_INSTANCE_MASK needs to be exact.
    /// This finds the entry with the given ID, INSTANCE_ID and exact
    /// BOARD_INSTANCE_MASK, if any.  If you have a board_id,
    /// BOARD_INSTANCE_MASK = 1 << board_id
    pub fn entry_exact_mut(
        &mut self,
        id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    ) -> Option<EntryMutItem<'_>> {
        self.entries_mut().find(|entry| {
            entry.id() == id
                && entry.instance_id() == instance_id
                && entry.board_instance_mask() == board_instance_mask
        })
    }

    pub(crate) fn delete_entry(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    ) -> Result<u32> {
        let mut entries = self.entries_mut();
        let (offset, entry_size) = entries.move_point_to(
            entry_id,
            instance_id,
            board_instance_mask,
            0,
        )?;
        let buf = &mut self.buf[offset..];
        buf.copy_within(entry_size..self.used_size, offset);
        Ok(entry_size as u32)
    }
    /// Resizes the given entry by SIZE_DIFF.
    #[pre("If `size_diff > 0`, caller needs to have expanded the group by `size_diff` already.  If `size_diff < 0`, caller needs to call `resize_entry_by` BEFORE resizing the group.")]
    pub(crate) fn resize_entry_by(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        size_diff: i64,
    ) -> Result<EntryMutItem<'_>> {
        let mut old_used_size = self.used_size;
        let mut entries = self.entries_mut();
        let (offset, entry_size) = entries.move_point_to(
            entry_id,
            instance_id,
            board_instance_mask,
            size_diff,
        )?;
        let entry_size: u16 =
            entry_size.try_into().map_err(|_| Error::ArithmeticOverflow)?;
        let entry = entries.next().ok_or(Error::EntryNotFound {
            entry_id,
            instance_id,
            board_instance_mask,
        })?;

        if size_diff > 0 {
            let size_diff: usize =
                size_diff.try_into().map_err(|_| Error::ArithmeticOverflow)?;
            old_used_size = old_used_size
                .checked_sub(size_diff)
                .ok_or(Error::ArithmeticOverflow)?
        }
        let old_entry_size = entry_size;
        let new_entry_size: u16 = if size_diff > 0 {
            let size_diff = size_diff as u64;
            old_entry_size
                .checked_add(
                    size_diff
                        .try_into()
                        .map_err(|_| Error::ArithmeticOverflow)?,
                )
                .ok_or(Error::OutOfSpace)?
        } else {
            let size_diff = (-size_diff) as u64;
            old_entry_size
                .checked_sub(
                    size_diff
                        .try_into()
                        .map_err(|_| Error::ArithmeticOverflow)?,
                )
                .ok_or(Error::OutOfSpace)?
        };

        entry.header.entry_size.set(new_entry_size);
        let buf = &mut self.buf[offset..];
        buf.copy_within(
            old_entry_size as usize..(old_used_size - offset),
            new_entry_size as usize,
        );
        match self.entry_exact_mut(entry_id, instance_id, board_instance_mask) {
            Some(e) => Ok(e),
            None => {
                panic!("Entry (entry_id = {:?}, instance_id = {:?}, board_instance_mask = {:?}) was found by move_point to, but not by manual iteration after resizing.", entry_id, instance_id, board_instance_mask);
            }
        }
    }
    /// Inserts the given token.
    #[pre("Caller already grew the group by `size_of::<TOKEN_ENTRY>()`")]
    pub(crate) fn insert_token(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        token_id: u32,
        token_value: u32,
    ) -> Result<()> {
        let token_size = size_of::<TOKEN_ENTRY>();
        // Note: Now, GroupMutItem.buf includes space for the token, claimed by
        // no entry so far.  This is bad when iterating over the group members
        // until the end--it will iterate over garbage. Therefore, mask
        // the new area out for the iterator--and reinstate it only after
        // resize_entry_by (which has been adapted specially) is finished with
        // Ok.
        #[assure("If `size_diff > 0`, caller needs to have expanded the group by `size_diff` already.  If `size_diff < 0`, caller needs to call `resize_entry_by` BEFORE resizing the group.", reason = "Our caller ensured that, and we have a precondition to make him")]
        let mut entry = self.resize_entry_by(
            entry_id,
            instance_id,
            board_instance_mask,
            token_size as i64,
        )?;
        #[assure(
            "Caller already increased the entry size by `size_of::<TOKEN_ENTRY>()`",
            reason = "See right before here"
        )]
        #[assure(
            "Caller already increased the group size by `size_of::<TOKEN_ENTRY>()`",
            reason = "See our caller (and our own precondition)"
        )]
        entry.insert_token(token_id, token_value)
    }

    /// Deletes the given token.
    /// Returns the number of bytes that were deleted.
    /// Postcondition: Caller will resize the given group
    #[pre]
    pub(crate) fn delete_token(
        &mut self,
        entry_id: EntryId,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        token_id: u32,
    ) -> Result<i64> {
        let token_size = size_of::<TOKEN_ENTRY>();
        // Note: Now, GroupMutItem.buf includes space for the token, claimed by
        // no entry so far.  This is bad when iterating over the group members
        // until the end--it will iterate over garbage. Therefore, mask
        // the new area out for the iterator--and reinstate it only after
        // resize_entry_by (which has been adapted specially) is finished with
        // Ok.
        let mut entry = self
            .entry_exact_mut(entry_id, instance_id, board_instance_mask)
            .ok_or(Error::EntryNotFound {
                entry_id,
                instance_id,
                board_instance_mask,
            })?;
        entry.delete_token(token_id)?;
        let mut token_size_diff: i64 =
            token_size.try_into().map_err(|_| Error::ArithmeticOverflow)?;
        token_size_diff = -token_size_diff;
        #[assure("If `size_diff > 0`, caller needs to have expanded the group by `size_diff` already.  If `size_diff < 0`, caller needs to call `resize_entry_by` BEFORE resizing the group.", reason = "Our caller will do that, after calling us")]
        self.resize_entry_by(
            entry_id,
            instance_id,
            board_instance_mask,
            token_size_diff,
        )?;
        Ok(token_size_diff)
    }

    pub fn entries(&self) -> GroupIter<'_> {
        GroupIter {
            context: self.context,
            header: self.header,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }

    pub fn entries_mut(&mut self) -> GroupMutIter<'_> {
        GroupMutIter {
            context: self.context,
            header: self.header,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }
}

impl<'a> Iterator for GroupMutIter<'a> {
    type Item = EntryMutItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(self.context, &mut self.buf) {
            Ok(e) => {
                assert!(e.header.group_id.get() == self.header.group_id.get());
                let entry_size = e.header.entry_size.get() as usize;
                assert!(self.remaining_used_size >= entry_size);
                self.remaining_used_size -= entry_size;
                Some(e)
            }
            Err(_) => None,
        }
    }
}
