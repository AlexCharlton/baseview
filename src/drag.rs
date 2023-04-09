#[cfg(target_os = "macos")]
use crate::macos as platform;
#[cfg(target_os = "windows")]
use crate::win as platform;
#[cfg(target_os = "linux")]
use crate::x11 as platform;

use crate::event::Data;

pub fn start_drag(data: Data) {
    platform::start_drag(data);
}
