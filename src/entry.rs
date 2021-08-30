use core::marker::PhantomData;
use core::mem::size_of;
use crate::types::{Result, Error, FileSystemError};
use crate::ondisk::ENTRY_HEADER;
use crate::ondisk::{PriorityLevels, ContextFormat, ContextType, EntryId, take_header_from_collection, take_header_from_collection_mut, EntryCompatible, HeaderWithTail};
use num_traits::FromPrimitive;
use crate::tokens_entry::TokensEntryBodyItem;
use zerocopy::{AsBytes, FromBytes};

/* Note: high-level interface is:

   enum EntryMutItem {
       Raw(&[u8]),
       Tokens(Token...),
       Params(Param...), // not seen in the wild anymore
   }

*/

#[derive(Debug)]
pub enum EntryItemBody<BufferType> {
    Struct(BufferType),
    Tokens(TokensEntryBodyItem<BufferType>),
    Parameters(BufferType), // not seen in the wild anymore
    /* Not seen in the wild anymore.
        /// If the value is a Parameter, returns its time point
        pub fn parameter_time_point(&self) -> u8 {
            assert!(self.context_type() == ContextType::Parameter);
            self.body[0]
        }

        /// If the value is a Parameter, returns its token
        pub fn parameter_token(&self) -> u16 {
            assert!(self.context_type() == ContextType::Parameter);
            let value = self.body[1] as u16 | ((self.body[2] as u16) << 8);
            value & 0x1FFF
        }

        // If the value is a Parameter, returns its size
        pub fn parameter_size(&self) -> u16 {
            assert!(self.context_type() == ContextType::Parameter);
            let value = self.body[1] as u16 | ((self.body[2] as u16) << 8);
            (value >> 13) + 1
        }
    */
}

impl<'a> EntryItemBody<&'a mut [u8]> {
    pub(crate) fn from_slice(unit_size: u8, entry_id: u16, context_type: ContextType, b: &'a mut [u8]) -> Result<EntryItemBody<&'a mut [u8]>> {
        Ok(match context_type {
            ContextType::Struct => {
                if unit_size != 0 {
                     return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::unit_size"));
                }
                Self::Struct(b)
            },
            ContextType::Tokens => {
                let used_size = b.len();
                Self::Tokens(TokensEntryBodyItem::<&'_ mut [u8]>::new(unit_size, entry_id, b, used_size)?)
            },
            ContextType::Parameters => {
                Self::Parameters(b)
            },
        })
    }
}

impl<'a> EntryItemBody<&'a [u8]> {
    pub(crate) fn from_slice(unit_size: u8, entry_id: u16, context_type: ContextType, b: &'a [u8]) -> Result<EntryItemBody<&'a [u8]>> {
        Ok(match context_type {
            ContextType::Struct => {
                if unit_size != 0 {
                     return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::unit_size"));
                }
                Self::Struct(b)
            },
            ContextType::Tokens => {
                let used_size = b.len();
                Self::Tokens(TokensEntryBodyItem::<&'_ [u8]>::new(unit_size, entry_id, b, used_size)?)
            },
            ContextType::Parameters => {
                Self::Parameters(b)
            },
        })
    }
    pub(crate) fn validate(&self) -> Result<()> {
        match self {
            EntryItemBody::Tokens(tokens) => {
                tokens.iter().validate()?;
            },
            EntryItemBody::Struct(_) => {
            },
            EntryItemBody::Parameters(_) => {
            },
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct EntryMutItem<'a> {
    pub(crate) header: &'a mut ENTRY_HEADER,
    pub body: EntryItemBody<&'a mut [u8]>,
}

pub struct StructArrayEntryMutItem<'a, T: Sized + FromBytes + AsBytes> {
    buf: &'a mut [u8],
    _item: PhantomData<&'a T>,
}

impl<'a, T: 'a + Sized + FromBytes + AsBytes> StructArrayEntryMutItem<'a, T> {
    pub fn iter_mut(&mut self) -> StructArrayEntryMutIter<'_, T> {
        StructArrayEntryMutIter {
            buf: self.buf,
            _item: PhantomData,
        }
    }
}

pub struct StructArrayEntryMutIter<'a, T: Sized + FromBytes + AsBytes> {
    buf: &'a mut [u8],
    _item: PhantomData<&'a T>,
}

impl<'a, T: 'a + Sized + FromBytes + AsBytes> Iterator for StructArrayEntryMutIter<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        if self.buf.is_empty() {
            None
        } else {
            // The "?" instead of '.unwrap()" here is solely to support BoardIdGettingMethod (the latter introduces useless padding at the end)
            Some(take_header_from_collection_mut::<T>(&mut self.buf)?)
        }
    }
}

