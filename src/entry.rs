use crate::naples::ParameterTokenConfig;
use crate::ondisk::ENTRY_HEADER;
use crate::ondisk::{
    take_header_from_collection, take_header_from_collection_mut,
    BoardInstances, ContextFormat, ContextType, EntryCompatible, EntryId,
    HeaderWithTail, MutSequenceElementFromBytes, PriorityLevels,
    SequenceElementFromBytes,
};
use crate::ondisk::{Parameters, ParametersIter};
use crate::tokens_entry::TokensEntryBodyItem;
use crate::types::{Error, FileSystemError, Result};
use core::marker::PhantomData;
use core::mem::size_of;
use num_traits::FromPrimitive;
use pre::pre;
use zerocopy::{AsBytes, FromBytes};

#[cfg(feature = "serde")]
use crate::ondisk::{Parameter, TOKEN_ENTRY};
#[cfg(feature = "serde")]
use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
#[cfg(feature = "serde")]
use serde::ser::{Serialize, SerializeStruct, Serializer};

/* Note: high-level interface is:

   enum EntryMutItem {
       Raw(&[u8]),
       Tokens(Token...),
       Params(Param...), // not seen in the wild anymore
   }

*/

#[derive(Debug, Clone, Copy)]
pub enum EntryItemBody<BufferType> {
    Struct(BufferType),
    Tokens(TokensEntryBodyItem<BufferType>),
}

impl<'a> EntryItemBody<&'a mut [u8]> {
    pub(crate) fn from_slice(
        header: &ENTRY_HEADER,
        b: &'a mut [u8],
    ) -> Result<EntryItemBody<&'a mut [u8]>> {
        let context_type = ContextType::from_u8(header.context_type).ok_or(
            Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::context_type",
            ),
        )?;
        match context_type {
            ContextType::Struct => {
                if header.unit_size != 0 {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "ENTRY_HEADER::unit_size",
                    ));
                }
                Ok(Self::Struct(b))
            }
            ContextType::Tokens => {
                let used_size = b.len();
                Ok(Self::Tokens(TokensEntryBodyItem::<&mut [u8]>::new(
                    header, b, used_size,
                )?))
            }
            ContextType::Parameters => Err(Error::EntryTypeMismatch),
        }
    }
}

impl<'a> EntryItemBody<&'a [u8]> {
    pub(crate) fn from_slice(
        header: &ENTRY_HEADER,
        b: &'a [u8],
    ) -> Result<EntryItemBody<&'a [u8]>> {
        let context_type = ContextType::from_u8(header.context_type).ok_or(
            Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::context_type",
            ),
        )?;
        match context_type {
            ContextType::Struct => {
                if header.unit_size != 0 {
                    return Err(Error::FileSystem(
                        FileSystemError::InconsistentHeader,
                        "ENTRY_HEADER::unit_size",
                    ));
                }
                Ok(Self::Struct(b))
            }
            ContextType::Tokens => {
                let used_size = b.len();
                Ok(Self::Tokens(TokensEntryBodyItem::<&[u8]>::new(
                    header, b, used_size,
                )?))
            }
            ContextType::Parameters => Err(Error::EntryTypeMismatch),
        }
    }
    pub(crate) fn validate(&self) -> Result<()> {
        match self {
            EntryItemBody::Tokens(tokens) => {
                tokens.validate()?;
            }
            EntryItemBody::Struct(_) => {}
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct EntryMutItem<'a> {
    pub(crate) header: &'a mut ENTRY_HEADER,
    pub body: EntryItemBody<&'a mut [u8]>,
}

pub struct StructSequenceEntryMutItem<'a, T> {
    buf: &'a mut [u8],
    entry_id: EntryId,
    _data: PhantomData<&'a T>,
}

pub struct StructSequenceEntryMutIter<'a, T> {
    buf: &'a mut [u8],
    entry_id: EntryId,
    _data: PhantomData<T>,
}

impl<'a, T: EntryCompatible + MutSequenceElementFromBytes<'a>>
    StructSequenceEntryMutItem<'a, T>
{
    pub fn iter_mut(&'a mut self) -> Result<StructSequenceEntryMutIter<'a, T>> {
        StructSequenceEntryMutIter::<T> {
            buf: &mut *self.buf,
            entry_id: self.entry_id,
            _data: PhantomData,
        }
        .validate()?;
        Ok(StructSequenceEntryMutIter::<T> {
            buf: self.buf,
            entry_id: self.entry_id,
            _data: PhantomData,
        })
    }
}

impl<'a, T: EntryCompatible + MutSequenceElementFromBytes<'a>>
    StructSequenceEntryMutIter<'a, T>
{
    fn next1(&'_ mut self) -> Result<T> {
        if self.buf.is_empty() {
            Err(Error::EntryTypeMismatch)
        } else if T::is_entry_compatible(self.entry_id, self.buf) {
            // Note: If it was statically known: let result =
            // take_header_from_collection_mut::<T>(&mut
            // a).ok_or(Error::EntryTypeMismatch)?;
            T::checked_from_bytes(self.entry_id, &mut self.buf)
        } else {
            Err(Error::EntryTypeMismatch)
        }
    }
}

impl<'a, 'b, T: EntryCompatible + MutSequenceElementFromBytes<'b>>
    StructSequenceEntryMutIter<'a, T>
{
    pub(crate) fn validate(mut self) -> Result<()> {
        while !self.buf.is_empty() {
            if T::is_entry_compatible(self.entry_id, self.buf) {
                let (_type, size) = T::skip_step(self.entry_id, self.buf)
                    .ok_or(Error::EntryTypeMismatch)?;
                let (_, buf) = self.buf.split_at_mut(size);
                self.buf = buf;
            } else {
                return Err(Error::EntryTypeMismatch);
            }
        }
        Ok(())
    }
}

// Note: T is an enum (usually a MutElementRef)
impl<'a, T: EntryCompatible + MutSequenceElementFromBytes<'a>> Iterator
    for StructSequenceEntryMutIter<'a, T>
{
    type Item = T;
    fn next(&'_ mut self) -> Option<Self::Item> {
        // Note: Further error checking is done in validate()
        if self.buf.is_empty() {
            None
        } else {
            self.next1().ok()
        }
    }
}

pub struct StructArrayEntryMutItem<'a, T: Sized + FromBytes + AsBytes> {
    buf: &'a mut [u8],
    _item: PhantomData<&'a T>,
}

