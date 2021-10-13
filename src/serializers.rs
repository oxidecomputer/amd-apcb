// This file contains the serializers for the ondisk formats.
// These are meant automatically make serde use a temporary serde-aware struct
// as a proxy when serializing/deserializing a non-serde-aware struct. Note that
// if too many fields are private, it means that those are not in the proxy
// struct in the first place. This might cause problems. Also, serialization can
// fail if the nice simple user-visible type cannot represent what we are doing.

use crate::ondisk::*;
use crate::struct_accessors::DummyErrorChecks;

// Note: This is written such that it will fail if the underlying struct has
// fields added/removed/renamed--if those have a public setter.
macro_rules! make_serde{($StructName:ident, $SerdeStructName:ident, [$($field_name:ident),* $(,)?]
) => (
	paste::paste!{
		impl<'de> serde::de::Deserialize<'de> for $StructName {
			fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
			where D: serde::de::Deserializer<'de>, {
				let config = $SerdeStructName::deserialize(deserializer)?;
				Ok($StructName::default()
				$(
				.[<with_ $field_name>](config.$field_name.into())
				)*)
		        }
		}
		impl serde::Serialize for $StructName {
			fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
			where S: serde::Serializer, {
				$SerdeStructName {
					$(
						$field_name: self.$field_name().map_err(|_| serde::ser::Error::custom("value unknown"))?.into(),
					)*
				}.serialize(serializer)
			}
		}
		#[cfg(std)]
		impl schemars::JsonSchema for $StructName {
			fn schema_name() -> String {
				$SerdeStructName::schema_name()
			}
			fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
				$SerdeStructName::json_schema(gen)
			}
			/*fn is_referenceable() -> bool {
				$SerdeStructName::is_referenceable()
			} FIXME */
		}
	}
)}

make_serde!(
    ENTRY_HEADER,
    SerdeENTRY_HEADER,
    [
        group_id,
        entry_id,
        entry_size,
        instance_id,
        context_type,
        context_format,
        unit_size,
        priority_mask,
        key_size,
        key_pos,
        board_instance_mask
    ]
);

make_serde!(
    PriorityLevels,
    SerdePriorityLevels,
    [hard_force, high, medium, event_logging, low, normal,]
);
/*
make_serde!(EfhBulldozerSpiMode, SerdeEfhBulldozerSpiMode, [read_mode, fast_speed_new]);
make_serde!(EfhNaplesSpiMode, SerdeEfhNaplesSpiMode, [read_mode, fast_speed_new, micron_mode]);
make_serde!(EfhRomeSpiMode, SerdeEfhRomeSpiMode, [read_mode, fast_speed_new, micron_mode]);
make_serde!(
    Efh,
    SerdeEfh,
    [
        signature,
        bhd_directory_table_milan,
        xhci_fw_location,
        gbe_fw_location,
        imc_fw_location,
        low_power_promontory_firmware_location,
        promontory_firmware_location,
        psp_directory_table_location_naples,
        psp_directory_table_location_zen,
        spi_mode_bulldozer,
        spi_mode_zen_naples,
        spi_mode_zen_rome
    ]
);

make_serde!(DirectoryAdditionalInfo, SerdeDirectoryAdditionalInfo, [base_address, address_mode, max_size]);
make_serde!(PspSoftFuseChain, SerdePspSoftFuseChain, [
    secure_debug_unlock,
    early_secure_debug_unlock,
    unlock_token_in_nvram,
    force_security_policy_loading_even_if_insecure,
    load_diagnostic_bootloader,
    disable_psp_debug_prints,
    spi_decoding,
    postcode_decoding,
    skip_mp2_firmware_loading,
    postcode_output_control_1byte,
    force_recovery_booting
]);
make_serde!(PspDirectoryEntryAttrs, CustomSerdePspDirectoryEntryAttrs, [
    type_,
    sub_program,
    rom_id
]);
make_serde!(BhdDirectoryEntryAttrs, CustomSerdeBhdDirectoryEntryAttrs, [
    type_,
    region_type,
    reset_image,
    copy_image,
    read_only,
    compressed,
    instance,
    sub_program,
    rom_id
]);
*/