impl<'a> EntryMutItem<'a> {
    pub fn group_id(&self) -> u16 {
        self.header.group_id.get()
    }
    pub fn type_id(&self) -> u16 {
        self.header.entry_id.get()
    }
    pub fn id(&self) -> EntryId {
        EntryId::decode(self.header.group_id.get(), self.header.entry_id.get())
    }
    pub fn instance_id(&self) -> u16 {
        self.header.instance_id.get()
    }
    pub fn context_type(&self) -> ContextType {
        ContextType::from_u8(self.header.context_type).unwrap()
    }
    pub fn context_format(&self) -> ContextFormat {
        ContextFormat::from_u8(self.header.context_format).unwrap()
    }
    /// Note: Applicable iff context_type() == 2.  Usual value then: 8.  If inapplicable, value is 0.
    pub fn unit_size(&self) -> u8 {
        self.header.unit_size
    }
    pub fn priority_mask(&self) -> Result<PriorityLevels> {
        self.header.priority_mask()
    }
    /// Note: Applicable iff context_format() != ContextFormat::Raw. Result <= unit_size.
    pub fn key_size(&self) -> u8 {
        self.header.key_size
    }
    pub fn key_pos(&self) -> u8 {
        self.header.key_pos
    }
    pub fn board_instance_mask(&self) -> u16 {
        self.header.board_instance_mask.get()
    }

    /* Not seen in the wild anymore.
        /// If the value is a Parameter, returns its time point
        pub fn parameter_time_point(&self) -> u8 {
            assert!(self.context_type() == ContextType::Parameter);
            self.body[0]
        }

        /// If the value is a Parameter, returns its token
        pub fn parameter_token(&self) -> u16 {
            assert!(self.context_type() == ContextType::Parameter);
            let value = self.body[1] as u16 | ((self.body[2] as u16) << 8);
            value & 0x1FFF
        }

        // If the value is a Parameter, returns its size
        pub fn parameter_size(&self) -> u16 {
            assert!(self.context_type() == ContextType::Parameter);
            let value = self.body[1] as u16 | ((self.body[2] as u16) << 8);
            (value >> 13) + 1
        }
    */

    pub fn set_priority_mask(&mut self, value: PriorityLevels) {
        self.header.set_priority_mask(value);
    }

    // Note: Because entry_id, instance_id, group_id and board_instance_mask are sort keys, these cannot be mutated.

    pub(crate) fn insert_token(&mut self, token_id: u32, token_value: u32) -> Result<()> {
        match &mut self.body {
            EntryItemBody::<_>::Tokens(a) => {
                a.insert_token(token_id, token_value)
            },
            _ => {
                Err(Error::EntryTypeMismatch)
            },
        }
    }

    pub(crate) fn delete_token(&mut self, token_id: u32) -> Result<()> {
        match &mut self.body {
            EntryItemBody::<_>::Tokens(a) => {
                a.delete_token(token_id)
            },
            _ => {
                Err(Error::EntryTypeMismatch)
            },
        }
    }

    pub fn body_as_struct_mut<T: 'a + EntryCompatible + Sized + FromBytes + AsBytes>(&mut self) -> Option<&mut T> {
        let id = self.id();
        match &mut self.body {
            EntryItemBody::Struct(buf) => {
                if T::is_entry_compatible(id, buf) {
                    let mut buf = buf; // FIXME .clone();
                    if buf.len() == size_of::<T>() {
                        let result = take_header_from_collection_mut::<T>(&mut buf)?;
                        assert!(buf.is_empty());
                        Some(result)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            _ => {
                None
            },
        }
    }

    pub fn body_as_struct_array_mut<T: EntryCompatible + Sized + FromBytes + AsBytes>(&mut self) -> Option<StructArrayEntryMutItem<'_, T>> {
        let id = self.id();
        match &mut self.body {
            EntryItemBody::Struct(buf) => {
                if T::is_entry_compatible(id, buf) {
                    let element_count: usize = buf.len() / size_of::<T>();
                    if buf.len() == element_count * size_of::<T>() {
                        Some(StructArrayEntryMutItem {
                            buf,
                            _item: PhantomData,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            _ => {
                None
            },
        }
    }

    pub fn body_as_headered_struct_array_mut<H: EntryCompatible + Sized + FromBytes + AsBytes + HeaderWithTail>(&mut self) -> Option<(&'_ mut H, StructArrayEntryMutItem<'_, H::TailSequenceType>)> {
        let id = self.id();
        match &mut self.body {
            EntryItemBody::Struct(buf) => {
                if H::is_entry_compatible(id, buf) {
                    let mut buf = &mut buf[..];
                    let header = take_header_from_collection_mut::<H>(&mut buf)?;
                    Some((header, StructArrayEntryMutItem {
                        buf,
                        _item: PhantomData,
                    }))
                } else {
                    None
                }
            },
            _ => {
                None
            },
        }
    }

}

#[derive(Debug)]
pub struct EntryItem<'a> {
    pub(crate) header: &'a ENTRY_HEADER,
    pub body: EntryItemBody<&'a [u8]>,
}

pub struct StructArrayEntryItem<'a, T: Sized + FromBytes> {
    buf: &'a [u8],
    _item: PhantomData<&'a T>,
}

impl<'a, T: 'a + Sized + FromBytes> StructArrayEntryItem<'a, T> {
    pub fn iter(&self) -> StructArrayEntryIter<'_, T> {
        StructArrayEntryIter {
            buf: self.buf,
            _item: PhantomData,
        }
    }
}

pub struct StructArrayEntryIter<'a, T: Sized + FromBytes> {
    buf: &'a [u8],
    _item: PhantomData<&'a T>,
}

impl<'a, T: 'a + Sized + FromBytes> Iterator for StructArrayEntryIter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        if self.buf.is_empty() {
            None
        } else {
            // The "?" instead of '.unwrap()" here is solely to support BoardIdGettingMethod (the latter introduces useless padding at the end)
            Some(take_header_from_collection::<T>(&mut self.buf)?)
        }
    }
}

