#![macro_use]

use num_traits::{FromPrimitive, ToPrimitive};
use crate::types::Error;
use crate::types::Result;
use crate::tokens_entry::TokensEntryBodyItem;

pub struct TokensMut<'a> {
    body: TokensEntryBodyItem<&'a mut [u8]>,
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
                let entry = match self.body.token($field_key) {
                    None => {
                        return T::from_u32($field_default_value);
                    },
                    x => x,
                };
                assert!(entry.id() == token_id);
                T::from_u32(entry.value())
            }
            $(
              paste! {
                  #[inline]
                  $setter_vis
                  fn [<set_ $field_name>] (self: &'_ mut Self, value: $field_setter_user_ty) -> Result<()> {
                      let entry = self.body.token(token_id).ok_or_else(|| Error::TokenNotFound)?;
                      entry.set_value(value.to_u32())
                  }
              }
            )?
        )?)*
    }
)}

pub(crate) use make_token_accessors;
