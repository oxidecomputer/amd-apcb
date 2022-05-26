#![macro_use]

/// This macro expects module contents as a parameter, and then, first, defines
/// the exact same contents.  Then it generates two enums with all the items
/// that implement EntryCompatible available in that module.  It then implements
/// SequenceElementFromBytes for the enum.
macro_rules! collect_EntryCompatible_impl_into_enum {
    // This provides the deserializer's type matching.
    (@match2 {$type_:ident}{$skip_step:ident}{$xbuf:ident}) => {
        {
            let (raw_value, b) = $xbuf.split_at($skip_step);
            $xbuf = b;
            (Self::Unknown(raw_value), $xbuf)
        }
    };
    (@match2 {$type_:ident}{$skip_step:ident}{$xbuf:ident} $struct_name:ident; $($tail:tt)*) => {
        if $skip_step == core::mem::size_of::<$struct_name>() && $type_ == <$struct_name>::TAG {
            (Self::$struct_name(take_header_from_collection::<$struct_name>(&mut $xbuf).ok_or_else(|| Error::EntryTypeMismatch)?), $xbuf)
        } else {
            $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@match2 {$type_}{$skip_step}{$xbuf}$($tail)*)
        }
    };
    (@match1 {$entry_id:ident}{$world:ident}{$($deserializer:tt)*}) => {
        {
            let (type_, skip_step) = Self::skip_step($entry_id, $world).ok_or_else(|| Error::EntryTypeMismatch)?;
            let mut xbuf = core::mem::take(&mut *$world);
            $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@match2 {type_}{skip_step}{xbuf}$($deserializer)*)
        }
    };
    (@match2mut {$type_:ident}{$skip_step:ident}{$xbuf:ident}) => {
        {
            let (raw_value, b) = $xbuf.split_at_mut($skip_step);
            $xbuf = b;
            (Self::Unknown(raw_value), $xbuf)
        }
    };
    (@match2mut {$type_:ident}{$skip_step:ident}{$xbuf:ident} $struct_name:ident; $($tail:tt)*) => {
        if $skip_step == core::mem::size_of::<$struct_name>() && $type_ == <$struct_name>::TAG {
            (Self::$struct_name(take_header_from_collection_mut::<$struct_name>(&mut $xbuf).ok_or_else(|| Error::EntryTypeMismatch)?), $xbuf)
        } else {
            $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@match2mut {$type_}{$skip_step}{$xbuf}$($tail)*)
        }
    };
    (@match1mut {$entry_id:ident}{$world:ident}{$($deserializer:tt)*}) => {
        {
            let (type_, skip_step) = Self::skip_step($entry_id, $world).ok_or_else(|| Error::EntryTypeMismatch)?;
            let mut xbuf = core::mem::take(&mut *$world);
            $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@match2mut {type_}{skip_step}{xbuf}$($deserializer)*)
        }
    };
    (@machine {$($deserializer:tt)*}{$($state:tt)*}{$($state_mut:tt)*}{$($state_obj:tt)*}{$($as_bytes:tt)*}
    ) => {
        #[non_exhaustive]
        #[derive(Debug)]
        #[cfg_attr(feature = "serde", derive(Serialize))]
        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
        pub enum ElementRef<'a> {
             Unknown(&'a [u8]),
             $($state)*
        }
        #[non_exhaustive]
        #[derive(Debug)]
        pub enum MutElementRef<'a> {
             Unknown(&'a mut [u8]),
             $($state_mut)*
        }

        #[cfg(feature = "serde")]
        #[non_exhaustive]
        #[derive(Serialize, Deserialize)]
        #[repr(C)]
        pub enum Element {
             Unknown(Vec<u8>),
             $($state_obj)*
        }

        impl<'a> SequenceElementFromBytes<'a> for ElementRef<'a> {
            fn checked_from_bytes(entry_id: EntryId, world: &mut &'a [u8]) -> Result<Self> {
                let (result, xbuf) = $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@match1 {entry_id}{world}{$($deserializer)*});
                (*world) = xbuf;
                Ok(result)
            }
        }
        impl<'a> MutSequenceElementFromBytes<'a> for MutElementRef<'a> {
            fn checked_from_bytes(entry_id: EntryId, world: &mut &'a mut [u8]) -> Result<Self> {
                let (result, xbuf) = $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@match1mut {entry_id}{world}{$($deserializer)*});
                (*world) = xbuf;
                Ok(result)
            }
        }

        #[cfg(feature = "std")]
        impl ElementAsBytes for Element {
            fn element_as_bytes(&self) -> &[u8] {
                match self {
                    Element::Unknown(vec) => vec.as_slice(),
                    $($as_bytes)*
                }
            }
        }
    };
    (@machine {$($deserializer:tt)*}{$($state:tt)*}{$($state_mut:tt)*}{$($state_obj:tt)*}{$($as_bytes:tt)*}
        $(#[$struct_meta:meta])*
        impl EntryCompatible for $struct_name:ident {
            $($impl_body:tt)*
        }
        $($tail:tt)*
    ) => {
        impl<'a> From<&'a $struct_name> for ElementRef<'a> {
            fn from(from: &'a $struct_name) -> Self {
                Self::$struct_name(from)
            }
        }
        impl<'a> From<&'a mut $struct_name> for MutElementRef<'a> {
            fn from(from: &'a mut $struct_name) -> Self {
                Self::$struct_name(from)
            }
        }
        $(#[$struct_meta])*
        impl EntryCompatible for $struct_name {
            $($impl_body)*
        }
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {$struct_name; $($deserializer)*}{$struct_name(&'a $struct_name), $($state)*}{$struct_name(&'a mut $struct_name), $($state_mut)*}{$struct_name($struct_name), $($state_obj)*}{Element::$struct_name($struct_name) => $struct_name.as_bytes(), $($as_bytes)*}
        $($tail)*);
    };
    // Who could possibly want non-eager evaluation here?  Sigh.
    (@machine {$($deserializer:tt)*}{$($state:tt)*}{$($state_mut:tt)*}{$($state_obj:tt)*}{$($as_bytes:tt)*}
        impl_EntryCompatible!($struct_name:ident, $($args:tt)*);
        $($tail:tt)*
    ) => {
        impl<'a> From<&'a $struct_name> for ElementRef<'a> {
            fn from(from: &'a $struct_name) -> Self {
                Self::$struct_name(from)
            }
        }
        impl<'a> From<&'a mut $struct_name> for MutElementRef<'a> {
            fn from(from: &'a mut $struct_name) -> Self {
                Self::$struct_name(from)
            }
        }
        impl_EntryCompatible!($struct_name, $($args)*);
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {$struct_name; $($deserializer)*}{$struct_name(&'a $struct_name), $($state)*}{$struct_name(&'a mut $struct_name), $($state_mut)*}{$struct_name($struct_name), $($state_obj)*}{Element::$struct_name($struct_name) => $struct_name.as_bytes(), $($as_bytes)*}
        $($tail)*);
    };
    (@machine {$($deserializer:tt)*}{$($state:tt)*}{$($state_mut:tt)*}{$($state_obj:tt)*}{$($as_bytes:tt)*}
        $(#[$struct_meta:meta])*
        $struct_vis:vis
        struct $struct_name:ident {
            $($struct_body:tt)*
        }
        $($tail:tt)*
    ) => {
        $(#[$struct_meta])*
        $struct_vis
        struct $struct_name { $($struct_body)* }

        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {$($deserializer)*}{$($state)*}{$($state_mut)*}{$($state_obj)*}{$($as_bytes)*}
        $($tail)*);
    };
    // Who could possibly want non-eager evaluation here?  Sigh.
    (@machine {$($deserializer:tt)*}{$($state:tt)*}{$($state_mut:tt)*}{$($state_obj:tt)*}{$($as_bytes:tt)*}
        make_bitfield_serde! {
            $(#[$struct_meta:meta])*
            $struct_vis:vis
            struct $struct_name:ident {
                $($struct_body:tt)*
            }
        }
        $($tail:tt)*
    ) => {
        make_bitfield_serde! {
            $(#[$struct_meta])*
            $struct_vis
            struct $struct_name { $($struct_body)* }
        }
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {$($deserializer)*}{$($state)*}{$($state_mut)*}{$($state_obj)*}{$($as_bytes)*}
        $($tail)*);
    };
    (@machine {$($deserializer:tt)*}{$($state:tt)*}{$($state_mut:tt)*}{$($state_obj:tt)*}{$($as_bytes:tt)*}
        $head:item
        $($tail:tt)*
    ) => {
        $head
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {$($deserializer)*}{$($state)*}{$($state_mut)*}{$($state_obj)*}{$($as_bytes)*}
        $($tail)*);
    };
    ($($tts:tt)*) => {
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {}{}{}{}{} $($tts)*);
    };
}

pub(crate) use collect_EntryCompatible_impl_into_enum;
