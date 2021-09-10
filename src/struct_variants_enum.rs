#![macro_use]

/// This macro expects module contents as a parameter, and then, first, defines the exact same contents.  Then it generates an enum with all the structs available in that module.
macro_rules! collect_EntryCompatible_impl_into_enum {
    ({$($state:tt)*}{$($state_mut:tt)*}
    ) => {
        pub enum RefTags<'a> {
             $($state)*
        }
        pub enum MutRefTags<'a> {
             $($state_mut)*
        }
    };
    ({$($state:tt)*}{$($state_mut:tt)*}
        $(#[$struct_meta:meta])*
        impl EntryCompatible for $struct_name:ident {
            $($impl_body:tt)*
        }
        $($tail:tt)*
    ) => {
        $(#[$struct_meta])*
        impl EntryCompatible for $struct_name {
            $($impl_body)*
        }
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!({$struct_name(&'a $struct_name), $($state)*}{$struct_name(&'a mut $struct_name), $($state_mut)*}
        $($tail)*);
    };
    ({$($state:tt)*}{$($state_mut:tt)*}
        // Who could possibly want non-eager evaluation here?  Sigh.
        impl_EntryCompatible!($struct_name:ident, $($args:tt)*);
        $($tail:tt)*
    ) => {
        impl_EntryCompatible!($struct_name, $($args)*);
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!({$struct_name(&'a $struct_name), $($state)*}{$struct_name(&'a mut $struct_name), $($state_mut)*}
        $($tail)*);
    };
    ({$($state:tt)*}{$($state_mut:tt)*}
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

        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!({$($state)*}{$($state_mut)*}
        $($tail)*);
    };
    ({$($state:tt)*}{$($state_mut:tt)*}
        $head:item
        $($tail:tt)*
    ) => {
        $head
        $crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum!({$($state)*}{$($state_mut)*}
        $($tail)*);
    };
}

pub(crate) use collect_EntryCompatible_impl_into_enum;
