use crate::ondisk::{
    take_header_from_collection, take_header_from_collection_mut, TokenEntryId,
    TOKEN_ENTRY,
    BoolTags,
    ByteTags,
    WordTags,
    DwordTags,
};
use crate::types::{Error, FileSystemError, Result};
use core::mem::size_of;
use num_traits::FromPrimitive;
use pre::pre;

#[derive(Debug)]
pub struct TokensEntryBodyItem<BufferType> {
    unit_size: u8,
    entry_id: TokenEntryId,
    buf: BufferType,
    used_size: usize,
}

pub struct TokensEntryIter<'a> {
    entry_id: TokenEntryId,
    buf: &'a [u8],
    remaining_used_size: usize,
}

pub struct TokensEntryIterMut<'a> {
    entry_id: TokenEntryId,
    buf: &'a mut [u8],
    remaining_used_size: usize,
}

impl<BufferType> TokensEntryBodyItem<BufferType> {
    pub(crate) fn new(
        unit_size: u8,
        entry_id: u16,
        buf: BufferType,
        used_size: usize,
    ) -> Result<Self> {
        if unit_size != 8 {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::unit_size",
            ));
        }
        Ok(Self {
            unit_size,
            entry_id: TokenEntryId::from_u16(entry_id).ok_or(
                Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "ENTRY_HEADER::entry_id",
                ),
            )?,
            buf,
            used_size,
        })
    }
}

#[derive(Debug)]
pub struct TokensEntryItemMut<'a> {
    entry_id: TokenEntryId,
    token: &'a mut TOKEN_ENTRY,
}

impl<'a> TokensEntryItemMut<'a> {
    pub fn id(&self) -> u32 {
        self.token.key.get()
    }
    pub fn value(&self) -> u32 {
        self.token.value.get()
            & match self.entry_id {
                TokenEntryId::Bool => 0x1,
                TokenEntryId::Byte => 0xFF,
                TokenEntryId::Word => 0xFFFF,
                TokenEntryId::Dword => 0xFFFF_FFFF,
                TokenEntryId::Unknown(_) => 0xFFFF_FFFF,
            }
    }

    // Since the id is a sort key, it cannot be mutated.

    pub fn set_value(&mut self, value: u32) -> Result<()> {
        if value
            == (value
                & match self.entry_id {
                    TokenEntryId::Bool => 0x1,
                    TokenEntryId::Byte => 0xFF,
                    TokenEntryId::Word => 0xFFFF,
                    TokenEntryId::Dword => 0xFFFF_FFFF,
                    TokenEntryId::Unknown(_) => 0xFFFF_FFFF,
                })
        {
            self.token.value.set(value);
            Ok(())
        } else {
            Err(Error::TokenRange)
        }
    }
}

