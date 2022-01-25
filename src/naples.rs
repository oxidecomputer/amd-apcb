// This file mostly contains the Naples backward-compatibility interface.

use modular_bitfield::prelude::*;
//use crate::struct_accessors::make_accessors;

#[derive(Debug, PartialEq, num_derive::FromPrimitive, Clone, Copy, BitfieldSpecifier)]
#[non_exhaustive]
#[bits = 8]
pub enum ParameterTimePoint {
    Never = 0,
    Any = 1,
}

#[derive(Debug, PartialEq, num_derive::FromPrimitive, Clone, Copy, BitfieldSpecifier)]
#[non_exhaustive]
#[bits = 13]
pub enum ParameterTokenConfig {
    CcxMinSevAsid = 0x0101,

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

    Limit = 0x1FFF,
}
