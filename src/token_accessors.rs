#![macro_use]

use crate::apcb::Apcb;
use crate::entry::EntryItemBody;
use crate::ondisk::BoardInstances;
use crate::ondisk::GroupId;
use crate::ondisk::PriorityLevels;
use crate::ondisk::{ContextType, EntryId, TokenEntryId};
use crate::types::Error;
use crate::types::Result;

pub struct TokensMut<'a, 'b> {
    pub(crate) apcb: &'b mut Apcb<'a>,
    pub(crate) instance_id: u16,
    pub(crate) board_instance_mask: BoardInstances,
    pub(crate) priority_mask: PriorityLevels,
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
    ) -> Result<Self> {
        match apcb.insert_group(GroupId::Token, *b"TOKN") {
            Err(Error::GroupUniqueKeyViolation) => {}
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
        })
    }

    pub(crate) fn get(
        &self,
        token_entry_id: TokenEntryId,
        field_key: u32,
    ) -> Result<u32> {
        let group = self
            .apcb
            .group(GroupId::Token)
            .ok_or_else(|| Error::GroupNotFound)?;
        let entry = group
            .entry_exact(
                EntryId::Token(token_entry_id),
                self.instance_id,
                self.board_instance_mask,
            )
            .ok_or_else(|| Error::EntryNotFound)?;
        match &entry.body {
            EntryItemBody::<_>::Tokens(ref a) => {
                let token =
                    a.token(field_key).ok_or_else(|| Error::TokenNotFound)?;
                assert!(token.id() == field_key);
                let token_value = token.value();
                Ok(token_value)
            }
            _ => {
                return Err(Error::EntryTypeMismatch);
            }
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
            Err(Error::EntryNotFound) => {
                match self.apcb.insert_entry(
                    entry_id,
                    self.instance_id,
                    self.board_instance_mask,
                    ContextType::Tokens,
                    self.priority_mask,
                    &[],
                ) {
                    Err(Error::EntryUniqueKeyViolation) => {}
                    Err(x) => {
                        return Err(x);
                    }
                    _ => {}
                };
                self.apcb.insert_token(
                    entry_id,
                    self.instance_id,
                    self.board_instance_mask,
                    token_id,
                    token_value,
                )?;
            }
            Err(Error::TokenUniqueKeyViolation) => {
                let mut group = self.apcb.group_mut(GroupId::Token).unwrap();
                let mut entry = group
                    .entry_exact_mut(
                        entry_id,
                        self.instance_id,
                        self.board_instance_mask,
                    )
                    .unwrap();
                match &mut entry.body {
                    EntryItemBody::<_>::Tokens(ref mut a) => {
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
        Ok(Self {
            apcb,
            instance_id,
            board_instance_mask,
        })
    }

    pub(crate) fn get(
        &self,
        token_entry_id: TokenEntryId,
        field_key: u32,
    ) -> Result<u32> {
        let group = self
            .apcb
            .group(GroupId::Token)
            .ok_or_else(|| Error::GroupNotFound)?;
        let entry = group
            .entry_exact(
                EntryId::Token(token_entry_id),
                self.instance_id,
                self.board_instance_mask,
            )
            .ok_or_else(|| Error::EntryNotFound)?;
        match &entry.body {
            EntryItemBody::<_>::Tokens(ref a) => {
                let token =
                    a.token(field_key).ok_or_else(|| Error::TokenNotFound)?;
                assert!(token.id() == field_key);
                let token_value = token.value();
                Ok(token_value)
            }
            _ => {
                return Err(Error::EntryTypeMismatch);
            }
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
macro_rules! make_token_accessors {(
    $(#[$enum_meta:meta])*
    $enum_vis:vis enum $enum_name:ident: {$field_entry_id:expr} {
        $(
            $(#[$field_meta:meta])*
            $field_vis:vis
            $field_name:ident(default $field_default_value:expr, id $field_key:expr) : $getter_vis:vis get $field_user_ty:ty $(: $setter_vis:vis set $field_setter_user_ty:ty)?
        ),* $(,)?
    }
) => (
    $(#[$enum_meta])*
    #[derive(Debug)] // TODO: EnumString
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    $enum_vis enum $enum_name {
        $(
         $(#[$field_meta])*
         $field_name($field_user_ty),
        )*
    }
    impl core::convert::TryFrom<&TOKEN_ENTRY> for $enum_name {
        type Error = Error;
        fn try_from(entry: &TOKEN_ENTRY) -> core::result::Result<Self, Self::Error> {
          let tag = entry.key.get();
          let value = entry.value.get();
          $(
            if ($field_key == tag) {
                let value = <$field_user_ty>::from_u32(value).ok_or(Error::TokenRange)?;
                Ok(Self::$field_name(value))
            } else
          )*{
                Err(Error::TokenNotFound)
            }
        }
    }
    impl core::convert::TryFrom<$enum_name> for TOKEN_ENTRY {
        type Error = Error;
        fn try_from(x: $enum_name) -> core::result::Result<Self, Self::Error> {
          $(
            if let $enum_name::$field_name(value) = x {
                Ok(Self {
                   key: $field_key.into(),
                   value: value.to_u32().ok_or(Error::TokenRange)?.into(),
                })
            } else
          )*{
                Err(Error::TokenNotFound)
            }
        }
    }
    $(
            impl<'a, 'b> Tokens<'a, 'b> {
                paste! {
                  #[allow(non_snake_case)]
                  #[inline]
                  $getter_vis
                  fn [<$field_name:snake>] (self: &'_ Self)
                      -> Result<$field_user_ty>
                  {
                      <$field_user_ty>::from_u32(self.get($field_entry_id, $field_key)?).ok_or_else(|| Error::EntryTypeMismatch)
                  }
                }
            }
            impl<'a, 'b> TokensMut<'a, 'b> {

            paste! {
              #[allow(non_snake_case)]
              #[inline]
              $getter_vis
              fn [<$field_name:snake>] (self: &'_ Self)
                  -> Result<$field_user_ty>
              {
                  <$field_user_ty>::from_u32(self.get($field_entry_id, $field_key)?).ok_or_else(|| Error::EntryTypeMismatch)
              }
            }
            $(
              paste! {
                  #[allow(non_snake_case)]
                  #[inline]
                  $setter_vis
                  fn [<set_ $field_name:snake>] (self: &'_ mut Self, value: $field_setter_user_ty) -> Result<()> {
                      let token_value = value.to_u32().unwrap();
                      self.set($field_entry_id, $field_key, token_value)
                  }
              }
            )?
            }
    )*
)}

pub(crate) use make_token_accessors;
