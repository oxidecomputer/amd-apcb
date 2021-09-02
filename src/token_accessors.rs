#![macro_use]

use num_traits::{FromPrimitive, ToPrimitive};
use crate::tokens_entry::TokensEntryBodyItem;

pub struct TokensMut<'a> {
    pub(crate) u8: TokensEntryBodyItem<&'a mut [u8]>,
}
pub struct Tokens<'a> {
    body: TokensEntryBodyItem<&'a [u8]>,
}

/// Automatically impl getters (and setters) for the fields where there was "get" (and "set") specified.
/// The getters and setters so generated are hardcoded as calling get1() and to_u32(), respectively.
/// Variant syntax:   NAME(TYPE, default DEFAULT_VALUE, id TOKEN_ID) = KEY[: pub get TYPE [: pub set TYPE]]
macro_rules! make_token_accessors {(
        $(
            $(#[$field_meta:meta])*
            $field_vis:vis
            $field_name:ident($field_ty:ident, default $field_default_value:expr, id $field_key:expr) $(: $getter_vis:vis get $field_user_ty:ty $(: $setter_vis:vis set $field_setter_user_ty:ty)?)?
        ),* $(,)?
) => (
    impl<'a> TokensMut<'a> {
        $($(
            #[inline]
            $getter_vis
            fn $field_name (self: &'_ Self)
                -> $field_user_ty
            {
                match self.$field_ty.token($field_key) {
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
                      let mut entry = self.$field_ty.token_mut($field_key).ok_or_else(|| Error::TokenNotFound)?;
                      entry.set_value(value.to_u32().ok_or_else(|| Error::EntryTypeMismatch)?)
                  }
              }
            )?
        )?)*
    }
)}

pub(crate) use make_token_accessors;
