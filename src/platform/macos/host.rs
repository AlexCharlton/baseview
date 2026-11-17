//! Detect the host process so the Reaper keyboard workaround stays off other hosts.
//!
//! Identification uses the current process executable, as on Windows.

use std::path::Path;
use std::sync::LazyLock;

/// Known plugin hosts with macOS keyboard behavior that differs from AppKit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PluginHost {
    Reaper,
    Other,
}

static DETECTED_HOST: LazyLock<PluginHost> = LazyLock::new(PluginHost::detect);

impl PluginHost {
    /// Returns the lazily detected host for this process.
    #[inline]
    pub(crate) fn current() -> Self {
        *DETECTED_HOST
    }

    fn detect() -> Self {
        match std::env::current_exe() {
            Ok(exe) => Self::from_exe_path(&exe),
            Err(_) => Self::Other,
        }
    }

    /// Classify a host from its executable path.
    ///
    /// The macOS Reaper binary is `REAPER.app/Contents/MacOS/REAPER`.
    pub(crate) fn from_exe_path(path: &Path) -> Self {
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        if file_name.contains("reaper") {
            return Self::Reaper;
        }

        Self::Other
    }

    /// Reaper consumes keys inside its own `sendEvent:` before AppKit asks the view.
    ///
    /// Wrapping `sendEvent:` on other hosts stops ignored keys from reaching them.
    /// Those hosts already deliver through `performKeyEquivalent:` and `keyDown:`.
    pub(crate) fn needs_send_event_hook(self) -> bool {
        matches!(self, Self::Reaper)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn host(path: &str) -> PluginHost {
        PluginHost::from_exe_path(&PathBuf::from(path))
    }

    #[test]
    fn detects_reaper() {
        assert_eq!(host("/Applications/REAPER.app/Contents/MacOS/REAPER"), PluginHost::Reaper);
        assert_eq!(host("/Applications/REAPER64.app/Contents/MacOS/REAPER"), PluginHost::Reaper);
    }

    #[test]
    fn other_hosts_do_not_use_the_reaper_hook() {
        assert_eq!(
            host("/Applications/Bitwig Studio.app/Contents/MacOS/BitwigStudio"),
            PluginHost::Other
        );
        assert_eq!(host("/Applications/Cubase 14.app/Contents/MacOS/Cubase 14"), PluginHost::Other);
        assert_eq!(
            host("/Applications/FL Studio 2024.app/Contents/MacOS/FL Studio"),
            PluginHost::Other
        );
        assert!(!PluginHost::Other.needs_send_event_hook());
        assert!(PluginHost::Reaper.needs_send_event_hook());
    }
}
