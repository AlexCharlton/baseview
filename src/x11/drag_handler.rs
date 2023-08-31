use super::XcbConnection;
use crate::event::Data;
use xcb::{self, ffi, GenericError};

pub(crate) struct DragHandler {
    data: Data,
    target_window: Option<u32>,
}

impl DragHandler {
    pub fn new(data: Data) -> Self {
        Self { data, target_window: None }
    }

    pub fn start(&self, conn: &XcbConnection, this_window: u32) {
        dbg!("start drag");
        xcb::set_selection_owner_checked(&conn.conn, this_window, conn.atoms.dnd_selection, 0);
        xcb::change_property_checked(
            &conn.conn,
            ffi::XCB_PROP_MODE_REPLACE as u8,
            this_window,
            conn.atoms.dnd_type_list,
            ffi::XCB_ATOM_ATOM,
            32,
            &[conn.atoms.dnd_uri_list],
        );
    }

    pub fn motion(
        &mut self, event: &xcb::MotionNotifyEvent, conn: &XcbConnection, this_window: u32,
    ) -> Result<(), GenericError> {
        let setup = conn.conn.get_setup();
        let screen = setup.roots().nth(conn.xlib_display as usize).unwrap();

        let mut target_window = screen.root();
        let abs_x = event.root_x() as i16;
        let abs_y = event.root_y() as i16;
        loop {
            // Find the (target) window under the cursor
            let r =
                xcb::translate_coordinates(&conn.conn, screen.root(), target_window, abs_x, abs_y)
                    .get_reply();
            if let Ok(r) = r {
                if r.child() == 0 {
                    break;
                } else {
                    target_window = r.child()
                }
            }
        }
        println!(
            "Motion: Window at ({abs_x},{abs_y}) is {target_window}. This is {this_window} ({this_window:x})",
        );
        if Some(target_window) != self.target_window {
            self.target_window = Some(target_window);
            conn.send_client_message(
                target_window,
                conn.atoms.dnd_enter,
                [
                    this_window,
                    (5 << 24) // Version
                    | 0, // All types supported listed in the rest of this data (no need to fetch more types)
                    conn.atoms.dnd_uri_list,
                    0,
                    0,
                ],
            )?;
        }

        // Relative position TODO?
        let x = event.event_x() as u32;
        let y = event.event_y() as u32;
        conn.send_client_message(
            target_window,
            conn.atoms.dnd_position,
            [
                this_window,
                0,
                (x << 16) | y,
                99, // TODO set to time?
                conn.atoms.dnd_action_copy,
            ],
        )
    }

    pub fn drop(&self, conn: &XcbConnection, this_window: u32) -> Result<(), GenericError> {
        // TODO
        dbg!("drop");
        // No need to clean up anything, this object is about to be dropped
        Ok(())
    }

    pub fn cancel(&self, conn: &XcbConnection, this_window: u32) -> Result<(), GenericError> {
        // TODO
        dbg!("cancel");
        // No need to clean up anything, this object is about to be dropped
        Ok(())
    }
}
