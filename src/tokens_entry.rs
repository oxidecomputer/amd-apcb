use crate::ondisk::{
    take_header_from_collection, take_header_from_collection_mut, BoolToken,
    ByteToken, DwordToken, TokenEntryId, WordToken, TOKEN_ENTRY, ENTRY_HEADER,
    ContextFormat,
};
use crate::types::{Error, FileSystemError, Result};
use core::convert::TryFrom;
use core::mem::size_of;
use num_traits::FromPrimitive;
use pre::pre;
use zerocopy::ByteSlice;

#[cfg(feature = "serde")]
use serde::ser::{Serialize, Serializer};

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // unit_size is not read when building without serde
pub struct TokensEntryBodyItem<BufferType> {
    unit_size: u8,
    key_pos: u8,
    key_size: u8,
    context_format: u8,
    entry_id: u16,
    buf: BufferType,
    used_size: usize,
}

// Note: Only construct those if unit_size, key_pos and key_size are correct!
pub struct TokensEntryIter<BufferType: ByteSlice> {
    context_format: u8,
    entry_id: TokenEntryId,
    buf: BufferType,
    remaining_used_size: usize,
}

impl<BufferType> TokensEntryBodyItem<BufferType> {
    pub(crate) fn new(
        header: &ENTRY_HEADER,
        buf: BufferType,
        used_size: usize,
    ) -> Result<Self> {
        Ok(Self {
            unit_size: header.unit_size,
            key_pos: header.key_pos,
            key_size: header.key_size,
            context_format: header.context_format,
            entry_id: header.entry_id.get(),
            buf,
            used_size,
        })
    }
    pub(crate) fn prepare_iter(&self) -> Result<TokenEntryId> {
        if self.unit_size != 8 {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::unit_size",
            ));
        }
        if self.key_pos != 0 {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::key_pos",
            ));
        }
        if self.key_size != 4 {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::key_size",
            ));
        }
        let entry_id = TokenEntryId::from_u16(self.entry_id).ok_or(
            Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::entry_id",
            ),
        )?;
        Ok(entry_id)
    }

}

pub struct TokensEntryItem<TokenType> {
    pub(crate) entry_id: TokenEntryId,
    pub(crate) token: TokenType,
}

impl<'a> TokensEntryItem<&'a mut TOKEN_ENTRY> {
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

impl<'a> TokensEntryIter<&'a mut [u8]> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what
    /// this function does. Note: The caller needs to manually decrease
    /// remaining_used_size for each call if desired.
    fn next_item<'b>(
        entry_id: TokenEntryId,
        buf: &mut &'b mut [u8],
    ) -> Result<TokensEntryItem<&'b mut TOKEN_ENTRY>> {
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
        Ok(TokensEntryItem { entry_id, token })
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
    ) -> Result<TokensEntryItem<&'a mut TOKEN_ENTRY>> {
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
        let mut result = TokensEntryItem {
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

impl<'a> Iterator for TokensEntryIter<&'a mut [u8]> {
    type Item = TokensEntryItem<&'a mut TOKEN_ENTRY>;

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

#[cfg(feature = "serde-hex")]
use serde_hex::{SerHex, StrictPfx};

#[cfg(feature = "serde")]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub(crate) enum SerdeTokensEntryItem {
    Bool(BoolToken),
    Byte(ByteToken),
    Word(WordToken),
    Dword(DwordToken),
    Unknown {
        entry_id: TokenEntryId,
        #[cfg_attr(
            feature = "serde-hex",
            serde(
                serialize_with = "SerHex::<StrictPfx>::serialize",
                deserialize_with = "SerHex::<StrictPfx>::deserialize"
            )
        )]
        tag: u32,
        #[cfg_attr(
            feature = "serde-hex",
            serde(
                serialize_with = "SerHex::<StrictPfx>::serialize",
                deserialize_with = "SerHex::<StrictPfx>::deserialize"
            )
        )]
        value: u32,
    },
}

#[cfg(feature = "serde")]
impl SerdeTokensEntryItem {
    pub(crate) fn entry_id(&self) -> TokenEntryId {
        match self {
            Self::Bool(_) => TokenEntryId::Bool,
            Self::Byte(_) => TokenEntryId::Byte,
            Self::Word(_) => TokenEntryId::Word,
            Self::Dword(_) => TokenEntryId::Dword,
            Self::Unknown {
                entry_id,
                tag: _,
                value: _,
            } => *entry_id,
        }
    }
}