impl<'a, T: 'a + Sized + FromBytes + AsBytes> StructArrayEntryMutItem<'a, T> {
    pub fn iter_mut(&mut self) -> StructArrayEntryMutIter<'_, T> {
        StructArrayEntryMutIter { buf: self.buf, _item: PhantomData }
    }
}

pub struct StructArrayEntryMutIter<'a, T: Sized + FromBytes + AsBytes> {
    buf: &'a mut [u8],
    _item: PhantomData<&'a T>,
}

impl<'a, T: 'a + Sized + FromBytes + AsBytes> Iterator
    for StructArrayEntryMutIter<'a, T>
{
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        if self.buf.is_empty() {
            None
        } else {
            // The "?" instead of '.unwrap()" here is solely to support
            // BoardIdGettingMethod (the latter introduces useless padding at
            // the end)
            Some(take_header_from_collection_mut::<T>(&mut self.buf)?)
        }
    }
}

impl<'a> EntryMutItem<'a> {
    pub fn group_id(&self) -> u16 {
        self.header.group_id.get()
    }
    pub fn type_id(&self) -> u16 {
        self.header.entry_id.get()
    }
    pub fn id(&self) -> EntryId {
        EntryId::decode(self.header.group_id.get(), self.header.entry_id.get())
    }
    pub fn instance_id(&self) -> u16 {
        self.header.instance_id.get()
    }
    pub fn context_type(&self) -> ContextType {
        ContextType::from_u8(self.header.context_type).unwrap()
    }
    pub fn context_format(&self) -> ContextFormat {
        ContextFormat::from_u8(self.header.context_format).unwrap()
    }
    /// Note: Applicable iff context_type() == 2.  Usual value then: 8.  If
    /// inapplicable, value is 0.
    pub fn unit_size(&self) -> u8 {
        self.header.unit_size
    }
    pub fn priority_mask(&self) -> Result<PriorityLevels> {
        self.header.priority_mask()
    }
    /// Note: Applicable iff context_format() != ContextFormat::Raw. Result <=
    /// unit_size.
    pub fn key_size(&self) -> u8 {
        self.header.key_size
    }
    pub fn key_pos(&self) -> u8 {
        self.header.key_pos
    }
    pub fn board_instance_mask(&self) -> BoardInstances {
        BoardInstances::from(self.header.board_instance_mask.get())
    }

    /* Not seen in the wild anymore.
        /// If the value is a Parameter, returns its time point
        pub fn parameter_time_point(&self) -> u8 {
            assert!(self.context_type() == ContextType::Parameter);
            self.body[0]
        }

        /// If the value is a Parameter, returns its token
        pub fn parameter_token(&self) -> u16 {
            assert!(self.context_type() == ContextType::Parameter);
            let value = self.body[1] as u16 | ((self.body[2] as u16) << 8);
            value & 0x1FFF
        }

        // If the value is a Parameter, returns its size
        pub fn parameter_size(&self) -> u16 {
            assert!(self.context_type() == ContextType::Parameter);
            let value = self.body[1] as u16 | ((self.body[2] as u16) << 8);
            (value >> 13) + 1
        }
    */

    pub fn set_priority_mask(&mut self, value: PriorityLevels) {
        self.header.set_priority_mask(value);
    }

    // Note: Because entry_id, instance_id, group_id and board_instance_mask are
    // sort keys, these cannot be mutated.

