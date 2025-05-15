// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![macro_use]

use crate::apcb::Apcb;
use crate::entry::EntryItemBody;
use crate::ondisk::BoardInstances;
use crate::ondisk::GroupId;
use crate::ondisk::PriorityLevels;
use crate::ondisk::{
    BoolToken, ByteToken, ContextType, DwordToken, EntryId, TokenEntryId,
    WordToken,
};
use crate::types::Error;
use crate::types::Result;

pub struct TokensMut<'a, 'b> {
    pub(crate) apcb: &'b mut Apcb<'a>,
    pub(crate) instance_id: u16,
    pub(crate) board_instance_mask: BoardInstances,
    pub(crate) priority_mask: PriorityLevels,
    pub(crate) abl0_version: Option<u32>,
}
pub struct Tokens<'a, 'b> {
    pub(crate) apcb: &'b Apcb<'a>,
    pub(crate) instance_id: u16,
    pub(crate) board_instance_mask: BoardInstances,
    //pub(crate) priority_mask: PriorityLevels,
}

impl<'a, 'b> TokensMut<'a, 'b> {
    pub(crate) fn new(
        apcb: &'b mut Apcb<'a>,
        instance_id: u16,
        board_instance_mask: BoardInstances,
        priority_mask: PriorityLevels,
        abl0_version: Option<u32>,
    ) -> Result<Self> {
        match apcb.insert_group(GroupId::Token, *b"TOKN") {
            Err(Error::GroupUniqueKeyViolation { .. }) => {}
            Err(x) => {
                return Err(x);
            }
            _ => {}
        };
        Ok(Self {
            apcb,
            instance_id,
            board_instance_mask,
            priority_mask,
            abl0_version,
        })
    }

    pub(crate) fn get(
        &self,
        token_entry_id: TokenEntryId,
        field_key: u32,
    ) -> Result<u32> {
        let group = self
            .apcb
            .group(GroupId::Token)?
            .ok_or(Error::GroupNotFound { group_id: GroupId::Token })?;
        let entry = group
            .entry_exact(
                EntryId::Token(token_entry_id),
                self.instance_id,
                self.board_instance_mask,
            )
            .ok_or(Error::EntryNotFound {
                entry_id: EntryId::Token(token_entry_id),
                instance_id: self.instance_id,
                board_instance_mask: self.board_instance_mask,
            })?;
        match &entry.body {
            EntryItemBody::<_>::Tokens(a) => {
                let token = a.token(field_key).ok_or(Error::TokenNotFound {
                    token_id: field_key,
                    //entry_id: token_entry_id,
                    //instance_id: self.instance_id,
                    //board_instance_mask: self.board_instance_mask,
                })?;
                assert!(token.id() == field_key);
                let token_value = token.value();
                Ok(token_value)
            }
            _ => Err(Error::EntryTypeMismatch),
        }
    }

    pub(crate) fn set(
        &mut self,
        token_entry_id: TokenEntryId,
        field_key: u32,
        token_value: u32,
    ) -> Result<()> {
        let entry_id = EntryId::Token(token_entry_id);
        let token_id = field_key;
        //let token_value = value.to_u32().ok_or_else(||
        // Error::EntryTypeMismatch)?;
        match self.apcb.insert_token(
            entry_id,
            self.instance_id,
            self.board_instance_mask,
            token_id,
            token_value,
        ) {
            Err(Error::EntryNotFound { .. }) => {
                match self.apcb.insert_entry(
                    entry_id,
                    self.instance_id,
                    self.board_instance_mask,
                    ContextType::Tokens,
                    self.priority_mask,
                    &[],
                ) {
                    Err(Error::EntryUniqueKeyViolation { .. }) => {}
                    Err(x) => {
                        return Err(x);
                    }
                    _ => {}
                };
                if let Some(abl0_version) = self.abl0_version {
                    let valid = match token_entry_id {
                        TokenEntryId::Bool => BoolToken::valid_for_abl0_raw(
                            abl0_version,
                            token_id,
                        ),
                        TokenEntryId::Byte => ByteToken::valid_for_abl0_raw(
                            abl0_version,
                            token_id,
                        ),
                        TokenEntryId::Word => WordToken::valid_for_abl0_raw(
                            abl0_version,
                            token_id,
                        ),
                        TokenEntryId::Dword => DwordToken::valid_for_abl0_raw(
                            abl0_version,
                            token_id,
                        ),
                        TokenEntryId::Unknown(_) => false,
                    };
                    if !valid {
                        return Err(Error::TokenVersionMismatch {
                            entry_id: token_entry_id,
                            token_id,
                            abl0_version,
                        });
                    }
                }
                self.apcb.insert_token(
                    entry_id,
                    self.instance_id,
                    self.board_instance_mask,
                    token_id,
                    token_value,
                )?;
            }
            Err(Error::TokenUniqueKeyViolation { .. }) => {
                let mut group = self.apcb.group_mut(GroupId::Token)?.unwrap();
                let mut entry = group
                    .entry_exact_mut(
                        entry_id,
                        self.instance_id,
                        self.board_instance_mask,
                    )
                    .unwrap();
                match &mut entry.body {
                    EntryItemBody::<_>::Tokens(a) => {
                        let mut token = a.token_mut(token_id).unwrap();
                        token.set_value(token_value)?;
                    }
                    _ => {
                        return Err(Error::EntryTypeMismatch);
                    }
                }
            }
            Err(x) => {
                return Err(x);
            }
            _ => { // inserted new token, and set its value
            }
        }
        Ok(())
    }
}