#[cfg(feature = "serde")]
impl From<&TokensEntryItem<&'_ TOKEN_ENTRY>> for SerdeTokensEntryItem {
    fn from(item: &TokensEntryItem<&'_ TOKEN_ENTRY>) -> Self {
        let entry = &*item.token;
        let key = entry.key.get();
        let mut st = Self::Unknown {
            entry_id: item.entry_id,
            tag: key,
            value: item.value(),
        };
        match item.entry_id {
            TokenEntryId::Bool => {
                if let Ok(token) = BoolToken::try_from(entry) {
                    st = Self::Bool(token);
                }
            }
            TokenEntryId::Byte => {
                if let Ok(token) = ByteToken::try_from(entry) {
                    st = Self::Byte(token);
                }
            }
            TokenEntryId::Word => {
                if let Ok(token) = WordToken::try_from(entry) {
                    st = Self::Word(token);
                }
            }
            TokenEntryId::Dword => {
                if let Ok(token) = DwordToken::try_from(entry) {
                    st = Self::Dword(token);
                }
            }
            TokenEntryId::Unknown(_) => {}
        }
        st
    }
}

#[cfg(feature = "serde")]
impl core::convert::TryFrom<SerdeTokensEntryItem> for TOKEN_ENTRY {
    type Error = Error;
    fn try_from(
        st: SerdeTokensEntryItem,
    ) -> core::result::Result<Self, Self::Error> {
        match st {
            SerdeTokensEntryItem::Bool(t) => TOKEN_ENTRY::try_from(&t),
            SerdeTokensEntryItem::Byte(t) => TOKEN_ENTRY::try_from(&t),
            SerdeTokensEntryItem::Word(t) => TOKEN_ENTRY::try_from(&t),
            SerdeTokensEntryItem::Dword(t) => TOKEN_ENTRY::try_from(&t),
            SerdeTokensEntryItem::Unknown {
                entry_id: _,
                tag,
                value,
            } => Ok(Self {
                key: tag.into(),
                value: value.into(),
            }),
        }
    }
}

#[cfg(feature = "schemars")]
impl<'a> schemars::JsonSchema for TokensEntryItem<&'a TOKEN_ENTRY> {
    fn schema_name() -> std::string::String {
        SerdeTokensEntryItem::schema_name()
    }
    fn json_schema(
        gen: &mut schemars::gen::SchemaGenerator,
    ) -> schemars::schema::Schema {
        SerdeTokensEntryItem::json_schema(gen)
    }
    fn is_referenceable() -> bool {
        SerdeTokensEntryItem::is_referenceable()
    }
}

#[cfg(feature = "serde")]
impl<'a> Serialize for TokensEntryItem<&'a TOKEN_ENTRY> {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SerdeTokensEntryItem::from(self).serialize(serializer)
    }
}

impl<'a> core::fmt::Debug for TokensEntryItem<&'a TOKEN_ENTRY> {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let entry = &*self.token;
        let key = entry.key.get();
        let mut ds = fmt.debug_struct("TokensEntryItem_TOKEN_ENTRY");
        ds.field("entry_id", &self.entry_id);
        let value = entry.value.get();
        match self.entry_id {
            TokenEntryId::Bool => {
                if let Ok(token) = BoolToken::try_from(entry) {
                    ds.field("token", &token)
                } else {
                    ds.field("key", &key)
                }
            }
            TokenEntryId::Byte => {
                if let Ok(token) = ByteToken::try_from(entry) {
                    ds.field("token", &token)
                } else {
                    ds.field("key", &key)
                }
            }
            TokenEntryId::Word => {
                if let Ok(token) = WordToken::try_from(entry) {
                    ds.field("token", &token)
                } else {
                    ds.field("key", &key)
                }
            }
            TokenEntryId::Dword => {
                if let Ok(token) = DwordToken::try_from(entry) {
                    ds.field("token", &token)
                } else {
                    ds.field("key", &key)
                }
            }
            TokenEntryId::Unknown(_) => ds.field("key", &key),
        }
        .field("value", &value)
        .finish()
    }
}

