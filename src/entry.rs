use core::mem::size_of;
use crate::types::{Result, Error, FileSystemError};
use crate::ondisk::ENTRY_HEADER;
pub use crate::ondisk::{ContextFormat, ContextType, EntryId, take_header_from_collection, take_header_from_collection_mut, take_body_from_collection, take_body_from_collection_mut, EntryCompatible, MemoryEntryId};
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
    pub header: &'a mut ENTRY_HEADER,
    pub body: EntryItemBody<&'a mut [u8]>,
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

    pub fn set_priority_mask(&mut self, value: u8) -> &mut Self {
        self.header.priority_mask = value;
        self
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
        if T::is_entry_compatible(self.id()) {
            match &mut self.body {
                EntryItemBody::Struct(buf) => {
                    let mut buf = buf; // FIXME .clone();
                    if buf.len() == size_of::<T>() {
                        let result = take_header_from_collection_mut::<T>(&mut buf)?;
                        assert!(buf.is_empty());
                        Some(result)
                    } else {
                        None
                    }
                },
                _ => {
                    None
                },
            }
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct EntryItem<'a> {
    pub header: &'a ENTRY_HEADER,
    pub body: EntryItemBody<&'a [u8]>,
}

impl EntryItem<'_> {
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
        if T::is_entry_compatible(self.id()) {
            match self.body {
                EntryItemBody::Struct(buf) => {
                    let mut buf = buf.clone();
                    if buf.len() == size_of::<T>() {
                        let result = take_header_from_collection::<T>(&mut buf)?;
                        assert!(buf.is_empty());
                        Some(result)
                    } else {
                        None
                    }
                },
                _ => {
                    None
                },
            }
        } else {
            None
        }
    }
}
