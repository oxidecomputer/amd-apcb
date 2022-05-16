// This file contains the serializers for the ondisk formats.
// These are meant automatically make serde use a temporary serde-aware struct
// as a proxy when serializing/deserializing a non-serde-aware struct. Note that
// if too many fields are private, it means that those are not in the proxy
// struct in the first place. This might cause problems. Also, serialization can
// fail if the nice simple user-visible type cannot represent what we are doing.

use crate::df::*;
use crate::memory::platform_tuning::*;
use crate::memory::*;
use crate::ondisk::memory::platform_specific_override::*;
use crate::ondisk::*;
use crate::psp::*;
use crate::struct_accessors::DummyErrorChecks;

// Note: This is written such that it will fail if the underlying struct has
// fields added/removed/renamed--if those have a public setter.
macro_rules! make_serde{($StructName:ident, $SerdeStructName:ident, [$($field_name:ident),* $(,)?]
) => (
    paste::paste!{
        #[cfg(feature = "serde")]
        impl<'de> serde::de::Deserialize<'de> for $StructName {
            fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
            where D: serde::de::Deserializer<'de>, {
                let config = $SerdeStructName::deserialize(deserializer)?;
                Ok($StructName::default()
                $(
                .[<serde_with_ $field_name>](config.$field_name.into())
                )*.build())
                }
        }
        #[cfg(feature = "serde")]
        impl serde::Serialize for $StructName {
            fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
            where S: serde::Serializer, {
                $SerdeStructName {
                    $(
                        $field_name: self.[<serde_ $field_name>]().map_err(|_| serde::ser::Error::custom("value unknown"))?.into(),
                    )*
                }.serialize(serializer)
            }
        }
        #[cfg(feature = "schemars")]
        impl schemars::JsonSchema for $StructName {
            fn schema_name() -> String {
                $SerdeStructName::schema_name()
            }
            fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
                $SerdeStructName::json_schema(gen)
            }
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
    [
        hard_force,
        high,
        medium,
        event_logging,
        low,
        normal,
        _reserved_1,
    ]
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
    [unpopulated, lr, _reserved_1,]
);
make_serde!(
    DdrRates,
    SerdeDdrRates,
    [
        _reserved_1,
        _reserved_2,
        _reserved_3,
        ddr400,
        ddr533,
        ddr667,
        ddr800,
        _reserved_4,
        ddr1066,
        _reserved_5,
        ddr1333,
        _reserved_6,
        ddr1600,
        _reserved_7,
        ddr1866,
        _reserved_8,
        ddr2133,
        _reserved_9,
        ddr2400,
        _reserved_10,
        ddr2667,
        _reserved_11,
        ddr2933,
        _reserved_12,
        ddr3200,
        _reserved_13,
        _reserved_14,
        _reserved_15,
        _reserved_16,
        _reserved_17,
        _reserved_18,
        _reserved_19,
    ]
);
make_serde!(
    RdimmDdr4Voltages,
    SerdeRdimmDdr4Voltages,
    [v_1_2, _reserved_1,]
);
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
        _reserved_,
        slow_mode,
        _reserved_2,
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
    [v_1_5, v_1_35, v_1_25, _reserved_1,]
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
        _reserved_,
        slow_mode,
        _reserved_2,
        address_command_control,
        cke_drive_strength,
        cs_odt_drive_strength,
        address_command_drive_strength,
        clk_drive_strength,
    ]
);
make_serde!(
    LrdimmDdr4Voltages,
    SerdeLrdimmDdr4Voltages,
    [v_1_2, _reserved_1,]
);
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
        _reserved_,
        slow_mode,
        _reserved_2,
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
        _reserved_1,
        _reserved_2,
    ]
);
make_serde!(
    V3_HEADER_EXT,
    SerdeV3_HEADER_EXT,
    [
        signature,
        _reserved_1,
        _reserved_2,
        struct_version,
        data_version,
        ext_header_size,
        _reserved_3,
        _reserved_4,
        _reserved_5,
        _reserved_6,
        _reserved_7,
        data_offset,
        header_checksum,
        _reserved_8,
        _reserved_9,
        integrity_sign,
        _reserved_10,
        signature_ending,
    ]
);
make_serde!(
    GROUP_HEADER,
    SerdeGROUP_HEADER,
    [
        signature,
        group_id,
        header_size,
        version,
        _reserved_,
        group_size,
    ]
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
        _reserved_,
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
        _reserved_,
        abl_console_port,
    ]
);
make_serde!(
    AblBreakpointControl,
    SerdeAblBreakpointControl,
    [enable_breakpoint, break_on_all_dies,]
);
make_serde!(
    ExtVoltageControl,
    SerdeExtVoltageControl,
    [
        enabled,
        _reserved_,
        input_port,
        output_port,
        input_port_size,
        output_port_size,
        input_port_type,
        output_port_type,
        clear_acknowledgement,
        _reserved_2,
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
    [dimm_slots_per_channel, _reserved_, conditions, speeds,]
);
make_serde!(
    LrMaxFreqElement,
    SerdeLrMaxFreqElement,
    [dimm_slots_per_channel, _reserved_, conditions, speeds,]
);
make_serde!(Gpio, SerdeGpio, [pin, iomux_control, bank_control,]);
make_serde!(
    ErrorOutControlBeepCode,
    CustomSerdeErrorOutControlBeepCode,
    [custom_error_type, peak_map, peak_attr,]
);
make_serde!(
    ErrorOutControl116,
    SerdeErrorOutControl116,
    [
        enable_error_reporting,
        enable_error_reporting_gpio,
        enable_error_reporting_beep_codes,
        enable_using_handshake,
        input_port,
        output_delay,
        output_port,
        stop_on_first_fatal_error,
        _reserved_,
        input_port_size,
        output_port_size,
        input_port_type,
        output_port_type,
        clear_acknowledgement,
        _reserved_before_gpio,
        error_reporting_gpio,
        _reserved_after_gpio,
        beep_code_table,
        enable_heart_beat,
        enable_power_good_gpio,
        power_good_gpio,
        _reserved_end,
    ]
);
make_serde!(
    ErrorOutControl112,
    SerdeErrorOutControl112,
    [
        enable_error_reporting,
        enable_error_reporting_gpio,
        enable_error_reporting_beep_codes,
        enable_using_handshake,
        input_port,
        output_delay,
        output_port,
        stop_on_first_fatal_error,
        _reserved_,
        input_port_size,
        output_port_size,
        input_port_type,
        output_port_type,
        clear_acknowledgement,
        _reserved_before_gpio,
        error_reporting_gpio,
        _reserved_after_gpio,
        beep_code_table,
        enable_heart_beat,
        enable_power_good_gpio,
        power_good_gpio,
        _reserved_end,
    ]
);

make_serde!(
    DimmsPerChannelSelector,
    SerdeDimmsPerChannelSelector,
    [one_dimm, two_dimms, three_dimms, four_dimms, _reserved_1,]
);

make_serde!(
    ErrorOutControlBeepCodePeakAttr,
    SerdeErrorOutControlBeepCodePeakAttr,
    [peak_count, pulse_width, repeat_count, _reserved_1,]
);

make_serde!(
    OdtPatPatterns,
    SerdeOdtPatPatterns,
    [reading_pattern, _reserved_1, writing_pattern, _reserved_2,]
);

make_serde!(
    LrdimmDdr4OdtPatDimmRankBitmaps,
    SerdeLrdimmDdr4OdtPatDimmRankBitmaps,
    [dimm0, dimm1, dimm2, _reserved_1,]
);
make_serde!(
    Ddr4OdtPatDimmRankBitmaps,
    SerdeDdr4OdtPatDimmRankBitmaps,
    [dimm0, dimm1, dimm2, _reserved_1,]
);

make_serde!(
    DimmSlotsSelection,
    SerdeDimmSlotsSelection,
    [
        dimm_slot_0,
        dimm_slot_1,
        dimm_slot_2,
        dimm_slot_3,
        _reserved_1,
    ]
);
make_serde!(
    ChannelIdsSelection,
    SerdeChannelIdsSelection,
    [a, b, c, d, e, f, g, h,]
);

make_serde!(
    SocketIds,
    SerdeSocketIds,
    [
        socket_0, socket_1, socket_2, socket_3, socket_4, socket_5, socket_6,
        socket_7,
    ]
);

make_serde!(
    Ddr4OdtPatElement,
    SerdeDdr4OdtPatElement,
    [
        dimm_rank_bitmaps,
        cs0_odt_patterns,
        cs1_odt_patterns,
        cs2_odt_patterns,
        cs3_odt_patterns,
    ]
);
make_serde!(
    LrdimmDdr4OdtPatElement,
    SerdeLrdimmDdr4OdtPatElement,
    [
        dimm_rank_bitmaps,
        cs0_odt_patterns,
        cs1_odt_patterns,
        cs2_odt_patterns,
        cs3_odt_patterns,
    ]
);
make_serde!(
    CkeTristateMap,
    SerdeCkeTristateMap,
    [type_, payload_size, sockets, channels, dimms, connections,]
);
make_serde!(
    OdtTristateMap,
    SerdeOdtTristateMap,
    [type_, payload_size, sockets, channels, dimms, connections,]
);
make_serde!(
    CsTristateMap,
    SerdeCsTristateMap,
    [type_, payload_size, sockets, channels, dimms, connections,]
);
make_serde!(
    MaxDimmsPerChannel,
    SerdeMaxDimmsPerChannel,
    [type_, payload_size, sockets, channels, dimms, value,]
);
make_serde!(
    MemclkMap,
    SerdeMemclkMap,
    [type_, payload_size, sockets, channels, dimms, connections,]
);
make_serde!(
    MaxChannelsPerSocket,
    SerdeMaxChannelsPerSocket,
    [type_, payload_size, sockets, channels, dimms, value,]
);
make_serde!(
    MemBusSpeed,
    SerdeMemBusSpeed,
    [
        type_,
        payload_size,
        sockets,
        channels,
        dimms,
        timing_mode,
        bus_speed,
    ]
);
make_serde!(
    MaxCsPerChannel,
    SerdeMaxCsPerChannel,
    [type_, payload_size, sockets, channels, dimms, value,]
);
make_serde!(
    MemTechnology,
    SerdeMemTechnology,
    [
        type_,
        payload_size,
        sockets,
        channels,
        dimms,
        technology_type,
    ]
);
make_serde!(
    WriteLevellingSeedDelay,
    SerdeWriteLevellingSeedDelay,
    [
        type_,
        payload_size,
        sockets,
        channels,
        dimms,
        seed,
        ecc_seed,
    ]
);
make_serde!(
    RxEnSeed,
    SerdeRxEnSeed,
    [
        type_,
        payload_size,
        sockets,
        channels,
        dimms,
        seed,
        ecc_seed,
    ]
);
make_serde!(
    LrDimmNoCs6Cs7Routing,
    SerdeLrDimmNoCs6Cs7Routing,
    [type_, payload_size, sockets, channels, dimms, value,]
);
make_serde!(
    SolderedDownSodimm,
    SerdeSolderedDownSodimm,
    [type_, payload_size, sockets, channels, dimms, value,]
);
make_serde!(
    LvDimmForce1V5,
    SerdeLvDimmForce1V5,
    [type_, payload_size, sockets, channels, dimms, value,]
);
make_serde!(
    MinimumRwDataEyeWidth,
    SerdeMinimumRwDataEyeWidth,
    [
        type_,
        payload_size,
        sockets,
        channels,
        dimms,
        min_read_data_eye_width,
        min_write_data_eye_width,
    ]
);
make_serde!(
    CpuFamilyFilter,
    SerdeCpuFamilyFilter,
    [type_, payload_size, cpu_family_revision,]
);
make_serde!(
    SolderedDownDimmsPerChannel,
    SerdeSolderedDownDimmsPerChannel,
    [type_, payload_size, sockets, channels, dimms, value,]
);
make_serde!(
    MemPowerPolicy,
    SerdeMemPowerPolicy,
    [type_, payload_size, sockets, channels, dimms, value,]
);
make_serde!(
    MotherboardLayers,
    SerdeMotherboardLayers,
    [type_, payload_size, sockets, channels, dimms, value,]
);
make_serde!(
    IdApcbMapping,
    SerdeIdApcbMapping,
    [
        id_and_feature_mask,
        id_and_feature_value,
        board_instance_index,
    ]
);
make_serde!(
    BoardIdGettingMethodCustom,
    SerdeBoardIdGettingMethodCustom,
    [access_method, feature_mask,]
);
make_serde!(
    BoardIdGettingMethodGpio,
    SerdeBoardIdGettingMethodGpio,
    [access_method, bit_locations,]
);
make_serde!(
    BoardIdGettingMethodSmbus,
    SerdeBoardIdGettingMethodSmbus,
    [
        access_method,
        i2c_controller_index,
        i2c_mux_address,
        mux_control_address,
        mux_channel,
        smbus_address,
        register_index,
    ]
);
make_serde!(
    FchGppClkMapSelection,
    SerdeFchGppClkMapSelection,
    [
        s0_gpp0_off,
        s0_gpp1_off,
        s0_gpp4_off,
        s0_gpp2_off,
        s0_gpp3_off,
        _reserved_1,
        s1_gpp0_off,
        s1_gpp1_off,
        s1_gpp4_off,
        s1_gpp2_off,
        s1_gpp3_off,
        _reserved_2,
    ]
);
make_serde!(Terminator, SerdeTerminator, [type_,]);
make_serde!(
    DdrPostPackageRepairElement,
    CustomSerdeDdrPostPackageRepairElement,
    [raw_body,]
);
make_serde!(
    DdrPostPackageRepairBody,
    SerdeDdrPostPackageRepairBody,
    [
        bank,
        rank_multiplier,
        xdevice_width,
        chip_select,
        column,
        hard_repair,
        valid,
        target_device,
        row,
        socket,
        channel,
        _reserved_1,
    ]
);
make_serde!(
    DimmInfoSmbusElement,
    SerdeDimmInfoSmbusElement,
    [
        dimm_slot_present,
        socket_id,
        channel_id,
        dimm_id,
        dimm_smbus_address,
        i2c_mux_address,
        mux_control_address,
        mux_channel,
    ]
);
make_serde!(
    ConsoleOutControl,
    SerdeConsoleOutControl,
    [abl_console_out_control, abl_breakpoint_control, _reserved_,]
);
make_serde!(
    BoardInstances,
    SerdeBoardInstances,
    [
        instance_0,
        instance_1,
        instance_2,
        instance_3,
        instance_4,
        instance_5,
        instance_6,
        instance_7,
        instance_8,
        instance_9,
        instance_10,
        instance_11,
        instance_12,
        instance_13,
        instance_14,
        instance_15,
    ]
);