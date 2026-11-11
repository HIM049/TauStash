#[cfg(target_os = "windows")]
pub fn shift_pressed() -> Option<bool> {
    #[link(name = "user32")]
    unsafe extern "system" {
        safe fn GetAsyncKeyState(v_key: i32) -> i16;
    }
    const VK_SHIFT: i32 = 0x10;

    // The sign bit is set while the key is down
    Some(GetAsyncKeyState(VK_SHIFT) < 0)
}

#[cfg(target_os = "macos")]
pub fn shift_pressed() -> Option<bool> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        safe fn CGEventSourceFlagsState(state_id: i32) -> u64;
    }
    const COMBINED_SESSION_STATE: i32 = 0;
    const FLAG_MASK_SHIFT: u64 = 0x0002_0000;

    Some(CGEventSourceFlagsState(COMBINED_SESSION_STATE) & FLAG_MASK_SHIFT != 0)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn shift_pressed() -> Option<bool> {
    None
}
