// This file contains the serializers for the ondisk formats.
// These are meant automatically make serde use a temporary serde-aware struct
// as a proxy when serializing/deserializing a non-serde-aware struct. Note that
// if too many fields are private, it means that those are not in the proxy
// struct in the first place. This might cause problems. Also, serialization can
// fail if the nice simple user-visible type cannot represent what we are doing.

use crate::memory::*;
use crate::ondisk::*;
use crate::psp::*;
use crate::df::*;
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

make_serde!(
    Ddr4DataBusElement,
    SerdeDdr4DataBusElement,
    [
        dimm_slots_per_channel,
        ddr_rates,
        vdd_io,
        dimm0_ranks,
        dimm1_ranks,
        rtt_nom,
        rtt_wr,
        rtt_park,
        dq_drive_strength,
        dqs_drive_strength,
        odt_drive_strength,
        pmu_phy_vref,
        vref_dq,
    ]
);
make_serde!(
    Ddr4DimmRanks,
    SerdeDdr4DimmRanks,
    [unpopulated, single_rank, dual_rank, quad_rank,]
);
make_serde!(
    LrdimmDdr4DimmRanks,
    SerdeLrdimmDdr4DimmRanks,
    [unpopulated, lr]
);
make_serde!(
    DdrRates,
    SerdeDdrRates,
    [
        ddr400, ddr533, ddr667, ddr800, ddr1066, ddr1333, ddr1600, ddr1866,
        ddr2133, ddr2400, ddr2667, ddr2933, ddr3200,
    ]
);
make_serde!(RdimmDdr4Voltages, SerdeRdimmDdr4Voltages, [v_1_2,]);
make_serde!(
    RdimmDdr4CadBusElement,
    SerdeRdimmDdr4CadBusElement,
    [
        dimm_slots_per_channel,
        ddr_rates,
        vdd_io,
        dimm0_ranks,
        dimm1_ranks,
        gear_down_mode,
        slow_mode,
        address_command_control,
        cke_drive_strength,
        cs_odt_drive_strength,
        address_command_drive_strength,
        clk_drive_strength,
    ]
);
make_serde!(
    UdimmDdr4Voltages,
    SerdeUdimmDdr4Voltages,
    [v_1_5, v_1_35, v_1_25]
);
make_serde!(
    UdimmDdr4CadBusElement,
    SerdeUdimmDdr4CadBusElement,
    [
        dimm_slots_per_channel,
        ddr_rates,
        vdd_io,
        dimm0_ranks,
        dimm1_ranks,
        gear_down_mode,
        slow_mode,
        address_command_control,
        cke_drive_strength,
        cs_odt_drive_strength,
        address_command_drive_strength,
        clk_drive_strength,
    ]
);
make_serde!(LrdimmDdr4Voltages, SerdeLrdimmDdr4Voltages, [v_1_2]);
make_serde!(
    LrdimmDdr4CadBusElement,
    SerdeLrdimmDdr4CadBusElement,
    [
        dimm_slots_per_channel,
        ddr_rates,
        vdd_io,
        dimm0_ranks,
        dimm1_ranks,
        gear_down_mode,
        slow_mode,
        address_command_control,
        cke_drive_strength,
        cs_odt_drive_strength,
        address_command_drive_strength,
        clk_drive_strength,
    ]
);

