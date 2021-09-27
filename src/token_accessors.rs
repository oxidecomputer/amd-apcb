#![macro_use]

use crate::apcb::Apcb;
use crate::ondisk::GroupId;
use crate::ondisk::PriorityLevels;
use crate::types::Error;
use crate::types::Result;

pub struct TokensMut<'a> {
    pub(crate) apcb: &'a mut Apcb<'a>,
    pub(crate) instance_id: u16,
    pub(crate) board_instance_mask: u16,
    pub(crate) priority_mask: PriorityLevels,
}
pub struct Tokens<'a> {
    pub(crate) apcb: &'a Apcb<'a>,
    pub(crate) instance_id: u16,
    pub(crate) board_instance_mask: u16,
    //pub(crate) priority_mask: PriorityLevels,
}

impl<'a> TokensMut<'a> {
    pub(crate) fn new(
        apcb: &'a mut Apcb<'a>,
        instance_id: u16,
        board_instance_mask: u16,
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
}

impl<'a> Tokens<'a> {
    pub(crate) fn new(
        apcb: &'a Apcb<'a>,
        instance_id: u16,
        board_instance_mask: u16,
    ) -> Result<Self> {
        Ok(Self {
            apcb,
            instance_id,
            board_instance_mask,
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
            $field_name:ident($field_entry_id:expr, default $field_default_value:expr, id $field_key:expr) $(: $getter_vis:vis get $field_user_ty:ty $(: $setter_vis:vis set $field_setter_user_ty:ty)?)?
        ),* $(,)?
) => (
    $(
        $(
            impl<'a> Tokens<'a> {
            #[inline]
            $getter_vis
            fn $field_name (self: &'_ Self)
                -> Result<$field_user_ty>
            {
                let group = self.apcb.group(GroupId::Token).ok_or_else(|| Error::GroupNotFound)?;
                let entry = group.entry_exact(EntryId::Token($field_entry_id), self.instance_id, self.board_instance_mask).ok_or_else(|| Error::EntryNotFound)?;
                match &entry.body {
                    EntryItemBody::<_>::Tokens(ref a) => {
                        let token = a.token($field_key).ok_or_else(|| Error::TokenNotFound)?;
                        assert!(token.id() == $field_key);
                        let token_value = token.value();
                        <$field_user_ty>::from_u32(token_value).ok_or_else(|| Error::EntryTypeMismatch)
                    },
                    _ => {
                        return Err(Error::EntryTypeMismatch);
                    },
                }
            }
            }
            impl<'a> TokensMut<'a> {
            #[inline]
            $getter_vis
            fn $field_name (self: &'_ Self)
                -> Result<$field_user_ty>
            {
                let group = self.apcb.group(GroupId::Token).ok_or_else(|| Error::GroupNotFound)?;
                let entry = group.entry_exact(EntryId::Token($field_entry_id), self.instance_id, self.board_instance_mask).ok_or_else(|| Error::EntryNotFound)?;
                match &entry.body {
                    EntryItemBody::<_>::Tokens(ref a) => {
                        let token = a.token($field_key).ok_or_else(|| Error::TokenNotFound)?;
                        assert!(token.id() == $field_key);
                        let token_value = token.value();
                        <$field_user_ty>::from_u32(token_value).ok_or_else(|| Error::EntryTypeMismatch)
                    },
                    _ => {
                        return Err(Error::EntryTypeMismatch);
                    },
                }
            }
            $(
              paste! {
                  #[inline]
                  $setter_vis
                  fn [<set_ $field_name>] (self: &'_ mut Self, value: $field_setter_user_ty) -> Result<()> {
                      let entry_id = EntryId::Token($field_entry_id);
                      let token_id = $field_key;
                      let token_value = value.to_u32().unwrap();
                      //let token_value = value.to_u32().ok_or_else(|| Error::EntryTypeMismatch)?;
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
                              let mut entry = group.entry_exact_mut(entry_id, self.instance_id, self.board_instance_mask).unwrap();
                              match &mut entry.body {
                                  EntryItemBody::<_>::Tokens(ref mut a) => {
                                      let mut token = a.token_mut(token_id).unwrap();
                                      token.set_value(token_value)?;
                                  },
                                  _ => {
                                      return Err(Error::EntryTypeMismatch);
                                  },
                              }
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
            }
        )?
    )*
)}

pub(crate) use make_token_accessors;