impl<'a> TokensEntryIterMut<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what
    /// this function does. Note: The caller needs to manually decrease
    /// remaining_used_size for each call if desired.
    fn next_item<'b>(
        entry_id: TokenEntryId,
        buf: &mut &'b mut [u8],
    ) -> Result<TokensEntryItemMut<'b>> {
        if buf.is_empty() {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "TOKEN_ENTRY",
            ));
        }
        let token =
            match take_header_from_collection_mut::<TOKEN_ENTRY>(&mut *buf) {
                Some(item) => item,
                None => {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "TOKEN_ENTRY",
                    ));
                }
            };
        Ok(TokensEntryItemMut { entry_id, token })
    }

    /// Find the place BEFORE which the entry TOKEN_ID is supposed to go.
    pub(crate) fn move_insertion_point_before(
        &mut self,
        token_id: u32,
    ) -> Result<()> {
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.is_empty() {
                break;
            }
            match Self::next_item(self.entry_id, &mut buf) {
                Ok(e) => {
                    if e.id() < token_id {
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
    /// Find the place BEFORE which the entry TOKEN_ID is supposed to go.
    pub(crate) fn move_point_to(&mut self, token_id: u32) -> Result<()> {
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.is_empty() {
                return Err(Error::TokenNotFound);
            }
            match Self::next_item(self.entry_id, &mut buf) {
                Ok(e) => {
                    if e.id() != token_id {
                        self.next().unwrap();
                    } else {
                        return Ok(());
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }

    /// Inserts the given entry data at the right spot.
    #[pre(
        "Caller already increased the group size by `size_of::<TOKEN_ENTRY>()`"
    )]
    #[pre(
        "Caller already increased the entry size by `size_of::<TOKEN_ENTRY>()`"
    )]
    pub(crate) fn insert_token(
        &mut self,
        token_id: u32,
        token_value: u32,
    ) -> Result<TokensEntryItemMut<'a>> {
        let token_size = size_of::<TOKEN_ENTRY>();

        // Make sure that move_insertion_point_before does not notice the new
        // uninitialized token
        self.remaining_used_size = self
            .remaining_used_size
            .checked_sub(token_size as usize)
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "TOKEN_ENTRY",
            ))?;
        self.move_insertion_point_before(token_id)?;
        // Move the entries from after the insertion point to the right (in
        // order to make room before for our new entry).
        self.buf
            .copy_within(0..self.remaining_used_size, token_size as usize);

        self.remaining_used_size = self
            .remaining_used_size
            .checked_add(token_size as usize)
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "TOKEN_ENTRY",
            ))?;
        let token =
            match take_header_from_collection_mut::<TOKEN_ENTRY>(&mut self.buf)
            {
                Some(item) => item,
                None => {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "TOKEN_ENTRY",
                    ));
                }
            };

        token.key.set(token_id);
        token.value.set(0); // always valid
        let mut result = TokensEntryItemMut {
            entry_id: self.entry_id,
            token,
        };

        // This has better error checking
        result.set_value(token_value)?;
        Ok(result)
    }

    /// Delete the token TOKEN_ID from the entry.
    /// Postcondition: Caller resizes the containers afterwards.
    pub(crate) fn delete_token(&mut self, token_id: u32) -> Result<()> {
        self.move_point_to(token_id)?;
        let token_size = size_of::<TOKEN_ENTRY>();
        // Move the tokens behind this one to the left
        self.buf
            .copy_within(token_size..self.remaining_used_size, 0);
        self.remaining_used_size = self
            .remaining_used_size
            .checked_sub(token_size as usize)
            .ok_or(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "TOKEN_ENTRY",
            ))?;
        Ok(())
    }
}

impl<'a> Iterator for TokensEntryIterMut<'a> {
    type Item = TokensEntryItemMut<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(self.entry_id, &mut self.buf) {
            Ok(e) => {
                assert!(self.remaining_used_size >= 8);
                self.remaining_used_size -= 8;
                Some(e)
            }
            Err(_) => None,
        }
    }
}

pub struct TokensEntryItem<'a> {
    entry_id: TokenEntryId,
    entry: &'a TOKEN_ENTRY,
}

impl<'a> core::fmt::Debug for TokensEntryItem<'a> {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let entry = self.entry;
        let key = entry.key.get();
        let mut ds = fmt.debug_struct("TokensEntryItem_TOKEN_ENTRY");
        ds.field("entry_id", &self.entry_id);
        match self.entry_id {
            TokenEntryId::Bool => if let Some(key) = BoolTags::from_u32(key) {
                ds.field("key", &key)
            } else {
                ds.field("key", &key)
            },
            TokenEntryId::Byte => if let Some(key) = ByteTags::from_u32(key) {
                ds.field("key", &key)
            } else {
                ds.field("key", &key)
            },
            TokenEntryId::Word => if let Some(key) = WordTags::from_u32(key) {
                ds.field("key", &key)
            } else {
                ds.field("key", &key)
            },
            TokenEntryId::Dword => if let Some(key) = DwordTags::from_u32(key) {
                ds.field("key", &key)
            } else {
                ds.field("key", &key)
            },
            TokenEntryId::Unknown(_) => {
                ds.field("key", &key)
            },
        }.field("value", &entry.value.get()).finish()
    }
}

