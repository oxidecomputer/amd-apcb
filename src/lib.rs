// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!

This crate allows you to manipulate APCB (AGESA PSP Configuration Blob),
directly in an image (u8 slice).

# Usage

Add

    amd-apcb = { path = "../amd-apcb", default_features = false, features = [] }

to the `[dependencies]` block in your `Cargo.toml`.

To iterate, you can do:

    let mut buffer: [u8; 8*1024] = ... load from file;
    let apcb = Apcb::load(&mut buffer[0..]).unwrap();
    for group in apcb.groups() {
        for entry in group.entries() {
            ...
        }
    }

To insert a new group:

    apcb.insert_group(GroupId::Psp, *b"PSPG")?;

To delete a group:

    apcb.delete_group(GroupId::Psp)?;

To insert a new entry:

    apcb.insert_entry(EntryId::Psp(PspEntryId::...), 0, 0xFFFF, 0)?;

(Note: There are convenience functions that you can use to insert Struct data:
`insert_struct_entry`, `insert_struct_sequence_as_entry`)

To delete an entry:

    apcb.delete_entry(EntryId::Psp(PspEntryId::...), 0, 0xFFFF)?;

To insert a new token:

    apcb.insert_token(EntryId::Token(TokenEntryId::Byte), 0, 0xFFFF, 0x42, 1)?;

To delete a token:

    apcb.delete_token(EntryId::Token(TokenEntryId::Byte), 0, 0xFFFF, 0x42)?;

If the entry is a struct entry, you can use something like

    let mut (entry, _tail) = entry.body_as_struct_mut::<memory::ExtVoltageControl>()?;
    entry.dimm_slot_present

to have the entry represented as a Rust struct (the `_tail` would be the
variable-length array at the end of the struct, if applicable).
This is only useful for structs whose name doesn't contain "Element" (since
those are the ones without a variable-length payload).

If the entry is a struct array entry (variable-length array), then you can
use something like

    let mut entry = entry.body_as_struct_array_mut::<memory::DimmInfoSmbusElement>()?;
    for element in entry {
        ...
    }

to iterate over it.  This is only useful for structs whose name contains "Element".

In order to update the checksum (you should do that once after any insertion/deletion/mutation):

    apcb.save()?;

Note that this also changes unique_apcb_instance.

# Implementation notes

