#[cfg(feature = "serde")]
const V3_CONFIG_STR: &str = r#"
{
        version: "0.1.0",
        header: {
                signature: "APCB",
                header_size: 0x0000,
                version: 48,
                unique_apcb_instance: 0x00000002,
        },
        v3_header_ext: {
                signature: "ECB2",
                _reserved_2: 0x0010,
                struct_version: 18,
                data_version: 256,
                ext_header_size: 0x00000060,
                _reserved_4: 0xffff,
                _reserved_5: 0x0040,
                data_offset: 0x0058,
                header_checksum: 0x00,
                integrity_sign: [
                        0x00,
                        0x42,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00
                ],
                signature_ending: "BCBA"
        },
        groups: [
        ],
        entries: [
        ]
}
"#;

#[cfg(feature = "serde")]
const INVALID_CONFIG_STR: &str = r#"
{
        version: "0.1.0",
        header: {
                signature: "APCB",
                header_size: 0x0000,
                version: 48,
                apcb_size: 0x00001274,
                unique_apcb_instance: 0x00000002,
                checksum_byte: 0x79
        },
        v3_headerquux: {
                signature: "ECB2",
                _reserved_2: 0x0010,
                struct_version: 18,
                data_version: 256,
                ext_header_size: 0x00000060,
                _reserved_4: 0xffff,
                _reserved_5: 0x0040,
                data_offset: 0x0058,
                header_checksum: 0x00,
                integrity_sign: [
                        0x00,
                        0x42,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00
                ],
                signature_ending: "BCBA"
        },
        groups: [
        ],
        entries: [
        ]
}
"#;

#[cfg(feature = "serde")]
const V2_CONFIG_STR: &str = r#"
{
        version: "0.1.0",
        header: {
                signature: "APCB",
                header_size: 0x0000,
                version: 48,
                unique_apcb_instance: 0x00000002,
        },
        groups: [
        ],
        entries: [
        ]
}
"#;

#[cfg(feature = "serde")]
#[test]
fn test_v3_header() {
    let configuration: amd_apcb::Apcb = serde_yaml::from_str(&V3_CONFIG_STR)
        .expect("configuration be valid JSON");
    let header = configuration.header().unwrap();
    assert_eq!(header.unique_apcb_instance().unwrap(), 2);
    assert_eq!(header.header_size.get(), 128);
    let v3_header_ext = configuration.v3_header_ext().unwrap().unwrap();
    assert_eq!(v3_header_ext.integrity_sign[1], 0x42);
}

#[cfg(feature = "serde")]
#[test]
fn test_v2_header() {
    let configuration: amd_apcb::Apcb = serde_yaml::from_str(&V2_CONFIG_STR)
        .expect("configuration be valid JSON");
    let header = configuration.header().unwrap();
    assert_eq!(header.header_size.get(), 32);
    assert_eq!(header.unique_apcb_instance().unwrap(), 2);
    assert!(configuration.v3_header_ext().unwrap().is_none());
}

#[cfg(feature = "serde")]
#[test]
fn test_unknown_field() {
    match serde_yaml::from_str::<amd_apcb::Apcb>(&INVALID_CONFIG_STR) {
        Ok(_) => {
            panic!("unexpected success");
        }
        Err(e) => {
            if e.to_string().contains("unknown field") {
                return;
            } else {
                panic!("unexpected error: {}", e.to_string());
            }
        }
    };
}

#[cfg(feature = "serde")]
fn take_header_from_collection_mut<
    'a,
    T: Sized + zerocopy::FromBytes + zerocopy::AsBytes,
>(
    buf: &mut &'a mut [u8],
) -> Option<&'a mut T> {
    use std::mem::take;
    use zerocopy::LayoutVerified;

    let xbuf = take(&mut *buf);
    match LayoutVerified::<_, T>::new_from_prefix(xbuf) {
        Some((item, xbuf)) => {
            *buf = xbuf;
            Some(item.into_mut())
        }
        None => None,
    }
}

#[cfg(feature = "serde")]
#[test]
fn test_skip_field() {
    use std::mem::size_of;

    use amd_apcb::memory::AblConsoleOutControl;
    // 3 is definitely not a valid value for a bool.
    let mut data = [3u8; size_of::<AblConsoleOutControl>()];
    let mut buf = &mut data[..];
    let header =
        take_header_from_collection_mut::<AblConsoleOutControl>(&mut buf)
            .unwrap();
    let s = serde_yaml::to_string(&header).unwrap();
    assert!(s.find("enable_console_logging").is_none());
    header.set_enable_console_logging(true);
    let s = serde_yaml::to_string(&header).unwrap();
    assert!(s.find("enable_console_logging").is_some());
}