impl<'a> TokensEntryItem<&'a TOKEN_ENTRY> {
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
}

impl<'a> TokensEntryIter<&'a [u8]> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what
    /// this function does. Note: The caller needs to manually decrease
    /// remaining_used_size for each call if desired.
    fn next_item<'b>(
        entry_id: TokenEntryId,
        buf: &mut &'b [u8],
    ) -> Result<TokensEntryItem<&'b TOKEN_ENTRY>> {
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
            token: header,
        })
    }
    pub(crate) fn next1(&mut self) -> Result<TokensEntryItem<&'a TOKEN_ENTRY>> {
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
        let context_format = ContextFormat::from_u8(self.context_format).ok_or(
            Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::context_format",
            ),
        )?;
        if context_format != ContextFormat::SortAscending {
            return Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::context_format",
            ));
        }
        let mut previous_id = 0_u32;
        while self.remaining_used_size > 0 {
            match self.next1() {
                Ok(item) => {
                    let id = item.id();
                    if id < previous_id {
                        return Err(Error::TokenOrderingViolation)
                    }
                    previous_id = id
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}

impl<'a> Iterator for TokensEntryIter<&'a [u8]> {
    type Item = TokensEntryItem<&'a TOKEN_ENTRY>;

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

impl<BufferType: ByteSlice> TokensEntryBodyItem<BufferType> {
    pub fn iter(&self) -> Result<TokensEntryIter<&'_ [u8]>> {
        let entry_id = self.prepare_iter()?;
        Ok(TokensEntryIter {
            context_format: self.context_format,
            entry_id,
            buf: &self.buf,
            remaining_used_size: self.used_size,
        })
    }
    pub fn token(&self, token_id: u32) -> Option<TokensEntryItem<&'_ TOKEN_ENTRY>> {
        for entry in self.iter().ok()? {
            if entry.id() == token_id {
                return Some(entry);
            }
        }
        None
    }
    pub fn validate(&self) -> Result<()> {
        self.iter()?.validate()
    }
}

impl<'a> TokensEntryBodyItem<&'a mut [u8]> {
    pub fn iter_mut(&mut self) -> Result<TokensEntryIter<&'_ mut [u8]>> {
        let entry_id = self.prepare_iter()?;
        Ok(TokensEntryIter {
            context_format: self.context_format,
            entry_id,
            buf: self.buf,
            remaining_used_size: self.used_size,
        })
    }
    pub fn token_mut(
        &mut self,
        token_id: u32,
    ) -> Option<TokensEntryItem<&'_ mut TOKEN_ENTRY>> {
        for entry in self.iter_mut().ok()? {
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
        let mut iter = self.iter_mut()?;
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
        self.iter_mut()?.delete_token(token_id)
    }
}

impl TokenEntryId {
    pub(crate) fn ensure_abl0_compatibility(&self, abl0_version: u32, tokens: &TokensEntryBodyItem<&[u8]>) -> Result<()> {
        match *self {
            TokenEntryId::Bool => {
                for token in tokens.iter()? {
                    let token_id = token.id();
                    if !BoolToken::valid_for_abl0_raw(abl0_version, token_id) {
                        return Err(Error::TokenVersionMismatch {
                            token_id,
                            abl0_version,
                        })
                    }
                }
            }
            TokenEntryId::Byte => {
                for token in tokens.iter()? {
                    let token_id = token.id();
                    if !ByteToken::valid_for_abl0_raw(abl0_version, token_id) {
                        return Err(Error::TokenVersionMismatch {
                            token_id,
                            abl0_version,
                        })
                    }
                }
            }
            TokenEntryId::Word => {
                for token in tokens.iter()? {
                    let token_id = token.id();
                    if !WordToken::valid_for_abl0_raw(abl0_version, token_id) {
                        return Err(Error::TokenVersionMismatch {
                            token_id,
                            abl0_version,
                        })
                    }
                }
            }
            TokenEntryId::Dword => {
                for token in tokens.iter()? {
                    let token_id = token.id();
                    if !DwordToken::valid_for_abl0_raw(abl0_version, token_id) {
                        return Err(Error::TokenVersionMismatch {
                            token_id,
                            abl0_version,
                        })
                    }
                }
            }
            TokenEntryId::Unknown(_) => {
            }
        }
        Ok(())
    }
}
