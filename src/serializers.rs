// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! This file contains the serializers for the ondisk formats.
//! These are meant automatically make serde use a temporary serde-aware
//! struct as a proxy when serializing/deserializing a non-serde-aware struct.
//! Note that fields which are private are not in the proxy struct in
//! the first place. This might cause problems. Also, serialization can
//! fail if the nice simple user-visible type cannot represent what we
//! are doing.

use crate::df::*;
use crate::fch::*;
use crate::memory::platform_tuning::*;
use crate::memory::*;
use crate::ondisk::memory::platform_specific_override::*;
use crate::ondisk::*;
use crate::psp::*;

// Note: This is written such that it will fail if the underlying struct has
// fields added/removed/renamed--if those have a public setter.
macro_rules! impl_struct_serde_conversion{($StructName:ident, $SerdeStructName:ident, [$($field_name:ident),* $(,)?]
) => (
    paste::paste!{
        #[cfg(feature = "serde")]
        impl<'de> serde::de::Deserialize<'de> for $StructName {
            fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
            where D: serde::de::Deserializer<'de>, {
                let config = $SerdeStructName::deserialize(deserializer)?;
                Ok($StructName::builder()
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
                        $field_name: self.[<serde_ $field_name>]().map_err(|_| serde::ser::Error::custom(format!("value unknown for {}.{}", stringify!($StructName), stringify!($field_name))))?.into(),
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
            fn is_referenceable() -> bool {
                $SerdeStructName::is_referenceable()
            }
        }
    }
)}

impl_struct_serde_conversion!(
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

impl_struct_serde_conversion!(
    PriorityLevels,
    SerdePriorityLevels,
    [hard_force, high, medium, event_logging, low, normal, _reserved_1,]
);

impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    Ddr4DimmRanks,
    SerdeDdr4DimmRanks,
    [unpopulated, single_rank, dual_rank, quad_rank,]
);
impl_struct_serde_conversion!(
    LrdimmDdr4DimmRanks,
    SerdeLrdimmDdr4DimmRanks,
    [unpopulated, lr, _reserved_1,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    RdimmDdr4Voltages,
    CustomSerdeRdimmDdr4Voltages,
    [_1_2V, _reserved_1,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    UdimmDdr4Voltages,
    CustomSerdeUdimmDdr4Voltages,
    [_1_5V, _1_35V, _1_25V, _reserved_1,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    LrdimmDdr4Voltages,
    CustomSerdeLrdimmDdr4Voltages,
    [_1_2V, _reserved_1,]
);
impl_struct_serde_conversion!(
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

impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    GROUP_HEADER,
    SerdeGROUP_HEADER,
    [signature, group_id, header_size, version, _reserved_, group_size,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    IdRevApcbMapping,
    SerdeIdRevApcbMapping,
    [
        id_and_rev_and_feature_mask,
        id_and_feature_value,
        rev_and_feature_value,
        board_instance_index,
    ]
);
impl_struct_serde_conversion!(
    SlinkRegion,
    SerdeSlinkRegion,
    [size, alignment, socket, phys_nbio_map, interleaving, _reserved_,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    NaplesAblConsoleOutControl,
    SerdeNaplesAblConsoleOutControl,
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
        _reserved_0,
        abl_console_port,
    ]
);
impl_struct_serde_conversion!(
    AblBreakpointControl,
    SerdeAblBreakpointControl,
    [enable_breakpoint, break_on_all_dies,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    MaxFreqElement,
    SerdeMaxFreqElement,
    [dimm_slots_per_channel, _reserved_, conditions, speeds,]
);
impl_struct_serde_conversion!(
    LrMaxFreqElement,
    SerdeLrMaxFreqElement,
    [dimm_slots_per_channel, _reserved_, conditions, speeds,]
);
impl_struct_serde_conversion!(
    Gpio,
    SerdeGpio,
    [pin, iomux_control, bank_control,]
);
impl_struct_serde_conversion!(
    ErrorOutControlBeepCode,
    CustomSerdeErrorOutControlBeepCode,
    [custom_error_type, peak_map, peak_attr,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
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

impl_struct_serde_conversion!(
    DimmsPerChannelSelector,
    SerdeDimmsPerChannelSelector,
    [one_dimm, two_dimms, three_dimms, four_dimms, _reserved_1,]
);

impl_struct_serde_conversion!(
    ErrorOutControlBeepCodePeakAttr,
    SerdeErrorOutControlBeepCodePeakAttr,
    [peak_count, pulse_width, repeat_count, _reserved_1,]
);

impl_struct_serde_conversion!(
    OdtPatPatterns,
    SerdeOdtPatPatterns,
    [reading_pattern, _reserved_1, writing_pattern, _reserved_2,]
);

impl_struct_serde_conversion!(
    LrdimmDdr4OdtPatDimmRankBitmaps,
    SerdeLrdimmDdr4OdtPatDimmRankBitmaps,
    [dimm0, dimm1, dimm2, _reserved_1,]
);
impl_struct_serde_conversion!(
    Ddr4OdtPatDimmRankBitmaps,
    SerdeDdr4OdtPatDimmRankBitmaps,
    [dimm0, dimm1, dimm2, _reserved_1,]
);

impl_struct_serde_conversion!(
    DimmSlotsSelection,
    SerdeDimmSlotsSelection,
    [dimm_slot_0, dimm_slot_1, dimm_slot_2, dimm_slot_3, _reserved_1,]
);
impl_struct_serde_conversion!(
    ChannelIdsSelection,
    SerdeChannelIdsSelection,
    [a, b, c, d, e, f, g, h,]
);
impl_struct_serde_conversion!(
    ChannelIdsSelection12,
    SerdeChannelIdsSelection12,
    [a, b, c, d, e, f, g, h, i, j, k, l, _reserved_1,]
);

impl_struct_serde_conversion!(
    SocketIds,
    SerdeSocketIds,
    [
        socket_0, socket_1, socket_2, socket_3, socket_4, socket_5, socket_6,
        socket_7,
    ]
);

impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    CkeTristateMap,
    SerdeCkeTristateMap,
    [type_, payload_size, sockets, channels, dimms, connections,]
);
impl_struct_serde_conversion!(
    OdtTristateMap,
    SerdeOdtTristateMap,
    [type_, payload_size, sockets, channels, dimms, connections,]
);
impl_struct_serde_conversion!(
    CsTristateMap,
    SerdeCsTristateMap,
    [type_, payload_size, sockets, channels, dimms, connections,]
);
impl_struct_serde_conversion!(
    MaxDimmsPerChannel,
    SerdeMaxDimmsPerChannel,
    [type_, payload_size, sockets, channels, dimms, value,]
);
impl_struct_serde_conversion!(
    MaxDimmsPerChannel6,
    SerdeMaxDimmsPerChannel6,
    [type_, payload_size, sockets, channels, dimms, value, _padding_0]
);
impl_struct_serde_conversion!(
    MemclkMap,
    SerdeMemclkMap,
    [type_, payload_size, sockets, channels, dimms, connections,]
);
impl_struct_serde_conversion!(
    MaxChannelsPerSocket,
    SerdeMaxChannelsPerSocket,
    [type_, payload_size, sockets, channels, dimms, value,]
);
impl_struct_serde_conversion!(
    MemBusSpeed,
    SerdeMemBusSpeed,
    [type_, payload_size, sockets, channels, dimms, timing_mode, bus_speed,]
);
impl_struct_serde_conversion!(
    MaxCsPerChannel,
    SerdeMaxCsPerChannel,
    [type_, payload_size, sockets, channels, dimms, value,]
);
impl_struct_serde_conversion!(
    MemTechnology,
    SerdeMemTechnology,
    [type_, payload_size, sockets, channels, dimms, technology_type,]
);
impl_struct_serde_conversion!(
    WriteLevellingSeedDelay,
    SerdeWriteLevellingSeedDelay,
    [type_, payload_size, sockets, channels, dimms, seed, ecc_seed,]
);
impl_struct_serde_conversion!(
    RxEnSeed,
    SerdeRxEnSeed,
    [type_, payload_size, sockets, channels, dimms, seed, ecc_seed,]
);
impl_struct_serde_conversion!(
    LrDimmNoCs6Cs7Routing,
    SerdeLrDimmNoCs6Cs7Routing,
    [type_, payload_size, sockets, channels, dimms, value,]
);
impl_struct_serde_conversion!(
    SolderedDownSodimm,
    SerdeSolderedDownSodimm,
    [type_, payload_size, sockets, channels, dimms, value,]
);
impl_struct_serde_conversion!(
    LvDimmForce1V5,
    SerdeLvDimmForce1V5,
    [type_, payload_size, sockets, channels, dimms, value,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    CpuFamilyFilter,
    SerdeCpuFamilyFilter,
    [type_, payload_size, cpu_family_revision,]
);
impl_struct_serde_conversion!(
    SolderedDownDimmsPerChannel,
    SerdeSolderedDownDimmsPerChannel,
    [type_, payload_size, sockets, channels, dimms, value,]
);
impl_struct_serde_conversion!(
    MemPowerPolicy,
    SerdeMemPowerPolicy,
    [type_, payload_size, sockets, channels, dimms, value,]
);
impl_struct_serde_conversion!(
    MotherboardLayers,
    SerdeMotherboardLayers,
    [type_, payload_size, sockets, channels, dimms, value,]
);
impl_struct_serde_conversion!(
    IdApcbMapping,
    SerdeIdApcbMapping,
    [id_and_feature_mask, id_and_feature_value, board_instance_index,]
);
impl_struct_serde_conversion!(
    BoardIdGettingMethodCustom,
    SerdeBoardIdGettingMethodCustom,
    [access_method, feature_mask,]
);
impl_struct_serde_conversion!(
    BoardIdGettingMethodGpio,
    SerdeBoardIdGettingMethodGpio,
    [access_method, bit_locations,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    FchGppClkMapSelection,
    SerdeFchGppClkMapSelection,
    [
        s0_gpp0_off,
        s0_gpp1_off,
        s0_gpp4_off,
        s0_gpp2_off,
        s0_gpp3_off,
        s0_gpp5_off,
        _reserved_1,
        s1_gpp0_off,
        s1_gpp1_off,
        s1_gpp4_off,
        s1_gpp2_off,
        s1_gpp3_off,
        s1_gpp5_off,
        _reserved_2,
    ]
);
impl_struct_serde_conversion!(Terminator, SerdeTerminator, [type_,]);
impl_struct_serde_conversion!(
    DdrPostPackageRepairElement,
    CustomSerdeDdrPostPackageRepairElement,
    [raw_body,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    ConsoleOutControl,
    SerdeConsoleOutControl,
    [abl_console_out_control, abl_breakpoint_control, _reserved_,]
);
impl_struct_serde_conversion!(
    NaplesConsoleOutControl,
    SerdeNaplesConsoleOutControl,
    [abl_console_out_control, abl_breakpoint_control, _reserved_,]
);
impl_struct_serde_conversion!(
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
impl_struct_serde_conversion!(
    Parameter,
    SerdeParameter,
    [time_point, token, value_size, value, _reserved_0,]
);
impl_struct_serde_conversion!(
    ParameterAttributes,
    SerdeParameterAttributes,
    [time_point, token, size_minus_one, _reserved_0,]
);
impl_struct_serde_conversion!(
    MemPmuBistTestSelect,
    SerdeMemPmuBistTestSelect,
    [
        algorithm_1,
        algorithm_2,
        algorithm_3,
        algorithm_4,
        algorithm_5,
        _reserved_0,
    ]
);
impl_struct_serde_conversion!(
    MemPmuBistAlgorithmSelect,
    SerdeMemPmuBistAlgorithmSelect,
    [
        algorithm_1,
        algorithm_2,
        algorithm_3,
        algorithm_4,
        algorithm_5,
        algorithm_6,
        algorithm_7,
        algorithm_8,
        algorithm_9,
        _reserved_0,
    ]
);
impl_struct_serde_conversion!(
    RdimmDdr5BusElementPayload,
    SerdeRdimmDdr5BusElementPayload,
    [
        total_size,
        ca_timing_mode,
        dimm0_rttnomwr,
        dimm0_rttnomrd,
        dimm0_rttwr,
        dimm0_rttpack,
        dimm0_dqs_rttpark,
        dimm1_rttnomwr,
        dimm1_rttnomrd,
        dimm1_rttwr,
        dimm1_rttpack,
        dimm1_dqs_rttpark,
        dram_drv,
        ck_odt_a,
        cs_odt_a,
        ca_odt_a,
        ck_odt_b,
        cs_odt_b,
        ca_odt_b,
        p_odt,
        dq_drv,
        alert_pullup,
        ca_drv,
        phy_vref,
        dq_vref,
        ca_vref,
        cs_vref,
        d_ca_vref,
        d_cs_vref,
        rx_dfe,
        tx_dfe,
    ]
);
impl_struct_serde_conversion!(
    Ddr5CaPinMapElementLane,
    SerdeDdr5CaPinMapElementLane,
    [pins,]
);
impl_struct_serde_conversion!(
    DdrDqPinMapElementLane,
    SerdeDdrDqPinMapElementLane,
    [pins,]
);
impl_struct_serde_conversion!(
    DdrDqPinMapElement,
    SerdeDdrDqPinMapElement,
    [lanes,]
);
impl_struct_serde_conversion!(
    RdimmDdr5BusElementHeader,
    SerdeRdimmDdr5BusElementHeader,
    [
        total_size,
        target_memclk,
        dimm_slots_per_channel,
        dimm0_rank_bitmap,
        dimm1_rank_bitmap,
        sdram_io_width_bitmap,
    ]
);
impl_struct_serde_conversion!(
    RdimmDdr5BusElement,
    SerdeRdimmDdr5BusElement,
    [header, payload,]
);
impl_struct_serde_conversion!(
    EspiInit,
    SerdeEspiInit,
    [
        espi_enabled,
        data_bus_select,
        clock_pin_select,
        cs_pin_select,
        clock_frequency,
        io_mode,
        alert_mode,
        pltrst_deassert,
        io80_decoding_enabled,
        io6064_decoding_enabled,
        io_range_size,
        io_range_base,
        mmio_range_size,
        mmio_range_base,
        irq_mask,
        irq_polarity,
        cputemp_rtctime_vw_enabled,
        cputemp_rtctime_vw_index_select,
        _dummy_1,
        _dummy_2,
        cpu_temp_mmio_base, // FIXME
        rtc_time_mmio_base, // FIXME
        bus_master_enabled,
        _dummy_3,
        _dummy_4,
        _dummy_5,
    ]
);
impl_struct_serde_conversion!(
    Ddr5CaPinMapElement,
    SerdeDdr5CaPinMapElement,
    [lanes,]
);
impl_struct_serde_conversion!(
    MemDfeSearchElementHeader,
    SerdeMemDfeSearchElementHeader,
    [
        total_size,
        dimm_slots_per_channel,
        dimm0_rank_bitmap,
        dimm1_rank_bitmap,
        sdram_io_width_bitmap,
    ]
);
impl_struct_serde_conversion!(
    MemDfeSearchElementPayload,
    SerdeMemDfeSearchElementPayload,
    [
        total_size,
        tx_dfe_tap_1_start,
        tx_dfe_tap_1_end,
        tx_dfe_tap_2_start,
        tx_dfe_tap_2_end,
        tx_dfe_tap_3_start,
        tx_dfe_tap_3_end,
        tx_dfe_tap_4_start,
        tx_dfe_tap_4_end,
    ]
);
impl_struct_serde_conversion!(
    MemDfeSearchElementPayloadExt,
    SerdeMemDfeSearchElementPayloadExt,
    [
        total_size,
        rx_dfe_tap_2_min_mv,
        rx_dfe_tap_2_max_mv,
        rx_dfe_tap_3_min_mv,
        rx_dfe_tap_3_max_mv,
        rx_dfe_tap_4_min_mv,
        rx_dfe_tap_4_max_mv,
    ]
);
impl_struct_serde_conversion!(
    MemDfeSearchElement20,
    SerdeMemDfeSearchElement20,
    [header, payload,]
);
impl_struct_serde_conversion!(
    MemDfeSearchElement32,
    SerdeMemDfeSearchElement32,
    [header, payload, payload_ext, _padding_0, _padding_1,]
);
impl_struct_serde_conversion!(
    DfXgmiChannelTypeSelect,
    SerdeDfXgmiChannelTypeSelect,
    [
        s0l0,
        s0l1,
        s0l2,
        s0l3,
        s1l0,
        s1l1,
        s1l2,
        s1l3,
    ]
);
impl_struct_serde_conversion!(
    FchConsoleOutSerialPortEspiControllerSelect,
    SerdeFchConsoleOutSerialPortEspiControllerSelect,
    [
        espi_controller,
        _reserved_0,
        io_2e_2f_disabled,
        io_4e_4f_disabled,
        _reserved_1,
    ]
);