impl<'a> EntryItem<'a> {
    // pub fn group_id(&self) -> u16  ; suppressed--replaced by an assert on read.
    pub fn id(&self) -> EntryId {
        EntryId::decode(self.header.group_id.get(), self.header.entry_id.get())
    }
    pub fn instance_id(&self) -> u16 {
        self.header.instance_id.get()
    }
    pub fn context_type(&self) -> ContextType {
        ContextType::from_u8(self.header.context_type).unwrap()
    }
    pub fn context_format(&self) -> ContextFormat {
        ContextFormat::from_u8(self.header.context_format).unwrap()
    }
    /// Note: Applicable iff context_type() == 2.  Usual value then: 8.  If inapplicable, value is 0.
    pub fn unit_size(&self) -> u8 {
        self.header.unit_size
    }
    pub fn priority_mask(&self) -> u8 {
        self.header.priority_mask
    }
    /// Note: Applicable iff context_format() != ContextFormat::Raw. Result <= unit_size.
    pub fn key_size(&self) -> u8 {
        self.header.key_size
    }
    pub fn key_pos(&self) -> u8 {
        self.header.key_pos
    }
    pub fn board_instance_mask(&self) -> u16 {
        self.header.board_instance_mask.get()
    }

    pub(crate) fn validate(&self) -> Result<()> {
        ContextType::from_u8(self.header.context_type).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::context_type"))?;
        ContextFormat::from_u8(self.header.context_format).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::context_format"))?;
        self.body.validate()?;
        Ok(())
    }

    pub fn body_as_struct<T: EntryCompatible + Sized + FromBytes>(&self) -> Option<&T> {
        let id = self.id();
        match self.body {
            EntryItemBody::Struct(buf) => {
                if T::is_entry_compatible(id, buf) {
                    let mut buf = buf.clone();
                    if buf.len() == size_of::<T>() {
                        let result = take_header_from_collection::<T>(&mut buf)?;
                        assert!(buf.is_empty());
                        Some(result)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            _ => {
                None
            },
        }
    }

    pub fn body_as_struct_array<T: EntryCompatible + Sized + FromBytes>(&self) -> Option<StructArrayEntryItem<'a, T>> {
        match &self.body {
            EntryItemBody::Struct(buf) => {
                if T::is_entry_compatible(self.id(), buf) {
                    let element_count: usize = buf.len() / size_of::<T>();
                    if buf.len() == element_count * size_of::<T>() {
                        Some(StructArrayEntryItem {
                            buf,
                            _item: PhantomData,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            _ => {
                None
            },
        }
    }

    /// This function exists solely to support BoardIdGettingMethod*.  Also, it has weird padding at the end that we are ignoring.
    pub fn body_as_headered_struct_array<H: EntryCompatible + Sized + FromBytes + HeaderWithTail>(&self) -> Option<(&'a H, StructArrayEntryItem<'a, H::TailSequenceType>)> {
        let id = self.id();
        match &self.body {
            EntryItemBody::Struct(buf) => {
                if H::is_entry_compatible(id, buf) {
                    let mut buf = &buf[..];
                    let header = take_header_from_collection::<H>(&mut buf)?;
                    Some((header, StructArrayEntryItem {
                        buf,
                        _item: PhantomData,
                    }))
                } else {
                    None
                }
            },
            _ => {
                None
            },
        }
    }
}
