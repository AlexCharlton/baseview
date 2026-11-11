//! Detect the host process so we can apply host-specific workarounds.
//!
//! Identification is based on the current process executable path (the DAW that
//! loaded this plugin), following the same practical approach used by JUCE's
//! `PluginHostType`.

use std::path::Path;
use std::sync::LazyLock;

/// Known plugin hosts that need special handling in baseview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Host {
    AbletonLive,
    BitwigStudio,
    Cubase,
    Reaper,
    FlStudio,
    Unknown,
}

/// How ignored keyboard messages should be forwarded to the parent window.
///
/// Empirically, on Windows:
/// - [`Self::Send`] works in Bitwig and Cubase
/// - [`Self::Post`] works in Ableton, Reaper, and Cubase
/// - neither works in FL Studio
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParentKeyMessageDelivery {
    /// Deliver synchronously with `SendMessageW`.
    Send,
    /// Queue asynchronously with `PostMessageW`.
    Post,
    /// No known working delivery method for this host.
    None,
}

static DETECTED_HOST: LazyLock<Host> = LazyLock::new(Host::detect);

impl Host {
    /// Returns the lazily detected host for this process.
    #[inline]
    pub fn current() -> Self {
        *DETECTED_HOST
    }

    fn detect() -> Self {
        match std::env::current_exe() {
            Ok(exe) => Self::from_exe_path(&exe),
            Err(_) => Self::Unknown,
        }
    }

    /// Classify a host from its executable path.
    ///
    /// Matching rules are intentionally conservative and aligned with common
    /// Windows executable names for each DAW.
    pub fn from_exe_path(path: &Path) -> Self {
        let path_lower = path.to_string_lossy().to_ascii_lowercase();
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        if path_lower.contains("bitwig") {
            return Self::BitwigStudio;
        }

        // REAPER: "reaper.exe"
        if file_name.contains("reaper") {
            return Self::Reaper;
        }

        // Cubase: "Cubase13.exe", "Cubase 12.exe", …
        if file_name.contains("cubase") {
            return Self::Cubase;
        }

        // Ableton: "Ableton Live 12 Suite.exe", "Live 11.exe", …
        if file_name.contains("live ") {
            return Self::AbletonLive;
        }

        // FL Studio: "FL64.exe", "FL.exe", plus the IL bridge process.
        if file_name.starts_with("fl") || file_name.contains("ilbridge.") {
            return Self::FlStudio;
        }

        Self::Unknown
    }

    /// Choose how to forward ignored keyboard messages to the parent HWND.
    pub fn parent_key_message_delivery(self) -> ParentKeyMessageDelivery {
        match self {
            // SendMessage works; Cubase also accepts PostMessage.
            Self::BitwigStudio | Self::Cubase => ParentKeyMessageDelivery::Send,
            // PostMessage works; SendMessage does not in these hosts.
            Self::AbletonLive | Self::Reaper => ParentKeyMessageDelivery::Post,
            // Neither SendMessage nor PostMessage reaches the host.
            Self::FlStudio => ParentKeyMessageDelivery::None,
            // Prefer PostMessage for unknowns: it covers more of the tested hosts
            // when Cubase (which accepts both) is set aside.
            Self::Unknown => ParentKeyMessageDelivery::Post,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn host(path: &str) -> Host {
        Host::from_exe_path(&PathBuf::from(path))
    }

    #[test]
    fn detects_ableton() {
        assert_eq!(
            host(r"C:\ProgramData\Ableton\Live 12 Suite\Program\Ableton Live 12 Suite.exe"),
            Host::AbletonLive
        );
        assert_eq!(host(r"C:\Ableton\Live 11.exe"), Host::AbletonLive);
    }

    #[test]
    fn detects_reaper() {
        assert_eq!(host(r"C:\Program Files\REAPER (x64)\reaper.exe"), Host::Reaper);
    }

    #[test]
    fn detects_cubase() {
        assert_eq!(host(r"C:\Program Files\Steinberg\Cubase 13\Cubase13.exe"), Host::Cubase);
        assert_eq!(host(r"C:\Program Files\Steinberg\Cubase 12\Cubase12.exe"), Host::Cubase);
    }

    #[test]
    fn detects_fl_studio() {
        assert_eq!(host(r"C:\Program Files\Image-Line\FL Studio 21\FL64.exe"), Host::FlStudio);
        assert_eq!(host(r"C:\Program Files\Image-Line\FL Studio 21\FL.exe"), Host::FlStudio);
        assert_eq!(host(r"C:\Program Files\Image-Line\FL Studio 21\ilbridge.exe"), Host::FlStudio);
    }

    #[test]
    fn detects_bitwig() {
        assert_eq!(
            host(r"C:\Program Files\Bitwig Studio\5.2.3\BitwigStudio.exe"),
            Host::BitwigStudio
        );
        assert_eq!(host(r"D:\BitwigStudio.exe"), Host::BitwigStudio);
    }

    #[test]
    fn unknown_for_unrecognized_hosts() {
        assert_eq!(host(r"C:\Program Files\SomeDaw\daw.exe"), Host::Unknown);
    }

    #[test]
    fn delivery_matches_known_host_matrix() {
        assert_eq!(
            Host::BitwigStudio.parent_key_message_delivery(),
            ParentKeyMessageDelivery::Send
        );
        assert_eq!(Host::Cubase.parent_key_message_delivery(), ParentKeyMessageDelivery::Send);
        assert_eq!(Host::AbletonLive.parent_key_message_delivery(), ParentKeyMessageDelivery::Post);
        assert_eq!(Host::Reaper.parent_key_message_delivery(), ParentKeyMessageDelivery::Post);
        assert_eq!(Host::FlStudio.parent_key_message_delivery(), ParentKeyMessageDelivery::None);
        assert_eq!(Host::Unknown.parent_key_message_delivery(), ParentKeyMessageDelivery::Post);
    }
}