make_serde!(
    V2_HEADER,
    SerdeV2_HEADER,
    [
        signature,
        header_size,
        version,
        apcb_size,
        unique_apcb_instance,
        checksum_byte,
    ]
);
make_serde!(
    V3_HEADER_EXT,
    SerdeV3_HEADER_EXT,
    [
        signature,
        struct_version,
        data_version,
        ext_header_size,
        data_offset,
        header_checksum,
        integrity_sign,
        signature_ending,
    ]
);
make_serde!(
    GROUP_HEADER,
    SerdeGROUP_HEADER,
    [signature, group_id, header_size, version, group_size,]
);
make_serde!(
    BoardIdGettingMethodEeprom,
    SerdeBoardIdGettingMethodEeprom,
    [
        access_method,
        i2c_controller_index,
        device_address,
        board_id_offset,
        board_rev_offset,
    ]
);
make_serde!(
    IdRevApcbMapping,
    SerdeIdRevApcbMapping,
    [
        id_and_rev_and_feature_mask,
        id_and_feature_value,
        rev_and_feature_value,
        board_instance_index,
    ]
);
make_serde!(
    SlinkRegion,
    SerdeSlinkRegion,
    [
        size,
        alignment,
        socket,
        phys_nbio_map,
        interleaving,
    ]
);
make_serde!(
    AblConsoleOutControl,
    SerdeAblConsoleOutControl,
    [
        enable_console_logging,
        enable_mem_flow_logging,
        enable_mem_setreg_logging,
        enable_mem_getreg_logging,
        enable_mem_status_logging,
        enable_mem_pmu_logging,
        enable_mem_pmu_sram_read_logging,
        enable_mem_pmu_sram_write_logging,
        enable_mem_test_verbose_logging,
        enable_mem_basic_output_logging,
        abl_console_port,
    ]
);
make_serde!(
    AblBreakpointControl,
    SerdeAblBreakpointControl,
    [
        enable_breakpoint,
        break_on_all_dies,
    ]
);
make_serde!(
    ExtVoltageControl,
    SerdeExtVoltageControl,
    [
        enabled,
        input_port,
        output_port,
        input_port_size,
        output_port_size,
        input_port_type,
        output_port_type,
        clear_acknowledgement,
    ]
);
make_serde!(
    LrdimmDdr4DataBusElement,
    SerdeLrdimmDdr4DataBusElement,
    [
        dimm_slots_per_channel,
        ddr_rates,
        vdd_io,
        dimm0_ranks,
        dimm1_ranks,
        rtt_nom,
        rtt_wr,
        rtt_park,
        dq_drive_strength,
        dqs_drive_strength,
        odt_drive_strength,
        pmu_phy_vref,
        vref_dq,
    ]
);
make_serde!(
    MaxFreqElement,
    SerdeMaxFreqElement,
    [
        dimm_slots_per_channel,
        conditions,
        speeds,
    ]
);
make_serde!(
    LrMaxFreqElement,
    SerdeLrMaxFreqElement,
    [
        dimm_slots_per_channel,
        conditions,
        speeds,
    ]
);
make_serde!(
    Gpio,
    SerdeGpio,
    [
        pin,
        iomux_control,
        bank_control,
    ]
);
make_serde!(
    ErrorOutControlBeepCode,
    CustomSerdeErrorOutControlBeepCode,
    [
        error_type,
        peak_map,
        peak_attr,
    ]
);
make_serde!(
    ErrorOutControl116,
    CustomSerdeErrorOutControl116,
    [
        enable_error_reporting,
        enable_error_reporting_gpio,
        enable_error_reporting_beep_codes,
        enable_using_handshake,
        input_port,
        output_delay,
        output_port,
        stop_on_first_fatal_error,
        input_port_size,
        output_port_size,
        input_port_type,
        output_port_type,
        clear_acknowledgement,
        error_reporting_gpio,
        beep_code_table,
        enable_heart_beat,
        enable_power_good_gpio,
        power_good_gpio,
    ]
);
make_serde!(
    ErrorOutControl112,
    CustomSerdeErrorOutControl112,
    [
        enable_error_reporting,
        enable_error_reporting_gpio,
        enable_error_reporting_beep_codes,
        enable_using_handshake,
        input_port,
        output_delay,
        output_port,
        stop_on_first_fatal_error,
        input_port_size,
        output_port_size,
        input_port_type,
        output_port_type,
        clear_acknowledgement,
        error_reporting_gpio,
        beep_code_table,
        enable_heart_beat,
        enable_power_good_gpio,
        power_good_gpio,
    ]
);

make_serde!(
    DimmsPerChannelSelector,
    SerdeDimmsPerChannelSelector,
    [
        one_dimm,
        two_dimms,
        three_dimms,
        four_dimms,
    ]
);

make_serde!(
    ErrorOutControlBeepCodePeakAttr,
    SerdeErrorOutControlBeepCodePeakAttr,
    [
        peak_count,
        pulse_width,
        repeat_count,
    ]
);

make_serde!(
    OdtPatPatterns,
    SerdeOdtPatPatterns,
    [
        reading_pattern,
        writing_pattern,
    ]
);

make_serde!(
    LrdimmDdr4OdtPatDimmRankBitmaps,
    SerdeLrdimmDdr4OdtPatDimmRankBitmaps,
    [
        dimm0,
        dimm1,
        dimm2,
    ]
);
make_serde!(
    Ddr4OdtPatDimmRankBitmaps,
    SerdeDdr4OdtPatDimmRankBitmaps,
    [
        dimm0,
        dimm1,
        dimm2,
    ]
);

