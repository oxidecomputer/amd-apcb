#![macro_use]

use num_traits::{FromPrimitive, ToPrimitive};
use crate::types::Result;
use crate::types::Error;
use crate::apcb::Apcb;
use crate::ondisk::PriorityLevels;
use crate::ondisk::GroupId;

pub struct TokensMut<'a> {
    pub(crate) apcb: &'a mut Apcb<'a>,
    pub(crate) instance_id: u16,
    pub(crate) board_instance_mask: u16,
    pub(crate) priority_mask: PriorityLevels,
}
pub struct Tokens<'a> {
    pub(crate) apcb: &'a mut Apcb<'a>,
    pub(crate) instance_id: u16,
    pub(crate) board_instance_mask: u16,
    //pub(crate) priority_mask: PriorityLevels,
}

impl<'a> TokensMut<'a> {
    pub(crate) fn new(apcb: &'a mut Apcb<'a>, instance_id: u16, board_instance_mask: u16, priority_mask: PriorityLevels) -> Result<TokensMut<'a>> {
        match apcb.insert_group(GroupId::Token, *b"TOKN") {
            Err(Error::GroupUniqueKeyViolation) => {
            },
            Err(x) => {
                return Err(x);
            },
            _ => {
            },
        };
        Ok(TokensMut {
            apcb,
            instance_id,
            board_instance_mask,
            priority_mask,
        })
    }
}

/// Automatically impl getters (and setters) for the fields where there was "get" (and "set") specified.
/// The getters and setters so generated are hardcoded as calling get1() and to_u32(), respectively.
/// Variant syntax:   NAME(TYPE, default DEFAULT_VALUE, id TOKEN_ID) = KEY[: pub get TYPE [: pub set TYPE]]
macro_rules! make_token_accessors {(
        $(
            $(#[$field_meta:meta])*
            $field_vis:vis
            $field_name:ident($field_ty:expr, default $field_default_value:expr, id $field_key:expr) $(: $getter_vis:vis get $field_user_ty:ty $(: $setter_vis:vis set $field_setter_user_ty:ty)?)?
        ),* $(,)?
) => (
    impl<'a> TokensMut<'a> {
        $($(
            #[inline]
            $getter_vis
            fn $field_name (self: &'_ Self)
                -> $field_user_ty
            {
                let group = self.apcb.group(GroupId::Token).ok_or_else(|| Error::GroupNotFound)?;
                let entry = group.entry(EntryId::Token($field_ty), self.instance_id, self.board_instance_mask).ok_or_else(|| Error::EntryNotFound)?;
                // FIXME: match properly (body_token does not check whether the thing is a token entry in the first place)
                match entry.body_token($field_key) {
                    None => {
                        ($field_default_value as u32).get1()
                    },
                    Some(ref token) => {
                        assert!(token.id() == $field_key);
                        token.value().get1()
                    },
                }
            }
            $(
              paste! {
                  #[inline]
                  $setter_vis
                  fn [<set_ $field_name>] (self: &'_ mut Self, value: $field_setter_user_ty) -> Result<()> {
                      let entry_id = EntryId::Token($field_ty);
                      let token_id = $field_key;
                      let token_value = value.to_u32().ok_or_else(|| Error::EntryTypeMismatch)?;
                      match self.apcb.insert_token(entry_id, self.instance_id, self.board_instance_mask, token_id, token_value) {
                          Err(Error::EntryNotFound) => {
                              match self.apcb.insert_entry(entry_id, self.instance_id, self.board_instance_mask, ContextType::Tokens, self.priority_mask, &[]) {
                                  Err(Error::EntryUniqueKeyViolation) => {
                                  },
                                  Err(x) => {
                                      return Err(x);
                                  },
                                  _ => {
                                  },
                              };
                              self.apcb.insert_token(entry_id, self.instance_id, self.board_instance_mask, token_id, token_value)?;
                          },
                          Err(Error::TokenUniqueKeyViolation) => {
                              let mut group = self.apcb.group_mut(GroupId::Token).unwrap();
                              let mut entry = group.entry_mut(entry_id, self.instance_id, self.board_instance_mask).unwrap();
                              // FIXME: match properly (body_token_mut does not check whether the thing is a token entry in the first place)
                              let mut token = entry.body_token_mut(token_id).unwrap();
                              token.set_value(token_value)?;
                          },
                          Err(x) => {
                              return Err(x);
                          },
                          _ => { // inserted new token, and set its value
                          },
                      }
                      Ok(())
                  }
              }
            )?
        )?)*
    }
)}

pub(crate) use make_token_accessors;