A library such as this, since we want to have type-safe fields (for example
values as enum variants etc), has to have some layer on top of zerocopy
(the latter of which would just have "this is a little-endian u32"--well,
that's not very expressive).

That is what the macros `make_accessors` and (or rather, xor) `make_bitfield_serde`
add.

We want the APCB serde interface to be as close as possible to the actual
ondisk structs (but using high-level types)--because that makes understanding
and debugging it much easier for the user. That meant that `make_accessors`
and `make_bitfield_serde` grew serde-specific automatically generated accessors,
serde structs (used for serialization and deserialization by serde--all the
ones we generate have `Serde` in the name) and impls Deserialize and Serialize
on the `Serde` (!) struct.

In general you can think of the entire thing to do basically the following
(pub chosen with great care in the following):

```rust
make_accessors! {
   struct Foo {
       a || Quux : Bar | pub get G : pub set S,
   }
}
```

=>

```rust
struct Foo {
    a: Bar
}
impl Foo {
    pub fn a(&self) -> Result<G> {
        self.a.get1()
    }
    pub fn set_a(&mut self, value: S) {
        self.a.set1(value)
    }
    pub fn builder() -> Self {
        Self::default()
    }
    pub fn build(&self) -> Self {
        self.clone()
    }
    pub fn with_a(self: &mut Self, value: S) -> &mut Self {
        let result = self;
        result.a.set1(value);
        result
    }
    fn serde_a(&self) -> Result<Quux> {
        self.a.get1()
    }
    fn serde_with_a(&mut self, value: Quux) -> &mut Self {
        let result = self;
        result.a.set1(value);
        result
    }
}

#[derive(Serialize, Deserialize)]
struct SerdeFoo {
    pub a: Quux
}
```

But it turned out that bitfields already have some of the things that
`make_accessors` generates, hence `make_bitfield_serde` was born--which is
basically `make_accessors` with duplicate things removed.

`make_bitfield_serde` would do:

```rust
make_bitfield_serde! {
   #[bitfield]
   struct Foo {
       // Note: G and S are ignored--but it's there so we have some
       // regularity in our DSL.
       a || Quux : B3 | pub get G : pub set S,
   }
}
```

=>

```rust
#[bitfield]
struct Foo {
    a: B3
}
impl Foo {
    // no getters, setters, builder, with_... --since the bitfields have
    // those automatically
    fn serde_a(&self) -> Result<Quux> {
        self.a()
    }
    fn serde_with_a(&mut self, value: Quux) -> &mut Self {
        self.set_a(value.into());
        self
    }
}

#[derive(Serialize, Deserialize)]
struct SerdeFoo {
    // WARNING:
    // If the `|| Quux` was left off in the input, then it would default to a
    // Rust serde-able type that is at least as big as B3 (the result would be u8).
    // This is specific to make_bitfield_serde and it's the result of me giving up
    // trying to use refinement types in Rust--see ux_serde crate for what
    // could have been. That also means that in the serde config, you can
    // specify a value that doesn't fit into a B3. It will then be truncated
    // and stored without warning (which is what a lot of Rust crates
    // culturally do--see for example register crates, hal crates etc).
    // To prevent truncation is why, whenever possible, we use an
    // enum that derives BitfieldSpecifier instead of things like B3.
    // That way, out of bounds values cannot happen.
    // These enums can be used as `TYPE` directly in `#[bitfield]` fields.
    pub a: Quux
}
```

The builder, with_ and build fns are implementing the builder pattern
(for a nicer user-level interface) in Rust.

make_accessors gets a struct definition as parameter, and the thing below is
one of the fields of the input struct definition. The result is that struct
is generated, and another struct is generated with `Serde` in front of the
struct name and different field types):

`NAME [|| SERDE_TYPE] : TYPE
[| pub get GETTER_RETURN_TYPE [: pub set SETTER_PARAMETER_TYPE]]`

The brackets denote optional parts.
It defines field `NAME` as `TYPE`. `TYPE` usually has to be the lowest-level
machine type (so it's "Little-endian u32", or "3 bits", or similar).
`SERDE_TYPE`, if given, will be the type used for the serde field. If it is
not given, `TYPE` will be used instead.
The "pub get" will use the given GETTER_RETURN_TYPE as the resulting type
of the generated getter, using `Getter` converters to get there as needed.
The "pub set" will use the given SETTER_PARAMETER_TYPE as the parameter
type of the generated setter, using `Setter` converters to get there as
needed.
The `[|| SERDE_TYPE] : TYPE` is weird because of an unfortunate limitation
of the Rust macro system. I've filed bugs upstream, but apparently they
don't consider those bugs (for example rust-lang/rust#96787 ).

In any case, in an ideal world, that `[|| SERDE_TYPE] : TYPE` would just be
`: [SERDE_TYPE | ] TYPE`, but that is not currently possible
(because the Rust macro parser apparently only has a lookahead of 1 token).

In the end, even now, it just means you can leave `|| SERDE_TYPE` off and
it will just use `TYPE` on the serde side as well.

In order to ensure backward-compatibility of the configuration files there
is NO automatic connection between Foo and SerdeFoo. Instead, the user is
supposed to do the connection (and conversion), possibly to ANOTHER custom
serde struct the user wrote manually.

If the conversion is 1:1 after all, there's a macro
`impl_struct_serde_conversion` for simplifying that, in `serializers.rs`.
You give it the two (existing!) struct
names and a list of fields and it will generate converters to and fro
(using the fns serde_... that were generated earlier).
Long term (with changing requirements), those conversions will become less
and less straightforward and more manual and so usage of
`impl_struct_serde_conversion` will decrease.

For tokens, there's a macro make_token_accessors in src/token_accessors.rs.

You give in an DSL-extended-enum (the DSL is designed to be very similar to
the struct one described in the beginning), and an enum with data (different
data type per variant) comes out--and some pub getters and setters
(one per variant).

Because it's guaranteed that all token values are at most `u32` by AMD
(and the lowlevel type is `U32<LittleEndian>`), the token field value
accessors just use `USER_TYPE::from_u32` and `USER_INSTANCE.to_u32`,
respectively, instead of doing all the complicated stuff described above.

But that means that `from_u32` and `to_u32` need to be implemented for the
bitfield. That is what `impl_bitfield_primitive_conversion` does (with great
care--as opposed to what bitfield itself would do if given a chance).

*/

#![forbid(unsafe_code)]
#![recursion_limit = "256"]
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(elided_lifetimes_in_paths)]
#![allow(clippy::collapsible_if)]

#[cfg(test)]
#[macro_use]
extern crate memoffset;

mod apcb;
mod entry;
mod group;
mod naples;
mod ondisk;
#[cfg(feature = "serde")]
mod serializers;
mod struct_accessors;
mod struct_variants_enum;
mod tests;
mod token_accessors;
mod tokens_entry;
mod types;
pub use apcb::Apcb;
pub use apcb::ApcbIoOptions;
pub use entry::EntryItemBody;
pub use ondisk::*;
pub use types::ApcbContext;
pub use types::Error;
pub use types::FileSystemError;
pub use types::MemDfeSearchVersion;
pub use types::PriorityLevel;
pub use types::Result;
