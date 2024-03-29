// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! This file mostly contains the Naples backward-compatibility interface.

use crate::struct_accessors::{Getter, Setter};
use crate::types::Result;
use modular_bitfield::prelude::*;

#[derive(
    Debug, PartialEq, num_derive::FromPrimitive, Clone, Copy, BitfieldSpecifier,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
#[bits = 8]
pub enum ParameterTimePoint {
    Never = 0,
    Any = 1,
}

impl Getter<Result<ParameterTimePoint>> for ParameterTimePoint {
    fn get1(self) -> Result<Self> {
        Ok(self)
    }
}

impl Setter<ParameterTimePoint> for ParameterTimePoint {
    fn set1(&mut self, value: Self) {
        *self = value
    }
}

impl Default for ParameterTimePoint {
    fn default() -> Self {
        Self::Any
    }
}

#[derive(
    Debug, PartialEq, num_derive::FromPrimitive, Clone, Copy, BitfieldSpecifier,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
#[bits = 13]
pub enum ParameterTokenConfig {
    // Cbs
    Cbs00 = 0x00,
    Cbs01 = 0x01,
    Cbs02 = 0x02,
    Cbs03 = 0x03,
    Cbs04 = 0x04,
    Cbs05 = 0x05,
    Cbs06 = 0x06,
    Cbs07 = 0x07,
    Cbs08 = 0x08,
    Cbs09 = 0x09,
    Cbs0a = 0x0a,
    Cbs0b = 0x0b,
    Cbs0c = 0x0c,
    Cbs0d = 0x0d,
    Cbs0e = 0x0e,
    Cbs0f = 0x0f,
    Cbs10 = 0x10,
    Cbs11 = 0x11,
    Cbs12 = 0x12,
    Cbs13 = 0x13,
    Cbs14 = 0x14,
    Cbs15 = 0x15,
    Cbs16 = 0x16,
    Cbs17 = 0x17,
    Cbs18 = 0x18,
    Cbs19 = 0x19,
    Cbs1a = 0x1a,
    Cbs1b = 0x1b,
    Cbs1c = 0x1c,
    Cbs1d = 0x1d,
    Cbs1e = 0x1e,
    Cbs1f = 0x1f,
    Cbs20 = 0x20,
    Cbs21 = 0x21,
    Cbs22 = 0x22,
    Cbs23 = 0x23,
    Cbs24 = 0x24,
    Cbs25 = 0x25,
    Cbs26 = 0x26,
    Cbs27 = 0x27,
    Cbs28 = 0x28,
    Cbs29 = 0x29,
    Cbs2a = 0x2a,
    Cbs2b = 0x2b,
    Cbs2c = 0x2c,
    Cbs2d = 0x2d,
    Cbs2e = 0x2e,
    Cbs2f = 0x2f,
    Cbs30 = 0x30,
    Cbs31 = 0x31,
    Cbs32 = 0x32,
    Cbs33 = 0x33,
    Cbs34 = 0x34,
    Cbs35 = 0x35,
    Cbs36 = 0x36,
    Cbs37 = 0x37,
    Cbs38 = 0x38,
    Cbs39 = 0x39,
    Cbs3a = 0x3a,
    Cbs3b = 0x3b,
    Cbs3c = 0x3c,
    Cbs3d = 0x3d,
    Cbs3e = 0x3e,
    Cbs3f = 0x3f,
    Cbs40 = 0x40,
    Cbs41 = 0x41,
    Cbs42 = 0x42,
    Cbs43 = 0x43,
    Cbs44 = 0x44,
    Cbs45 = 0x45,
    Cbs46 = 0x46,
    Cbs47 = 0x47,
    Cbs48 = 0x48,
    Cbs49 = 0x49,
    Cbs4a = 0x4a,
    Cbs4b = 0x4b,
    Cbs4c = 0x4c,
    Cbs4d = 0x4d,
    Cbs4e = 0x4e,
    Cbs4f = 0x4f,
    Cbs50 = 0x50,
    Cbs51 = 0x51,
    Cbs52 = 0x52,
    Cbs53 = 0x53,
    Cbs54 = 0x54,
    Cbs55 = 0x55,
    Cbs56 = 0x56,
    Cbs57 = 0x57,
    Cbs58 = 0x58,
    Cbs59 = 0x59,
    Cbs5a = 0x5a,
    Cbs5b = 0x5b,
    Cbs5c = 0x5c,
    Cbs5d = 0x5d,
    Cbs5e = 0x5e,
    Cbs5f = 0x5f,
    Cbs60 = 0x60,
    Cbs61 = 0x61,
    Cbs62 = 0x62,
    Cbs63 = 0x63,
    Cbs64 = 0x64,
    Cbs65 = 0x65,
    Cbs66 = 0x66,
    Cbs67 = 0x67,
    Cbs68 = 0x68,
    Cbs69 = 0x69,
    Cbs6a = 0x6a,
    Cbs6b = 0x6b,
    Cbs6c = 0x6c,
    Cbs6d = 0x6d,
    Cbs6e = 0x6e,
    Cbs6f = 0x6f,
    Cbs70 = 0x70,
    Cbs71 = 0x71,
    Cbs72 = 0x72,
    Cbs73 = 0x73,
    Cbs74 = 0x74,
    Cbs75 = 0x75,
    Cbs76 = 0x76,
    Cbs77 = 0x77,
    Cbs78 = 0x78,
    Cbs79 = 0x79,
    Cbs7a = 0x7a,
    Cbs7b = 0x7b,
    Cbs7c = 0x7c,
    Cbs7d = 0x7d,
    Cbs7e = 0x7e,
    Cbs7f = 0x7f,
    Cbs80 = 0x80,
    Cbs81 = 0x81,
    Cbs82 = 0x82,
    Cbs83 = 0x83,
    Cbs84 = 0x84,
    Cbs85 = 0x85,
    Cbs86 = 0x86,
    Cbs87 = 0x87,
    Cbs88 = 0x88,
    Cbs89 = 0x89,
    Cbs8a = 0x8a,
    Cbs8b = 0x8b,
    Cbs8c = 0x8c,
    Cbs8d = 0x8d,
    Cbs8e = 0x8e,
    Cbs8f = 0x8f,
    Cbs90 = 0x90,
    Cbs91 = 0x91,
    Cbs92 = 0x92,
    Cbs93 = 0x93,
    Cbs94 = 0x94,
    Cbs95 = 0x95,
    Cbs96 = 0x96,
    Cbs97 = 0x97,
    Cbs98 = 0x98,
    Cbs99 = 0x99,
    Cbs9a = 0x9a,
    Cbs9b = 0x9b,
    Cbs9c = 0x9c,
    Cbs9d = 0x9d,
    Cbs9e = 0x9e,
    Cbs9f = 0x9f,
    Cbsa0 = 0xa0,
    Cbsa1 = 0xa1,
    Cbsa2 = 0xa2,
    Cbsa3 = 0xa3,
    Cbsa4 = 0xa4,
    Cbsa5 = 0xa5,
    Cbsa6 = 0xa6,
    Cbsa7 = 0xa7,
    Cbsa8 = 0xa8,
    Cbsa9 = 0xa9,
    Cbsaa = 0xaa,
    Cbsab = 0xab,
    Cbsac = 0xac,
    Cbsad = 0xad,
    Cbsae = 0xae,
    Cbsaf = 0xaf,
    Cbsb0 = 0xb0,
    Cbsb1 = 0xb1,
    Cbsb2 = 0xb2,
    Cbsb3 = 0xb3,
    Cbsb4 = 0xb4,
    Cbsb5 = 0xb5,
    Cbsb6 = 0xb6,
    Cbsb7 = 0xb7,
    Cbsb8 = 0xb8,
    Cbsb9 = 0xb9,
    Cbsba = 0xba,
    Cbsbb = 0xbb,
    Cbsbc = 0xbc,
    Cbsbd = 0xbd,
    Cbsbe = 0xbe,
    Cbsbf = 0xbf,
    Cbsc0 = 0xc0,
    Cbsc1 = 0xc1,
    Cbsc2 = 0xc2,
    Cbsc3 = 0xc3,
    Cbsc4 = 0xc4,
    Cbsc5 = 0xc5,
    Cbsc6 = 0xc6,
    Cbsc7 = 0xc7,
    Cbsc8 = 0xc8,
    Cbsc9 = 0xc9,
    Cbsca = 0xca,
    Cbscb = 0xcb,
    Cbscc = 0xcc,
    Cbscd = 0xcd,
    Cbsce = 0xce,
    Cbscf = 0xcf,
    Cbsd0 = 0xd0,
    Cbsd1 = 0xd1,
    Cbsd2 = 0xd2,
    Cbsd3 = 0xd3,
    Cbsd4 = 0xd4,
    Cbsd5 = 0xd5,
    Cbsd6 = 0xd6,
    Cbsd7 = 0xd7,
    Cbsd8 = 0xd8,
    Cbsd9 = 0xd9,
    Cbsda = 0xda,
    Cbsdb = 0xdb,
    Cbsdc = 0xdc,
    Cbsdd = 0xdd,
    Cbsde = 0xde,
    Cbsdf = 0xdf,
    Cbse0 = 0xe0,
    Cbse1 = 0xe1,
    Cbse2 = 0xe2,
    Cbse3 = 0xe3,
    Cbse4 = 0xe4,
    Cbse5 = 0xe5,
    Cbse6 = 0xe6,
    Cbse7 = 0xe7,
    Cbse8 = 0xe8,
    Cbse9 = 0xe9,
    Cbsea = 0xea,
    Cbseb = 0xeb,
    Cbsec = 0xec,
    Cbsed = 0xed,
    Cbsee = 0xee,
    Cbsef = 0xef,
    Cbsf0 = 0xf0,
    Cbsf1 = 0xf1,
    Cbsf2 = 0xf2,
    Cbsf3 = 0xf3,
    Cbsf4 = 0xf4,
    Cbsf5 = 0xf5,
    Cbsf6 = 0xf6,
    Cbsf7 = 0xf7,
    Cbsf8 = 0xf8,
    Cbsf9 = 0xf9,
    Cbsfa = 0xfa,
    Cbsfb = 0xfb,
    Cbsfc = 0xfc,
    Cbsfd = 0xfd,
    Cbsfe = 0xfe,
    Cbsff = 0xff,