impl<'a, 'b> Tokens<'a, 'b> {
    pub(crate) fn new(
        apcb: &'b Apcb<'a>,
        instance_id: u16,
        board_instance_mask: BoardInstances,
    ) -> Result<Self> {
        Ok(Self { apcb, instance_id, board_instance_mask })
    }

    pub fn get(
        &self,
        token_entry_id: TokenEntryId,
        field_key: u32,
    ) -> Result<u32> {
        let group = self
            .apcb
            .group(GroupId::Token)?
            .ok_or(Error::GroupNotFound { group_id: GroupId::Token })?;
        let entry = group
            .entry_exact(
                EntryId::Token(token_entry_id),
                self.instance_id,
                self.board_instance_mask,
            )
            .ok_or(Error::EntryNotFound {
                entry_id: EntryId::Token(token_entry_id),
                instance_id: self.instance_id,
                board_instance_mask: self.board_instance_mask,
            })?;
        match &entry.body {
            EntryItemBody::<_>::Tokens(a) => {
                let token = a.token(field_key).ok_or(Error::TokenNotFound {
                    token_id: field_key,
                    //instance_id: self.instance_id,
                    //board_instance_mask: self.board_instance_mask,
                })?;
                assert!(token.id() == field_key);
                let token_value = token.value();
                Ok(token_value)
            }
            _ => Err(Error::EntryTypeMismatch),
        }
    }
}

/// Automatically impl getters (and setters) for the fields where there was
/// "get" (and "set") specified. The getters and setters so generated are
/// hardcoded as calling from_u32() and to_u32(), respectively. Variant syntax:
/// [ATTRIBUTES]
/// NAME(TYPE, default DEFAULT_VALUE, id TOKEN_ID) = KEY: pub get TYPE [: pub
/// set TYPE]
/// The ATTRIBUTES (`#`...) make it into the resulting enum variant.
/// We ensure that MINIMAL_VERSION <= abl0_version < FRONTIER_VERSION at
/// runtime.
macro_rules! make_token_accessors {(
    $(#[$enum_meta:meta])*
    $enum_vis:vis enum $enum_name:ident: {$field_entry_id:expr} {
        $(
            $(#[$field_meta:meta])*
            $field_vis:vis
            $field_name:ident(
              $(minimal_version $minimal_version:expr,
                frontier_version $frontier_version:expr,)?
              default $field_default_value:expr, id $field_key:expr)
              | $getter_vis:vis
              get $field_user_ty:ty
              $(: $setter_vis:vis set $field_setter_user_ty:ty)?
        ),* $(,)?
    }
) => (
    $(#[$enum_meta])*
    #[derive(Debug)] // TODO: EnumString
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    $enum_vis enum $enum_name {
        $(
         $(#[$field_meta])*
         $field_name($field_user_ty), // = $field_key
        )*
    }
    impl core::convert::TryFrom<&TOKEN_ENTRY> for $enum_name {
        type Error = Error;
        fn try_from(entry: &TOKEN_ENTRY) -> core::result::Result<Self, Self::Error> {
          let tag = entry.key.get();
          let value = entry.value.get();
          $(
            if ($field_key == tag) {
                let value = <$field_user_ty>::from_u32(value).ok_or(Error::TokenRange { token_id: tag })?;
                Ok(Self::$field_name(value))
            } else
          )*{
                Err(Error::TokenNotFound {
                    token_id: tag,
                    //entry_id: TokenEntryId::from(entry),
                    //instance_id
                    //board_instance_mask
                })
            }
        }
    }
    impl core::convert::TryFrom<&$enum_name> for TOKEN_ENTRY {
        type Error = Error;
        fn try_from(x: &$enum_name) -> core::result::Result<Self, Self::Error> {
          $(
            if let $enum_name::$field_name(value) = x {
                let key: u32 = $field_key;
                Ok(Self {
                   key: key.into(),
                   value: value.to_u32().ok_or(Error::TokenRange { token_id: key })?.into(),
                })
            } else
          )*{
                panic!("having a token enum variant for something we don't have a key for should be impossible--since both are generated by the same macro.  But it just happened.")
            }
        }
    }
    impl $enum_name {
      #[allow(unused_variables)]
      pub(crate) fn valid_for_abl0_raw(abl0_version: u32, field_key: u32) -> bool {
       $(
           if (field_key == $field_key) {
             $(
               #[allow(unused_comparisons)]
               return ($minimal_version..$frontier_version).contains(&abl0_version);
             )?
             #[allow(unreachable_code)]
             return true
           } else
       )*
       {
           // We default to VALID for things that have no declaration at all.
           //
           // This is in order to simplify both bringup of a new generation
           // and also to allow the user to temporarily add debug token that
           // we don't statically know. Downside is that you can add nonsense
           // tokens and we will not complain.
           true
       }
      }
      pub fn valid_for_abl0(&self, abl0_version: u32) -> core::result::Result<bool, Error> {
          let token_entry = TOKEN_ENTRY::try_from(self)?;
          Ok(Self::valid_for_abl0_raw(abl0_version, token_entry.key.get()))
      }
    }
)}

pub(crate) use make_token_accessors;
