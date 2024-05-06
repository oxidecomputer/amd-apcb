#[cfg(feature = "serde")]
#[test]
#[allow(non_snake_case)]
fn test_current_FchConsoleOutMode_Disabled() {
    let mode: amd_apcb::FchConsoleOutMode =
        serde_yaml::from_str("\"Disabled\"")
            .expect("configuration be valid JSON");
    assert_eq!(mode, amd_apcb::FchConsoleOutMode::Disabled);
}

#[cfg(feature = "serde")]
#[test]
#[allow(non_snake_case)]
fn test_current_FchConsoleOutMode_Enabled() {
    let mode: amd_apcb::FchConsoleOutMode = serde_yaml::from_str("\"Enabled\"")
        .expect("configuration be valid JSON");
    assert_eq!(mode, amd_apcb::FchConsoleOutMode::Enabled);
}

#[cfg(feature = "serde")]
#[test]
#[allow(non_snake_case)]
fn test_compat_FchConsoleOutMode_0() {
    let mode: amd_apcb::FchConsoleOutMode =
        serde_yaml::from_str("0").expect("configuration be valid JSON");
    assert_eq!(mode, amd_apcb::FchConsoleOutMode::Disabled);
}

#[cfg(feature = "serde")]
#[test]
#[allow(non_snake_case)]
fn test_compat_FchConsoleOutMode_1() {
    let mode: amd_apcb::FchConsoleOutMode =
        serde_yaml::from_str("1").expect("configuration be valid JSON");
    assert_eq!(mode, amd_apcb::FchConsoleOutMode::Enabled);
}

#[cfg(feature = "serde")]
#[test]
#[allow(non_snake_case)]
fn test_invalid_FchConsoleOutMode() {
    match serde_yaml::from_str::<amd_apcb::FchConsoleOutMode>("\"Disabledx\"") {
        Ok(_) => {
            panic!("unexpected success");
        }
        Err(_) => {}
    };
}

#[cfg(feature = "serde")]
#[test]
#[allow(non_snake_case)]
fn test_invalid_FchConsoleOutMode_5() {
    match serde_yaml::from_str::<amd_apcb::FchConsoleOutMode>("5") {
        Ok(_) => {
            panic!("unexpected success");
        }
        Err(_) => {}
    };
}