    // Ccx
    CcxMinSevAsid = 0x0101,

    // Df
    DfGmiEncrypt = 0x0301,
    DfXgmiEncrypt = 0x0302,
    DfSaveRestoreMemEncrypt = 0x0303,
    DfSysStorageAtTopOfMem = 0x0304,
    DfProbeFilter = 0x0305,
    DfBottomIo = 0x0306,
    DfMemInterleaving = 0x0307,
    DfMemInterleavingSize = 0x0308,
    DfMemInterleavingHash = 0x0309,
    DfPciMmioSize = 0x030A,
    DfCakeCrcThreshPerfBounds = 0x030B,
    DfMemClear = 0x030C,

    // TODO: mem
    MemBottomIo = 0x0701,
    MemHoleRemapping = 0x0702,
    MemLimitToBelow1TiB = 0x0703,
    MemUserTimingMode = 0x0704,
    MemClockValue = 0x0705,
    MemEnableChipSelectInterleaving = 0x0706,
    MemEnableChannelInterleaving = 0x0707,
    MemEnableEccFeature = 0x0708,
    MemEnablePowerDown = 0x0709,
    MemEnableParity = 0x070A,
    MemEnableBankSwizzle = 0x070B,
    MemEnableClearing = 0x070C,
    MemUmaMode = 0x070D,
    MemUmaSize = 0x070E,
    MemRestoreControl = 0x070F,
    MemSaveMemContextControl = 0x0710,
    MemIsCapsuleMode = 0x0711,
    MemForceTraining = 0x0712,
    MemDimmTypeMixedConfig = 0x0713,
    MemEnableAmp = 0x0714,
    MemDramDoubleRefreshRate = 0x0715,
    MemPmuTrainingMode = 0x0716,
    MemEccRedirection = 0x0717,
    MemScrubDramRate = 0x0718,
    MemScrubL2Rate = 0x0719,
    MemScrubL3Rate = 0x071A,
    MemScrubInstructionCacheRate = 0x071B,
    MemScrubDataCacheRate = 0x071C,
    MemEccSyncFlood = 0x071D,
    MemEccSymbolSize = 0x071E,
    MemDqsTrainingControl = 0x071F,
    MemUmaAbove4GiB = 0x0720,
    MemUmaAlignment = 0x0721,
    MemEnableAllClocks = 0x0722,
    MemBusFrequencyLimit = 0x0723,
    MemPowerDownMode = 0x0724,
    MemIgnoreSpdChecksum = 0x0725,
    MemModeUnganged = 0x0726,
    MemQuadRankCapable = 0x0727,
    MemRdimmCapable = 0x0728,
    MemLrdimmCapable = 0x0729,
    MemUdimmCapable = 0x072A,
    MemSodimmCapable = 0x072B,
    MemEnableDoubleRefreshRate = 0x072C,
    MemDimmTypeDdr4Capable = 0x072D,
    MemDimmTypeDdr3Capable = 0x072E,
    MemDimmTypeLpddr3Capable = 0x072F,
    MemEnableZqReset = 0x0730,
    MemEnableBankGroupSwap = 0x0731,
    MemEnableOdtsCmdThrottle = 0x0732,
    MemEnableSwCmdThrottle = 0x0733,
    MemEnableForcePowerDownThrotle = 0x0734,
    MemOdtsCmdThrottleCycles = 0x0735,
    // See PPR SwCmdThrotCyc
    MemSwCmdThrottleCycles = 0x0736,
    MemDimmSensorConf = 0x0737,
    MemDimmSensorUpper = 0x0738,
    MemDimmSensorLower = 0x0739,
    MemDimmSensorCritical = 0x073A,
    MemDimmSensorResolution = 0x073B,
    MemAutoRefreshFineGranMode = 0x073C,
    MemEnablePState = 0x073D,
    MemSolderedDown = 0x073E,
    MemDdrRouteBalancedTee = 0x073F,
    MemEnableMbistTest = 0x0740,
    MemEnableTsme = 0x0746,
    MemPlatformSpecificErrorHandling = 0x074A,
    MemEnableTemperatureControlledRefresh = 0x074B,
    MemEnableBankGroupSwapAlt = 0x074D,
    MemEnd = 0x074E,
    Mem74f = 0x074F, // FIXME
    Mem750 = 0x0750, // FIXME

    GnbBmcSocketNumber = 0x1801,
    GnbBmcStartLane = 0x1802,
    GnbBmcEndLane = 0x1803,
    GnbBmcDevice = 0x1804,
    GnbBmcFunction = 0x1805,
    GnbPcieResetControl = 0x1806,
    GnbEnd = 0x1807,

    FchConsoleOutEnable = 0x1C01,
    FchConsoleOutSerialPort = 0x1C02,
    FchSmbusSpeed = 0x1C03,
    FchEnd = 0x1C04,
    Fch1c05 = 0x1C05, // FIXME
    Fch1c06 = 0x1C06, // FIXME
    Fch1c07 = 0x1C07, // FIXME

    Limit = 0x1FFF,
}

impl Default for ParameterTokenConfig {
    fn default() -> Self {
        Self::Limit
    }
}

impl Getter<Result<ParameterTokenConfig>> for ParameterTokenConfig {
    fn get1(self) -> Result<Self> {
        Ok(self)
    }
}

impl Setter<ParameterTokenConfig> for ParameterTokenConfig {
    fn set1(&mut self, value: Self) {
        *self = value
    }
}