impl<'a> TokensEntryItem<'a> {
    pub fn id(&self) -> u32 {
        self.entry.key.get()
    }
    pub fn value(&self) -> u32 {
        self.entry.value.get()
            & match self.entry_id {
                TokenEntryId::Bool => 0x1,
                TokenEntryId::Byte => 0xFF,
                TokenEntryId::Word => 0xFFFF,
                TokenEntryId::Dword => 0xFFFF_FFFF,
                TokenEntryId::Unknown(_) => 0xFFFF_FFFF,
            }
    }
}

impl<'a> TokensEntryIter<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what
    /// this function does. Note: The caller needs to manually decrease
    /// remaining_used_size for each call if desired.
    fn next_item<'b>(
        entry_id: TokenEntryId,
        buf: &mut &'b [u8],
    ) -> Result<TokensEntryItem<'b>> {
        if buf.is_empty() {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "TOKEN_ENTRY",
            ));
        }
        let header = match take_header_from_collection::<TOKEN_ENTRY>(&mut *buf)
        {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem(
                    FileSystemError::InconsistentHeader,
                    "TOKEN_ENTRY",
                ));
            }
        };
        Ok(TokensEntryItem {
            entry_id,
            entry: header,
        })
    }
    pub(crate) fn next1(&mut self) -> Result<TokensEntryItem<'a>> {
        if self.remaining_used_size == 0 {
            panic!("Internal error");
        }
        match Self::next_item(self.entry_id, &mut self.buf) {
            Ok(e) => {
                if self.remaining_used_size >= 8 {
                } else {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "TOKEN_ENTRY",
                    ));
                }
                self.remaining_used_size -= 8;
                Ok(e)
            }
            Err(e) => Err(e),
        }
    }
    /// Validates the entries (recursively).  Also consumes iterator.
    pub(crate) fn validate(mut self) -> Result<()> {
        while self.remaining_used_size > 0 {
            match self.next1() {
                Ok(_) => {}
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}

impl<'a> Iterator for TokensEntryIter<'a> {
    type Item = TokensEntryItem<'a>;

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

impl<'a> TokensEntryBodyItem<&'a mut [u8]> {
    pub fn iter(&self) -> TokensEntryIter<'_> {
        TokensEntryIter {
            entry_id: self.entry_id,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }

    pub fn iter_mut(&mut self) -> TokensEntryIterMut<'_> {
        TokensEntryIterMut {
            entry_id: self.entry_id,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }
    pub fn token(&self, token_id: u32) -> Option<TokensEntryItem<'_>> {
        for entry in self.iter() {
            if entry.id() == token_id {
                return Some(entry);
            }
        }
        None
    }
    pub fn token_mut(
        &mut self,
        token_id: u32,
    ) -> Option<TokensEntryItemMut<'_>> {
        for entry in self.iter_mut() {
            if entry.id() == token_id {
                return Some(entry);
            }
        }
        None
    }

    #[pre(
        "Caller already increased the group size by `size_of::<TOKEN_ENTRY>()`"
    )]
    #[pre(
        "Caller already increased the entry size by `size_of::<TOKEN_ENTRY>()`"
    )]
    pub(crate) fn insert_token(
        &mut self,
        token_id: u32,
        token_value: u32,
    ) -> Result<()> {
        let mut iter = self.iter_mut();
        match #[assure(
            "Caller already increased the group size by `size_of::<TOKEN_ENTRY>()`",
            reason = "See our own precondition"
        )]
        #[assure(
            "Caller already increased the entry size by `size_of::<TOKEN_ENTRY>()`",
            reason = "See our own precondition"
        )]
        iter.insert_token(token_id, token_value)
        {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub(crate) fn delete_token(&mut self, token_id: u32) -> Result<()> {
        self.iter_mut().delete_token(token_id)
    }
}

impl<'a> TokensEntryBodyItem<&'a [u8]> {
    pub fn iter(&self) -> TokensEntryIter<'_> {
        TokensEntryIter {
            entry_id: self.entry_id,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }
    pub fn token(&self, token_id: u32) -> Option<TokensEntryItem<'_>> {
        for entry in self.iter() {
            if entry.id() == token_id {
                return Some(entry);
            }
        }
        None
    }
}
