// https://github.com/superlistapp/super_native_extensions/blob/beabd4aca7f353a94f41b635aace9e625ca89aff/super_native_extensions/rust/src/win32/drag.rs
// used as a reference

use windows::{
    core::{implement, Interface, HRESULT, PCWSTR},
    Win32::{
        Foundation::{
            BOOL, COLORREF, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HWND,
            SIZE, S_OK,
        },
        System::{
            Ole::{
                DoDragDrop, IDropSource, IDropSource_Impl, DROPEFFECT, DROPEFFECT_COPY,
                DROPEFFECT_NONE,
            },
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
    },
};

use super::data_object::*;
use crate::event::Data;

pub fn start_drag(data: Data) {
    dbg!(data);

    let data_object = DataObject::create(); // TODO
    let drop_source = DropSource::create();
    let mut effects_out = DROPEFFECT_NONE;
    unsafe {
        let _ = DoDragDrop(
            &data_object,
            &drop_source,
            DROPEFFECT_COPY,
            &mut effects_out as *mut DROPEFFECT,
        );
    }
}

#[implement(IDropSource)]
pub struct DropSource {}

#[allow(non_snake_case)]
impl DropSource {
    pub fn create() -> IDropSource {
        Self {}.into()
    }
}

#[allow(non_snake_case)]
impl IDropSource_Impl for DropSource {
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fescapepressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if grfkeystate.0 & MK_LBUTTON.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}
