// Adapted from https://github.com/rust-windowing/winit/blob/master/src/platform_impl/linux/x11/dnd.rs
use std::{
    io,
    path::{Path, PathBuf},
    str::Utf8Error,
};

use percent_encoding::percent_decode;
use xcb::{Atom, GenericError};
use xcb_util::ewmh::send_client_message;

use super::XcbConnection;

#[derive(Debug, Clone, Copy)]
pub enum DndState {
    Accepted,
    Rejected,
}

#[derive(Debug)]
pub enum DndDataParseError {
    EmptyData,
    InvalidUtf8(Utf8Error),
    HostnameSpecified(String),
    UnexpectedProtocol(String),
    UnresolvablePath(io::Error),
}

impl From<Utf8Error> for DndDataParseError {
    fn from(e: Utf8Error) -> Self {
        DndDataParseError::InvalidUtf8(e)
    }
}

impl From<io::Error> for DndDataParseError {
    fn from(e: io::Error) -> Self {
        DndDataParseError::UnresolvablePath(e)
    }
}

#[derive(Default)]
pub(crate) struct DropHandler {
    pub drop_target_valid: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    // Populated by XdndEnter event handler
    pub version: Option<u32>,
    pub type_list: Option<Vec<u32>>,
    // Populated by XdndPosition event handler
    pub source_window: Option<u32>,
    // Populated by SelectionNotify event handler (triggered by XdndPosition event handler)
    pub result: Option<Result<Vec<PathBuf>, DndDataParseError>>,
}

impl std::fmt::Debug for DropHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DropHandler")
            .field("version", &self.version)
            .field("type_list", &self.type_list)
            .field("source_window", &self.source_window)
            .field("result", &self.result)
            .finish()
    }
}

impl DropHandler {
    pub fn reset(&mut self) {
        self.version = None;
        self.type_list = None;
        self.source_window = None;
        self.result = None;
    }

    pub fn send_status(
        &self, conn: &XcbConnection, this_window: u32, target_window: u32, state: DndState,
    ) -> Result<(), GenericError> {
        let (accepted, action) = match state {
            DndState::Accepted => (1, conn.atoms.dnd_action_private),
            DndState::Rejected => (0, xcb::ATOM_NONE),
        };
        send_client_message(
            &conn.conn,
            target_window,
            target_window,
            conn.atoms.dnd_status,
            &[this_window, accepted, 0, 0, action.into()],
        )
        .request_check()
    }

    pub fn send_finished(
        &self, conn: &XcbConnection, this_window: u32, target_window: u32, state: DndState,
    ) -> Result<(), GenericError> {
        let (accepted, action) = match state {
            DndState::Accepted => (1, conn.atoms.dnd_action_private),
            DndState::Rejected => (0, xcb::ATOM_NONE),
        };
        send_client_message(
            &conn.conn,
            target_window,
            target_window,
            conn.atoms.dnd_finished,
            &[this_window, accepted, action, 0, 0],
        )
        .request_check()
    }

    pub fn get_type_list(
        &self, conn: &XcbConnection, source_window: u32,
    ) -> Result<Vec<Atom>, GenericError> {
        xcb::get_property(
            &conn.conn,
            false,
            source_window,
            conn.atoms.dnd_type_list,
            xcb::ATOM_ATOM,
            0,
            0,
        )
        .get_reply()
        .map(|r| r.value::<Atom>().to_vec())
    }

    pub fn convert_selection(&self, conn: &XcbConnection, window: u32, time: u32) {
        xcb::convert_selection(
            &conn.conn,
            window,
            conn.atoms.dnd_selection,
            conn.atoms.dnd_uri_list,
            conn.atoms.dnd_selection,
            time,
        );
    }

    pub fn read_data(&self, conn: &XcbConnection, window: u32) -> Result<Vec<u8>, GenericError> {
        xcb::get_property(
            &conn.conn,
            false,
            window,
            conn.atoms.dnd_selection,
            conn.atoms.dnd_uri_list,
            0,
            0,
        )
        .get_reply()
        .map(|r| r.value::<u8>().to_vec())
    }

    pub fn parse_data(&self, data: &mut [u8]) -> Result<Vec<PathBuf>, DndDataParseError> {
        if !data.is_empty() {
            let mut path_list = Vec::new();
            let decoded = percent_decode(data).decode_utf8()?.into_owned();
            for uri in decoded.split("\r\n").filter(|u| !u.is_empty()) {
                // The format is specified as protocol://host/path
                // However, it's typically simply protocol:///path
                let path_str = if uri.starts_with("file://") {
                    let path_str = uri.replace("file://", "");
                    if !path_str.starts_with('/') {
                        // A hostname is specified
                        // Supporting this case is beyond the scope of my mental health
                        return Err(DndDataParseError::HostnameSpecified(path_str));
                    }
                    path_str
                } else {
                    // Only the file protocol is supported
                    return Err(DndDataParseError::UnexpectedProtocol(uri.to_owned()));
                };

                let path = Path::new(&path_str).canonicalize()?;
                path_list.push(path);
            }
            Ok(path_list)
        } else {
            Err(DndDataParseError::EmptyData)
        }
    }
}
