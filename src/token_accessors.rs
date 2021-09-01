#![macro_use]

use num_traits::{FromPrimitive, ToPrimitive};
use crate::types::Error;
use crate::types::Result;
use crate::tokens_entry::TokensEntryBodyItem;
use crate::struct_accessors::Getter;

pub struct TokensMut<'a> {
    pub(crate) body: TokensEntryBodyItem<&'a mut [u8]>,
}
impl<'a> TokensMut<'a> {
    pub(crate) fn new(body: TokensEntryBodyItem<&'a mut [u8]>) -> Self {
        Self {
            body,
        }
    }
}
pub struct Tokens<'a> {
    body: TokensEntryBodyItem<&'a [u8]>,
}

/// This macro expects an enum as a parameter (attributes are fine) and then, first, defines the exact same enum.
/// Afterwards, it automatically impl getters (and setters) for the fields where there was "get" (and "set") specified.
/// The getters and setters so generated are hardcoded as calling from_u32 and to_u32, respectively.
/// Note: If you want to add a docstring, put it before the struct, inside the macro parameter at the usage site, not before the macro call.
/// Variant syntax:   NAME(TYPE, DEFAULT_VALUE) = KEY[: pub get TYPE [: pub set TYPE]]
macro_rules! make_token_accessors {(
    $(#[$struct_meta:meta])*
    $struct_vis:vis
    enum $EnumName:ident {
        $(
            $(#[$field_meta:meta])*
            $field_vis:vis
            $field_name:ident($field_ty:ty, default $field_default_value:expr, id $field_key:expr) $(: $getter_vis:vis get $field_user_ty:ty $(: $setter_vis:vis set $field_setter_user_ty:ty)?)?
        ),* $(,)?
    }
) => (
/*
    $(#[$struct_meta])*
    $struct_vis
    enum $EnumName {
        $(
            $(#[$field_meta])*
            $field_vis
            $field_name = $field_key,
        )*
    }
*/

    impl<'a> TokensMut<'a> {
        $($(
            #[inline]
            $getter_vis
            fn $field_name (self: &'_ Self)
                -> $field_user_ty
            {
                match self.body.token($field_key) {
                    None => {
                        ($field_default_value as u32).get1()
                    },
                    Some(ref entry) => {
                        assert!(entry.id() == $field_key);
                        entry.value().get1()
                    },
                }
            }
            $(
              paste! {
                  #[inline]
                  $setter_vis
                  fn [<set_ $field_name>] (self: &'_ mut Self, value: $field_setter_user_ty) -> Result<()> {
                      let mut entry = self.body.token_mut($field_key).ok_or_else(|| Error::TokenNotFound)?;
                      entry.set_value(value.to_u32().ok_or_else(|| Error::EntryTypeMismatch)?)
                  }
              }
            )?
        )?)*
    }
)}

pub(crate) use make_token_accessors;
