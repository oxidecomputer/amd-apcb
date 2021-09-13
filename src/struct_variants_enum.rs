#![macro_use]

/// This macro expects module contents as a parameter, and then, first, defines the exact same contents.  Then it generates two enums with all the items that implement EntryCompatible available in that module.
macro_rules! collect_EntryCompatible_impl_into_enum {
    (@machine {$($state:tt)*}{$($state_mut:tt)*}
    ) => {
        #[non_exhaustive]
        pub enum RefTags<'a> {
             Unknown(&'a [u8]),
             $($state)*
        }
        #[non_exhaustive]
        pub enum MutRefTags<'a> {
             Unknown(&'a mut [u8]),
             $($state_mut)*
        }
    };
    (@machine {$($state:tt)*}{$($state_mut:tt)*}
        $(#[$struct_meta:meta])*
        impl EntryCompatible for $struct_name:ident {
            $($impl_body:tt)*
        }
        $($tail:tt)*
    ) => {
        impl<'a> From<&'a $struct_name> for RefTags<'a> {
            fn from(from: &'a $struct_name) -> RefTags<'a> {
                RefTags::$struct_name(from)
            }
        }
        impl<'a> From<&'a mut $struct_name> for MutRefTags<'a> {
            fn from(from: &'a mut $struct_name) -> MutRefTags<'a> {
                MutRefTags::$struct_name(from)
            }
        }
        $(#[$struct_meta])*
        impl EntryCompatible for $struct_name {
            $($impl_body)*
        }
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {$struct_name(&'a $struct_name), $($state)*}{$struct_name(&'a mut $struct_name), $($state_mut)*}
        $($tail)*);
    };
    // Who could possibly want non-eager evaluation here?  Sigh.
    (@machine {$($state:tt)*}{$($state_mut:tt)*}
        impl_EntryCompatible!($struct_name:ident, $($args:tt)*);
        $($tail:tt)*
    ) => {
        impl<'a> From<&'a $struct_name> for RefTags<'a> {
            fn from(from: &'a $struct_name) -> RefTags<'a> {
                RefTags::$struct_name(from)
            }
        }
        impl<'a> From<&'a mut $struct_name> for MutRefTags<'a> {
            fn from(from: &'a mut $struct_name) -> MutRefTags<'a> {
                MutRefTags::$struct_name(from)
            }
        }
        impl_EntryCompatible!($struct_name, $($args)*);
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {$struct_name(&'a $struct_name), $($state)*}{$struct_name(&'a mut $struct_name), $($state_mut)*}
        $($tail)*);
    };
    (@machine {$($state:tt)*}{$($state_mut:tt)*}
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

        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {$($state)*}{$($state_mut)*}
        $($tail)*);
    };
    (@machine {$($state:tt)*}{$($state_mut:tt)*}
        $head:item
        $($tail:tt)*
    ) => {
        $head
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {$($state)*}{$($state_mut)*}
        $($tail)*);
    };
    ($($tts:tt)*) => {
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!(@machine {} {} $($tts)*);
    };
}

pub(crate) use collect_EntryCompatible_impl_into_enum;
