use super::XcbConnection;
use crate::event::Data;
use xcb::{self, ffi, GenericError};

#[derive(Default)]
pub(crate) struct DragHandler {
    /// The data we're dragging
    data: Option<Data>,
    /// Are we dragging something right now?
    active: bool,
    /// Are we over a drag target that will accept the drop?
    accept: bool,
    /// Are we waiting for a XdndStatus message?
    waiting_for_status: bool,
    /// Have we deferred sending a XdndPosition message because we're waiting for a status?
    deferred_position_message: bool,
    /// What window are we over?
    target_window: Option<u32>,
    /// Where is our cursor?
    position: (u32, u32),
}

impl DragHandler {
    pub fn activate(&mut self, data: Data) {
        self.data = Some(data);
        self.active = true;
        self.accept = true;
        self.waiting_for_status = false;
        self.deferred_position_message = false;
        self.target_window = None;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn will_accept(&self) -> bool {
        self.accept
    }

    pub fn start(&self, conn: &XcbConnection, this_window: u32) {
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
        if Some(target_window) != self.target_window {
            // Enter window
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

        let x = event.root_x() as u32;
        let y = event.root_y() as u32;
        self.position = (x, y);
        if !self.waiting_for_status {
            self.waiting_for_status = true;
            self.deferred_position_message = false;
            conn.send_client_message(
                target_window,
                conn.atoms.dnd_position,
                [
                    this_window,
                    0,
                    (x << 16) | y,
                    99, // TODO set some actual time?
                    conn.atoms.dnd_action_copy,
                ],
            )
        } else {
            self.deferred_position_message = true;
            Ok(())
        }
    }

    pub fn handle_status(
        &mut self, status: &[u32], conn: &XcbConnection, this_window: u32,
    ) -> Result<(), GenericError> {
        self.accept = status[1] & 1 == 1;
        self.waiting_for_status = false;
        if self.deferred_position_message && self.target_window.is_some() {
            conn.send_client_message(
                self.target_window.unwrap(),
                conn.atoms.dnd_position,
                [
                    this_window,
                    0,
                    (self.position.0 << 16) | self.position.1,
                    99, // TODO set to some actual time?
                    conn.atoms.dnd_action_copy,
                ],
            )
        } else {
            Ok(())
        }
    }

    pub fn selection_requst(
        &mut self, event: &xcb::SelectionRequestEvent, conn: &XcbConnection,
    ) -> Result<(), GenericError> {
        match self.data.as_ref() {
            Some(Data::Filepath(p)) => unsafe {
                let property =
                    if event.property() == 0 { event.selection() } else { event.property() };
                let path = format!("file://{}", p.clone().into_os_string().into_string().unwrap());

                let cookie = ffi::xcb_change_property_checked(
                    conn.conn.get_raw_conn(),
                    ffi::XCB_PROP_MODE_REPLACE as _,
                    event.requestor(),
                    property,
                    conn.atoms.dnd_uri_list,
                    8,
                    path.len() as _,
                    path.as_ptr() as _,
                );
                xcb::base::VoidCookie { cookie, conn: &conn.conn, checked: true }
                    .request_check()?;

                let msg = ffi::xcb_selection_notify_event_t {
                    response_type: ffi::XCB_SELECTION_NOTIFY,
                    requestor: event.requestor(),
                    selection: event.selection(),
                    target: event.target(),
                    time: event.time(),
                    property,
                    sequence: 0,
                    pad0: 0,
                };
                let cookie = ffi::xcb_send_event_checked(
                    conn.conn.get_raw_conn(),
                    0,
                    event.requestor(),
                    0,
                    &msg as *const _ as _,
                );
                xcb::base::VoidCookie { cookie, conn: &conn.conn, checked: true }.request_check()
            },
            _ => Ok(()),
        }
    }

    pub fn do_drop(&mut self, conn: &XcbConnection, this_window: u32) -> Result<(), GenericError> {
        if !self.accept {
            return self.cancel(conn, this_window);
        }
        self.active = false;
        // We don't set self.data to None because we still need to handle the selection_request
        if let Some(target_window) = self.target_window {
            conn.send_client_message(
                target_window,
                conn.atoms.dnd_drop,
                [
                    this_window,
                    0,
                    99, // TODO set to some actual time?
                    0,
                    0,
                ],
            )
        } else {
            Ok(())
        }
    }

    pub fn cancel(&mut self, conn: &XcbConnection, this_window: u32) -> Result<(), GenericError> {
        self.active = false;
        self.data = None;
        if let Some(target_window) = self.target_window {
            conn.send_client_message(target_window, conn.atoms.dnd_leave, [this_window, 0, 0, 0, 0])
        } else {
            Ok(())
        }
    }
}
