// use std::{
//     ffi::OsString,
//     os::windows::ffi::OsStringExt,
//     path::PathBuf,
//     ptr,
//     sync::atomic::{AtomicUsize, Ordering},
// };

// use winapi::{
//     ctypes::c_void,
//     shared::{
//         guiddef::REFIID,
//         minwindef::{DWORD, UINT, ULONG},
//         windef::{HWND, POINTL},
//         winerror::S_OK,
//     },
//     um::{
//         objidl::IDataObject,
//         oleidl::{IDropTarget, IDropTargetVtbl, DROPEFFECT_COPY, DROPEFFECT_NONE},
//         shellapi, unknwnbase,
//         winnt::HRESULT,
//     },
// };

use crate::event::Data;

pub fn start_drag(data: Data) {
    dbg!(data);
    todo!();
    // TODO
}