    #[pre(
        "Caller already increased the group size by `size_of::<TOKEN_ENTRY>()`"
    )]
    #[pre(
        "Caller already increased the entry size by `size_of::<TOKEN_ENTRY>()`"
    )]
    pub(crate) fn insert_token(
        &mut self,
        token_id: u32,
        token_value: u32,
    ) -> Result<()> {
        match &mut self.body {
            EntryItemBody::<_>::Tokens(a) =>
            {
                #[assure(
                    "Caller already increased the group size by `size_of::<TOKEN_ENTRY>()`",
                    reason = "It's our caller's responsibility and our precondition"
                )]
                #[assure(
                    "Caller already increased the entry size by `size_of::<TOKEN_ENTRY>()`",
                    reason = "It's our caller's responsibility and our precondition"
                )]
                a.insert_token(token_id, token_value)
            }
            _ => Err(Error::EntryTypeMismatch),
        }
    }

    pub(crate) fn delete_token(&mut self, token_id: u32) -> Result<()> {
        match &mut self.body {
            EntryItemBody::<_>::Tokens(a) => a.delete_token(token_id),
            _ => Err(Error::EntryTypeMismatch),
        }
    }

    pub fn body_as_struct_mut<
        H: EntryCompatible + Sized + FromBytes + AsBytes + HeaderWithTail,
    >(
        &mut self,
    ) -> Option<(
        &'_ mut H,
        StructArrayEntryMutItem<'_, H::TailArrayItemType<'_>>,
    )> {
        let id = self.id();
        match &mut self.body {
            EntryItemBody::Struct(buf) => {
                if H::is_entry_compatible(id, buf) {
                    let mut buf = &mut buf[..];
                    let header =
                        take_header_from_collection_mut::<H>(&mut buf)?;
                    Some((
                        header,
                        StructArrayEntryMutItem { buf, _item: PhantomData },
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn body_as_struct_array_mut<
        T: EntryCompatible + Sized + FromBytes + AsBytes,
    >(
        &mut self,
    ) -> Option<StructArrayEntryMutItem<'_, T>> {
        let id = self.id();
        match &mut self.body {
            EntryItemBody::Struct(buf) => {
                if T::is_entry_compatible(id, buf) {
                    let element_count: usize = buf.len() / size_of::<T>();
                    if buf.len() == element_count * size_of::<T>() {
                        Some(StructArrayEntryMutItem {
                            buf,
                            _item: PhantomData,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// This allows the user to iterate over a sequence of different-size
    /// structs in the same Entry.
    pub fn body_as_struct_sequence_mut<T: EntryCompatible>(
        &'a mut self,
    ) -> Option<StructSequenceEntryMutItem<'a, T>> {
        let id = self.id();
        match &mut self.body {
            EntryItemBody::Struct(buf) => {
                if T::is_entry_compatible(id, buf) {
                    Some(StructSequenceEntryMutItem::<T> {
                        buf,
                        entry_id: id,
                        _data: PhantomData,
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
use std::fmt;

#[derive(Clone)]
pub struct EntryItem<'a> {
    pub(crate) header: &'a ENTRY_HEADER,
    pub body: EntryItemBody<&'a [u8]>,
}

#[cfg(feature = "serde")]
pub struct SerdeEntryItem {
    pub(crate) header: ENTRY_HEADER,
    pub(crate) body: Vec<u8>,
}

#[cfg(feature = "schemars")]
impl<'a> schemars::JsonSchema for EntryItem<'a> {
    fn schema_name() -> std::string::String {
        String::from("EntryItem")
    }
    fn json_schema(
        gen: &mut schemars::gen::SchemaGenerator,
    ) -> schemars::schema::Schema {
        use crate::memory;
        use crate::psp;
        use crate::tokens_entry::TokensEntryItem;
        let mut schema = schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::Object.into()),
            ..Default::default()
        };
        let obj = schema.object();
        obj.required.insert("header".to_owned());
        obj.properties
            .insert("header".to_owned(), <ENTRY_HEADER>::json_schema(gen));
        obj.properties.insert(
            "tokens".to_owned(),
            <Vec<TokensEntryItem<&'_ TOKEN_ENTRY>>>::json_schema(gen),
        );
        obj.properties.insert(
            "LrdimmDdr4OdtPatElement".to_owned(),
            <Vec<memory::LrdimmDdr4OdtPatElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "Ddr4OdtPatElement".to_owned(),
            <Vec<memory::Ddr4OdtPatElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "DdrPostPackageRepairElement".to_owned(),
            <Vec<memory::DdrPostPackageRepairElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "DimmInfoSmbusElement".to_owned(),
            <Vec<memory::DimmInfoSmbusElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "RdimmDdr4CadBusElement".to_owned(),
            <Vec<memory::RdimmDdr4CadBusElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "UdimmDdr4CadBusElement".to_owned(),
            <Vec<memory::UdimmDdr4CadBusElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "LrdimmDdr4CadBusElement".to_owned(),
            <Vec<memory::LrdimmDdr4CadBusElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "Ddr4DataBusElement".to_owned(),
            <Vec<memory::Ddr4DataBusElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "LrdimmDdr4DataBusElement".to_owned(),
            <Vec<memory::LrdimmDdr4DataBusElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "MaxFreqElement".to_owned(),
            <Vec<memory::MaxFreqElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "LrMaxFreqElement".to_owned(),
            <Vec<memory::LrMaxFreqElement>>::json_schema(gen),
        );
        obj.properties.insert(
            "ConsoleOutControl".to_owned(),
            <memory::ConsoleOutControl>::json_schema(gen),
        );
        obj.properties.insert(
            "NaplesConsoleOutControl".to_owned(),
            <memory::NaplesConsoleOutControl>::json_schema(gen),
        );
        obj.properties.insert(
            "ExtVoltageControl".to_owned(),
            <memory::ExtVoltageControl>::json_schema(gen),
        );
        obj.properties.insert(
            "ErrorOutControl116".to_owned(),
            <memory::ErrorOutControl116>::json_schema(gen),
        );
        obj.properties.insert(
            "ErrorOutControl112".to_owned(),
            <memory::ErrorOutControl112>::json_schema(gen),
        );
        obj.properties.insert(
            "SlinkConfig".to_owned(),
            <crate::df::SlinkConfig>::json_schema(gen),
        );

        obj.properties
            .insert("BoardIdGettingMethodGpio".to_owned(),
                <(psp::BoardIdGettingMethodGpio,
                    Vec<<psp::BoardIdGettingMethodGpio as
                        HeaderWithTail>::TailArrayItemType<'_>>)>::json_schema(gen));
        obj.properties
            .insert("BoardIdGettingMethodEeprom".to_owned(),
                <(psp::BoardIdGettingMethodEeprom,
                    Vec<<psp::BoardIdGettingMethodEeprom as
                        HeaderWithTail>::TailArrayItemType<'_>>)>::json_schema(gen));
        obj.properties
            .insert("BoardIdGettingMethodSmbus".to_owned(),
                <(psp::BoardIdGettingMethodSmbus,
                    Vec<<psp::BoardIdGettingMethodSmbus as
                        HeaderWithTail>::TailArrayItemType<'_>>)>::json_schema(gen));
        obj.properties
            .insert("BoardIdGettingMethodCustom".to_owned(),
                <(psp::BoardIdGettingMethodCustom,
                    Vec<<psp::BoardIdGettingMethodCustom as
                        HeaderWithTail>::TailArrayItemType<'_>>)>::json_schema(gen));

        obj.properties.insert(
            "platform_specific_overrides".to_owned(),
            <Vec<memory::platform_specific_override::ElementRef<'_>>>::json_schema(
                gen,
            ),
        );
        obj.properties.insert(
            "platform_tuning".to_owned(),
            <Vec<memory::platform_tuning::ElementRef<'_>>>::json_schema(gen),
        );

        obj.properties
            .insert("parameters".to_owned(), <Parameters>::json_schema(gen));
        schema.into()
    }
}
#[cfg(feature = "schemars")]
impl schemars::JsonSchema for SerdeEntryItem {
    fn schema_name() -> std::string::String {
        EntryItem::schema_name()
    }
    fn json_schema(
        gen: &mut schemars::gen::SchemaGenerator,
    ) -> schemars::schema::Schema {
        EntryItem::json_schema(gen)
    }
    fn is_referenceable() -> bool {
        EntryItem::is_referenceable()
    }
}

#[cfg(feature = "serde")]
impl<'a> Serialize for EntryItem<'a> {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use crate::df::SlinkConfig;
        use crate::memory;
        use crate::psp;
        let mut state = serializer.serialize_struct("EntryItem", 2)?;
        state.serialize_field("header", self.header)?;

        // TODO: Automate this type determination instead of maintaining this
        // manually.
        match &self.body {
            EntryItemBody::<_>::Tokens(tokens) => {
                let v = tokens
                    .iter()
                    .map_err(|e| serde::ser::Error::custom(format!("{e:?}")))?
                    .collect::<Vec<_>>();
                state.serialize_field("tokens", &v)?;
            }
            EntryItemBody::<_>::Struct(buf) => {
                if let Some(s) = self.body_as_struct_array::<memory::LrdimmDdr4OdtPatElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("LrdimmDdr4OdtPatElement", &v)?;
                } else if let Some(s) = self.body_as_struct_array::<memory::Ddr4OdtPatElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("Ddr4OdtPatElement", &v)?;
                } else if let Some(s) = self.body_as_struct_array::<memory::DdrPostPackageRepairElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("DdrPostPackageRepairElement", &v)?;
                } else if let Some(s) = self.body_as_struct_array::<memory::DimmInfoSmbusElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("DimmInfoSmbusElement", &v)?;
                } else if let Some(s) = self.body_as_struct_array::<memory::RdimmDdr4CadBusElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("RdimmDdr4CadBusElement", &v)?;
                } else if let Some(s) = self.body_as_struct_array::<memory::UdimmDdr4CadBusElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("UdimmDdr4CadBusElement", &v)?;
                } else if let Some(s) = self.body_as_struct_array::<memory::LrdimmDdr4CadBusElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("LrdimmDdr4CadBusElement", &v)?;
                } else if let Some(s) = self.body_as_struct_array::<memory::Ddr4DataBusElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("Ddr4DataBusElement", &v)?;
                } else if let Some(s) = self.body_as_struct_array::<memory::LrdimmDdr4DataBusElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("LrdimmDdr4DataBusElement", &v)?;
                } else if let Some(s) = self.body_as_struct_array::<memory::MaxFreqElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("MaxFreqElement", &v)?;
                } else if let Some(s) = self.body_as_struct_array::<memory::LrMaxFreqElement>() {
                    let v = s.iter().collect::<Vec<_>>();
                    state.serialize_field("LrMaxFreqElement", &v)?;
                } else if let Some((s, _)) = self.body_as_struct::<memory::ConsoleOutControl>() {
                    state.serialize_field("ConsoleOutControl", &s)?;
                } else if let Some((s, _)) = self.body_as_struct::<memory::NaplesConsoleOutControl>() {
                    state.serialize_field("NaplesConsoleOutControl", &s)?;
                } else if let Some((s, _)) = self.body_as_struct::<memory::ExtVoltageControl>() {
                    state.serialize_field("ExtVoltageControl", &s)?;
                } else if let Some((s, _)) = self.body_as_struct::<memory::ErrorOutControl116>() {
                    state.serialize_field("ErrorOutControl116", &s)?;
                } else if let Some((s, _)) = self.body_as_struct::<memory::ErrorOutControl112>() {
                    state.serialize_field("ErrorOutControl112", &s)?;
                } else if let Some((s, _)) = self.body_as_struct::<SlinkConfig>() {
                    state.serialize_field("SlinkConfig", &s)?;
                } else if let Some((header, s)) = self.body_as_struct::<psp::BoardIdGettingMethodGpio>() {
                    let v = s.iter().collect::<Vec<_>>();
                    let t = (header, v);
                    state.serialize_field("BoardIdGettingMethodGpio", &t)?;
                } else if let Some((header, s)) = self.body_as_struct::<psp::BoardIdGettingMethodEeprom>() {
                    let v = s.iter().collect::<Vec<_>>();
                    let t = (header, v);
                    state.serialize_field("BoardIdGettingMethodEeprom", &t)?;
                } else if let Some((header, s)) = self.body_as_struct::<psp::BoardIdGettingMethodSmbus>() {
                    let v = s.iter().collect::<Vec<_>>();
                    let t = (header, v);
                    state.serialize_field("BoardIdGettingMethodSmbus", &t)?;
                } else if let Some((header, s)) = self.body_as_struct::<psp::BoardIdGettingMethodCustom>() {
                    let v = s.iter().collect::<Vec<_>>();
                    let t = (header, v);
                    state.serialize_field("BoardIdGettingMethodCustom", &t)?;
                } else if let Some(s) =
self.body_as_struct_sequence::<memory::platform_specific_override::ElementRef<'_>>() {
                    let i = s.iter().unwrap();
                    let v = i.collect::<Vec<_>>();
                    state.serialize_field("platform_specific_overrides", &v)?;
                } else if let Some(s) =
self.body_as_struct_sequence::<memory::platform_tuning::ElementRef<'_>>() {
                    let i = s.iter().unwrap();
                    let v = i.collect::<Vec<_>>();
                    state.serialize_field("platform_tuning", &v)?;
                } else if let Some((_, s)) = self.body_as_struct::<Parameters>() {
                    let parameters = ParametersIter::new(s.into_slice())
                        .map_err(|_| serde::ser::Error::custom("could not serialize Parameters"))?;
                    let v = parameters.collect::<Vec<_>>();
                    state.serialize_field("parameters", &v)?;
                } else {
                    state.serialize_field("struct_body", &buf)?;
                }
            }
        }
        state.end()
    }
}

#[cfg(feature = "serde")]
/// if BODY is empty, read a value (which is a Vec of TokensEntryItem) from MAP
/// and stash it into a new BODY. If BODY is not empty, that's an error. This is
/// used purely as a helper function during deserialize.
fn token_vec_to_body<'a, M>(
    body: &mut Option<Vec<u8>>,
    map: &mut M,
) -> core::result::Result<(), M::Error>
where
    M: MapAccess<'a>,
{
    use crate::ondisk::TokenEntryId;
    use crate::tokens_entry::SerdeTokensEntryItem;
    use core::convert::TryFrom;
    if body.is_some() {
        return Err(de::Error::duplicate_field("body"));
    }
    let val: Vec<SerdeTokensEntryItem> = map.next_value()?;
    let mut buf: Vec<u8> = Vec::new();

    if !val.is_empty() {
        // Ensure that all tokens in this entry have the same id.
        if let TokenEntryId::Unknown(_eid) = val[0].entry_id() {
            return Err(de::Error::invalid_value(
                de::Unexpected::Enum,
                &"expected one of [Bool, Byte, Word, Dword]",
            ));
        }
        let entry_id = val[0].entry_id();
        for v in val {
            if entry_id != v.entry_id() {
                return Err(de::Error::invalid_value(
                    de::Unexpected::Enum,
                    &entry_id,
                ));
            }
            if let Ok(te) = TOKEN_ENTRY::try_from(v) {
                buf.extend_from_slice(te.as_bytes())
            } else {
                return Err(de::Error::invalid_value(
                    de::Unexpected::Enum,
                    &"a valid Token Entry",
                ));
            }
        }
    }
    *body = Some(buf);
    Ok(())
}

#[cfg(feature = "serde")]
/// if BODY is empty, read a value (which is a Vec) from MAP and stash it into a
/// new BODY. If BODY is not empty, that's an error. This is used purely as a
/// helper function during deserialize.
fn struct_vec_to_body<'a, T, M>(
    body: &mut Option<Vec<u8>>,
    map: &mut M,
) -> core::result::Result<(), M::Error>
where
    T: zerocopy::AsBytes + Deserialize<'a>,
    M: MapAccess<'a>,
{
    if body.is_some() {
        return Err(de::Error::duplicate_field("body"));
    }
    let val: Vec<T> = map.next_value()?;
    let mut buf: Vec<u8> = Vec::new();
    for v in val {
        buf.extend_from_slice(v.as_bytes());
    }
    *body = Some(buf);
    Ok(())
}

#[cfg(feature = "serde")]
/// if BODY is empty, read a value (which is a struct) from MAP and stash it
/// into a new BODY. If BODY is not empty, that's an error. This is used purely
/// as a helper function during deserialize.
fn struct_to_body<'a, T, M>(
    body: &mut Option<Vec<u8>>,
    map: &mut M,
) -> core::result::Result<(), M::Error>
where
    T: zerocopy::AsBytes + Deserialize<'a> + HeaderWithTail,
    M: MapAccess<'a>,
{
    if body.is_some() {
        return Err(de::Error::duplicate_field("body"));
    }
    let mut buf: Vec<u8> = Vec::new();
    if size_of::<T::TailArrayItemType<'_>>() != 0 {
        let (h, val): (T, Vec<T::TailArrayItemType<'a>>) = map.next_value()?;
        buf.extend_from_slice(h.as_bytes());
        for v in val {
            buf.extend_from_slice(v.as_bytes());
        }
    } else {
        let h: T = map.next_value()?;
        buf.extend_from_slice(h.as_bytes());
    }
    *body = Some(buf);
    Ok(())
}

#[cfg(feature = "serde")]
/// if BODY is empty, read a value (which is a struct) from MAP and stash it
/// into a new BODY. If BODY is not empty, that's an error. This is used purely
/// as a helper function during deserialize.
fn parameters_struct_to_body<'a, M>(
    body: &mut Option<Vec<u8>>,
    map: &mut M,
) -> core::result::Result<(), M::Error>
where
    M: MapAccess<'a>,
{
    if body.is_some() {
        return Err(de::Error::duplicate_field("body"));
    }
    let val: Vec<Parameter> = map.next_value()?;
    let buf = Parameters::new_tail_from_vec(val).unwrap();
    *body = Some(buf);
    Ok(())
}

#[cfg(feature = "serde")]
/// if BODY is empty, read a value (which is a Vec) from MAP and stash it into a
/// new BODY. If BODY is not empty, that's an error. This is used purely as a
/// helper function during deserialize. This handles the sequences, and thus the
/// elements in this vec can each be different.
fn struct_sequence_to_body<'a, T, M>(
    body: &mut Option<Vec<u8>>,
    map: &mut M,
) -> core::result::Result<(), M::Error>
where
    T: crate::ondisk::ElementAsBytes + Deserialize<'a>,
    M: MapAccess<'a>,
{
    if body.is_some() {
        return Err(de::Error::duplicate_field("body"));
    }
    let val: Vec<T> = map.next_value()?;
    let mut buf: Vec<u8> = Vec::new();
    for v in val {
        buf.extend_from_slice(v.element_as_bytes());
    }
    *body = Some(buf);
    Ok(())
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for SerdeEntryItem {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Field {
            Header,
            Tokens,
            // Body as struct array
            LrdimmDdr4OdtPatElement,
            Ddr4OdtPatElement,
            DdrPostPackageRepairElement,
            DimmInfoSmbusElement,
            RdimmDdr4CadBusElement,
            UdimmDdr4CadBusElement,
            LrdimmDdr4CadBusElement,
            Ddr4DataBusElement,
            LrdimmDdr4DataBusElement,
            MaxFreqElement,
            LrMaxFreqElement,
            // Body as struct
            ConsoleOutControl,
            ExtVoltageControl,
            ErrorOutControl116,
            ErrorOutControl112,
            SlinkConfig,
            BoardIdGettingMethodGpio,
            BoardIdGettingMethodEeprom,
            BoardIdGettingMethodSmbus,
            BoardIdGettingMethodCustom,
            // struct sequence
            PlatformSpecificOverrides,
            PlatformTuning,
            Parameters,
        }
        const FIELDS: &[&str] = &[
            "header",
            "tokens",
            "LrdimmDdr4OdtPatElement",
            "Ddr4OdtPatElement",
            "DdrPostPackageRepairElement",
            "DimmInfoSmbusElement",
            "RdimmDdr4CadBusElement",
            "UdimmDdr4CadBusElement",
            "LrdimmDdr4CadBusElement",
            "Ddr4DataBusElement",
            "LrdimmDdr4DataBusElement",
            "MaxFreqElement",
            "LrMaxFreqElement",
            // Body as struct
            "ConsoleOutControl",
            "ExtVoltageControl",
            "ErrorOutControl116",
            "ErrorOutControl112",
            "SlinkConfig",
            "BoardIdGettingMethodGpio",
            "BoardIdGettingMethodEeprom",
            "BoardIdGettingMethodSmbus",
            "BoardIdGettingMethodCustom",
            // struct sequence
            "platform_specific_overrides",
            "platform_tuning",
            "parameters",
        ];

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(
                deserializer: D,
            ) -> core::result::Result<Field, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;

                    fn expecting(
                        &self,
                        formatter: &mut fmt::Formatter<'_>,
                    ) -> fmt::Result {
                        formatter.write_str("`header` or Entry Item Body")
                    }

                    fn visit_str<E>(
                        self,
                        value: &str,
                    ) -> core::result::Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "header" => Ok(Field::Header),
                            "tokens" => Ok(Field::Tokens),
                            "LrdimmDdr4OdtPatElement" => {
                                Ok(Field::LrdimmDdr4OdtPatElement)
                            }
                            "Ddr4OdtPatElement" => Ok(Field::Ddr4OdtPatElement),
                            "DdrPostPackageRepairElement" => {
                                Ok(Field::DdrPostPackageRepairElement)
                            }
                            "DimmInfoSmbusElement" => {
                                Ok(Field::DimmInfoSmbusElement)
                            }
                            "RdimmDdr4CadBusElement" => {
                                Ok(Field::RdimmDdr4CadBusElement)
                            }
                            "UdimmDdr4CadBusElement" => {
                                Ok(Field::UdimmDdr4CadBusElement)
                            }
                            "LrdimmDdr4CadBusElement" => {
                                Ok(Field::LrdimmDdr4CadBusElement)
                            }
                            "Ddr4DataBusElement" => {
                                Ok(Field::Ddr4DataBusElement)
                            }
                            "LrdimmDdr4DataBusElement" => {
                                Ok(Field::LrdimmDdr4DataBusElement)
                            }
                            "MaxFreqElement" => Ok(Field::MaxFreqElement),
                            "LrMaxFreqElement" => Ok(Field::LrMaxFreqElement),
                            "ConsoleOutControl" => Ok(Field::ConsoleOutControl),
                            "ExtVoltageControl" => Ok(Field::ExtVoltageControl),
                            "ErrorOutControl116" => {
                                Ok(Field::ErrorOutControl116)
                            }
                            "ErrorOutControl112" => {
                                Ok(Field::ErrorOutControl112)
                            }
                            "SlinkConfig" => Ok(Field::SlinkConfig),
                            "BoardIdGettingMethodGpio" => {
                                Ok(Field::BoardIdGettingMethodGpio)
                            }
                            "BoardIdGettingMethodEeprom" => {
                                Ok(Field::BoardIdGettingMethodEeprom)
                            }
                            "BoardIdGettingMethodSmbus" => {
                                Ok(Field::BoardIdGettingMethodSmbus)
                            }
                            "BoardIdGettingMethodCustom" => {
                                Ok(Field::BoardIdGettingMethodCustom)
                            }
                            "platform_specific_overrides" => {
                                Ok(Field::PlatformSpecificOverrides)
                            }
                            "platform_tuning" => Ok(Field::PlatformTuning),
                            "parameters" => Ok(Field::Parameters),
                            _ => Err(de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct SerdeEntryItemVisitor;

        impl<'de> Visitor<'de> for SerdeEntryItemVisitor {
            type Value = SerdeEntryItem;

            fn expecting(
                &self,
                formatter: &mut fmt::Formatter<'_>,
            ) -> fmt::Result {
                formatter.write_str("struct EntryItem")
            }

            fn visit_map<V>(
                self,
                mut map: V,
            ) -> core::result::Result<SerdeEntryItem, V::Error>
            where
                V: MapAccess<'de>,
            {
                use crate::df;
                use crate::memory;
                use crate::psp;
                let mut header: Option<ENTRY_HEADER> = None;
                let mut body: Option<Vec<u8>> = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Header => {
                            if header.is_some() {
                                return Err(de::Error::duplicate_field(
                                    "header",
                                ));
                            }
                            header = Some(map.next_value()?);
                        }
                        Field::Tokens => {
                            token_vec_to_body::<V>(&mut body, &mut map)?;
                        }
                        Field::LrdimmDdr4OdtPatElement => {
                            struct_vec_to_body::<
                                memory::LrdimmDdr4OdtPatElement,
                                V,
                            >(&mut body, &mut map)?;
                        }
                        Field::Ddr4OdtPatElement => {
                            struct_vec_to_body::<memory::Ddr4OdtPatElement, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::DdrPostPackageRepairElement => {
                            struct_vec_to_body::<
                                memory::DdrPostPackageRepairElement,
                                V,
                            >(&mut body, &mut map)?;
                        }
                        Field::DimmInfoSmbusElement => {
                            struct_vec_to_body::<
                                memory::DimmInfoSmbusElement,
                                V,
                            >(&mut body, &mut map)?;
                        }
                        Field::RdimmDdr4CadBusElement => {
                            struct_vec_to_body::<
                                memory::RdimmDdr4CadBusElement,
                                V,
                            >(&mut body, &mut map)?;
                        }
                        Field::UdimmDdr4CadBusElement => {
                            struct_vec_to_body::<
                                memory::UdimmDdr4CadBusElement,
                                V,
                            >(&mut body, &mut map)?;
                        }
                        Field::LrdimmDdr4CadBusElement => {
                            struct_vec_to_body::<
                                memory::LrdimmDdr4CadBusElement,
                                V,
                            >(&mut body, &mut map)?;
                        }
                        Field::Ddr4DataBusElement => {
                            struct_vec_to_body::<memory::Ddr4DataBusElement, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::LrdimmDdr4DataBusElement => {
                            struct_vec_to_body::<
                                memory::LrdimmDdr4DataBusElement,
                                V,
                            >(&mut body, &mut map)?;
                        }
                        Field::MaxFreqElement => {
                            struct_vec_to_body::<memory::MaxFreqElement, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::LrMaxFreqElement => {
                            struct_vec_to_body::<memory::LrMaxFreqElement, V>(
                                &mut body, &mut map,
                            )?;
                        }

                        Field::ConsoleOutControl => {
                            struct_to_body::<memory::ConsoleOutControl, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::ExtVoltageControl => {
                            struct_to_body::<memory::ExtVoltageControl, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::ErrorOutControl116 => {
                            struct_to_body::<memory::ErrorOutControl116, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::ErrorOutControl112 => {
                            struct_to_body::<memory::ErrorOutControl112, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::SlinkConfig => {
                            struct_to_body::<df::SlinkConfig, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::BoardIdGettingMethodGpio => {
                            struct_to_body::<psp::BoardIdGettingMethodGpio, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::BoardIdGettingMethodEeprom => {
                            struct_to_body::<psp::BoardIdGettingMethodEeprom, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::BoardIdGettingMethodSmbus => {
                            struct_to_body::<psp::BoardIdGettingMethodSmbus, V>(
                                &mut body, &mut map,
                            )?;
                        }
                        Field::BoardIdGettingMethodCustom => {
                            struct_to_body::<psp::BoardIdGettingMethodCustom, V>(
                                &mut body, &mut map,
                            )?;
                        }

                        Field::PlatformSpecificOverrides => {
                            struct_sequence_to_body::<
                                memory::platform_specific_override::Element,
                                V,
                            >(&mut body, &mut map)?;
                        }
                        Field::PlatformTuning => {
                            struct_sequence_to_body::<
                                memory::platform_tuning::Element,
                                V,
                            >(&mut body, &mut map)?;
                        }
                        Field::Parameters => {
                            /* Parameters itself is empty. The tail is not. */
                            parameters_struct_to_body::<V>(
                                &mut body, &mut map,
                            )?;
                        }
                    }
                }
                let header =
                    header.ok_or_else(|| de::Error::missing_field("header"))?;
                let body =
                    body.ok_or_else(|| de::Error::missing_field("body"))?;
                Ok(SerdeEntryItem { header, body })
            }
        }
        let mut result = deserializer.deserialize_struct(
            "EntryItem",
            FIELDS,
            SerdeEntryItemVisitor,
        )?;
        let header = &result.header;
        if header.context_format == ContextFormat::SortAscending as u8
            && header.context_type == (ContextType::Tokens as u8)
        {
            let body = result.body.as_mut_slice();
            let mut tokens = zerocopy::LayoutVerified::<
                _,
                [crate::ondisk::TOKEN_ENTRY],
            >::new_slice_unaligned(body)
            .ok_or(de::Error::custom("tokens could not be sorted"))?;
            tokens.sort_by(|a, b| a.key.get().cmp(&b.key.get()));
        }
        Ok(result)
    }
}

pub struct StructSequenceEntryItem<'a, T> {
    buf: &'a [u8],
    entry_id: EntryId,
    _data: PhantomData<&'a T>,
}

impl<'a, T: EntryCompatible + SequenceElementFromBytes<'a>>
    StructSequenceEntryItem<'a, T>
{
    pub fn iter(&'a self) -> Result<StructSequenceEntryIter<'a, T>> {
        StructSequenceEntryIter::<T> {
            buf: self.buf,
            entry_id: self.entry_id,
            _data: PhantomData,
        }
        .validate()?;
        Ok(StructSequenceEntryIter::<T> {
            buf: self.buf,
            entry_id: self.entry_id,
            _data: PhantomData,
        })
    }
}

pub struct StructSequenceEntryIter<
    'a,
    T: EntryCompatible + SequenceElementFromBytes<'a>,
> {
    buf: &'a [u8],
    entry_id: EntryId,
    _data: PhantomData<T>,
}

// Note: T is an enum (usually a ElemntRef)
impl<'a, T: EntryCompatible + SequenceElementFromBytes<'a>>
    StructSequenceEntryIter<'a, T>
{
    fn next1(&'_ mut self) -> Result<T> {
        if self.buf.is_empty() {
            Err(Error::EntryTypeMismatch)
        } else if T::is_entry_compatible(self.entry_id, self.buf) {
            // Note: If it was statically known: let result =
            // take_header_from_collection::<T>(&mut
            // a).ok_or(Error::EntryTypeMismatch)?;
            T::checked_from_bytes(self.entry_id, &mut self.buf)
        } else {
            Err(Error::EntryTypeMismatch)
        }
    }
    pub(crate) fn validate(mut self) -> Result<()> {
        while !self.buf.is_empty() {
            self.next1()?;
        }
        Ok(())
    }
}

// Note: T is an enum (usually a ElemntRef)
impl<'a, T: EntryCompatible + SequenceElementFromBytes<'a>> Iterator
    for StructSequenceEntryIter<'a, T>
{
    type Item = T;
    fn next(&'_ mut self) -> Option<Self::Item> {
        // Note: Proper error check is done on creation of the iter in
        // StructSequenceEntryItem.
        self.next1().ok()
    }
}

pub struct StructArrayEntryItem<'a, T: Sized + FromBytes> {
    buf: &'a [u8],
    _item: PhantomData<&'a T>,
}

impl<'a, T: 'a + Sized + FromBytes> StructArrayEntryItem<'a, T> {
    pub fn iter(&self) -> StructArrayEntryIter<'_, T> {
        StructArrayEntryIter { buf: self.buf, _item: PhantomData }
    }

    /// This is mostly useful for Naples Parameters.  They are modeled as a
    /// struct array with an u8 tail, and we want to decode the entire tail
    /// at once in further processing.
    pub(crate) fn into_slice(self) -> &'a [u8] {
        self.buf
    }
}

/// Naples
impl Parameters {
    pub fn iter(
        tail: StructArrayEntryItem<'_, u8>,
    ) -> Result<ParametersIter<'_>> {
        ParametersIter::new(tail.into_slice())
    }
    pub fn get(
        tail: StructArrayEntryItem<'_, u8>,
        key: ParameterTokenConfig,
    ) -> Result<u64> {
        for parameter in Self::iter(tail)? {
            if let Ok(t) = parameter.token() {
                if t == key {
                    return parameter.value();
                }
            }
        }
        Err(Error::ParameterNotFound)
    }
}

pub struct StructArrayEntryIter<'a, T: Sized + FromBytes> {
    buf: &'a [u8],
    _item: PhantomData<&'a T>,
}

impl<'a, T: 'a + Sized + FromBytes> Iterator for StructArrayEntryIter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        if self.buf.is_empty() {
            None
        } else {
            // The "?" instead of '.unwrap()" here is solely to support
            // BoardIdGettingMethod (the latter introduces useless padding at
            // the end)
            Some(take_header_from_collection::<T>(&mut self.buf)?)
        }
    }
}

impl<'a> EntryItem<'a> {
    // pub fn group_id(&self) -> u16  ; suppressed--replaced by an assert on
    // read.
    pub fn id(&self) -> EntryId {
        EntryId::decode(self.header.group_id.get(), self.header.entry_id.get())
    }
    pub fn instance_id(&self) -> u16 {
        self.header.instance_id.get()
    }
    pub fn context_type(&self) -> ContextType {
        ContextType::from_u8(self.header.context_type).unwrap()
    }
    pub fn context_format(&self) -> ContextFormat {
        ContextFormat::from_u8(self.header.context_format).unwrap()
    }
    /// Note: Applicable iff context_type() == 2.  Usual value then: 8.  If
    /// inapplicable, value is 0.
    pub fn unit_size(&self) -> u8 {
        self.header.unit_size
    }
    pub fn priority_mask(&self) -> u8 {
        self.header.priority_mask
    }
    /// Note: Applicable iff context_format() != ContextFormat::Raw. Result <=
    /// unit_size.
    pub fn key_size(&self) -> u8 {
        self.header.key_size
    }
    pub fn key_pos(&self) -> u8 {
        self.header.key_pos
    }
    pub fn board_instance_mask(&self) -> BoardInstances {
        BoardInstances::from(self.header.board_instance_mask.get())
    }

    pub(crate) fn validate(&self) -> Result<()> {
        ContextType::from_u8(self.header.context_type).ok_or(
            Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::context_type",
            ),
        )?;
        ContextFormat::from_u8(self.header.context_format).ok_or(
            Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "ENTRY_HEADER::context_format",
            ),
        )?;
        self.body.validate()?;
        Ok(())
    }

    pub fn body_as_buf(&'a self) -> Option<&[u8]> {
        match &self.body {
            EntryItemBody::Struct(buf) => Some(buf),
            _ => None,
        }
    }

    pub fn body_as_struct<
        H: EntryCompatible + Sized + FromBytes + HeaderWithTail,
    >(
        &'a self,
    ) -> Option<(&'a H, StructArrayEntryItem<'a, H::TailArrayItemType<'_>>)>
    {
        let id = self.id();
        match &self.body {
            EntryItemBody::Struct(buf) => {
                if H::is_entry_compatible(id, buf) {
                    let mut buf = &buf[..];
                    let header = take_header_from_collection::<H>(&mut buf)?;
                    Some((
                        header,
                        StructArrayEntryItem { buf, _item: PhantomData },
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn body_as_struct_array<T: EntryCompatible + Sized + FromBytes>(
        &'a self,
    ) -> Option<StructArrayEntryItem<'a, T>> {
        match &self.body {
            EntryItemBody::Struct(buf) => {
                if T::is_entry_compatible(self.id(), buf) {
                    let element_count: usize = buf.len() / size_of::<T>();
                    if buf.len() == element_count * size_of::<T>() {
                        Some(StructArrayEntryItem { buf, _item: PhantomData })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// This allows the user to iterate over a sequence of different-size
    /// structs in the same Entry.
    pub fn body_as_struct_sequence<T: EntryCompatible>(
        &'a self,
    ) -> Option<StructSequenceEntryItem<'a, T>> {
        let id = self.id();
        match &self.body {
            EntryItemBody::Struct(buf) => {
                if T::is_entry_compatible(id, buf) {
                    Some(StructSequenceEntryItem::<T> {
                        buf,
                        entry_id: id,
                        _data: PhantomData,
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl core::fmt::Debug for EntryItem<'_> {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let id = self.id();
        let instance_id = self.instance_id();
        let context_type = self.context_type();
        let context_format = self.context_format();
        let priority_mask = self.priority_mask();
        let board_instance_mask = self.board_instance_mask();
        let entry_size = self.header.entry_size;
        let header_size = size_of::<ENTRY_HEADER>();
        // Note: Elides BODY--so, technically, it's not a 1:1 representation
        fmt.debug_struct("EntryItem")
            .field("id", &id)
            .field("entry_size", &entry_size)
            .field("header_size", &header_size)
            .field("instance_id", &instance_id)
            .field("context_type", &context_type)
            .field("context_format", &context_format)
            .field("unit_size", &self.header.unit_size)
            .field("priority_mask", &priority_mask)
            .field("key_size", &self.header.key_size)
            .field("key_pos", &self.header.key_pos)
            .field("board_instance_mask", &board_instance_mask)
            .field("body", &self.body)
            .finish()
    }
}
