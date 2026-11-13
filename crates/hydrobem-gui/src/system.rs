//! What the operating system reports.

/// The physical memory available now, in bytes (0 when unknown).
#[allow(unsafe_code)] // one call of the Windows API with a struct it fills
pub fn available_memory() -> u64 {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: the struct is initialized with its size, as the function requires.
    if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 { status.ullAvailPhys } else { 0 }
}

/// The mouse cursor on the screen, physical pixels. While a file is dragged over the window from
/// another program the window receives no mouse moves, so the drop targets ask the system.
#[allow(unsafe_code)] // one call of the Windows API with a struct it fills
pub fn cursor_position() -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: the pointer is to a POINT the function fills.
    (unsafe { GetCursorPos(&mut p) } != 0).then_some((p.x, p.y))
}
