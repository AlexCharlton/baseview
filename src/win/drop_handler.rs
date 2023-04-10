// Taken from https://github.com/rust-windowing/winit/blob/master/src/platform_impl/windows/drop_handler.rs
use std::sync::atomic::{AtomicUsize, Ordering};
use winapi::{
    ctypes::c_void,
    shared::{
        guiddef::REFIID,
        minwindef::{DWORD, ULONG},
        windef::{HWND, POINTL},
        winerror::S_OK,
    },
    um::{
        objidl::IDataObject,
        oleidl::{IDropTarget, IDropTargetVtbl, DROPEFFECT_COPY, DROPEFFECT_NONE},
        shellapi, unknwnbase,
        winnt::HRESULT,
    },
};

use super::data::*;
use crate::event::{Event, WindowEvent};

#[repr(C)]
pub struct DropHandlerData {
    pub interface: IDropTarget,
    refcount: AtomicUsize,
    window: HWND,
    send_event: Box<dyn Fn(Event)>,
    cursor_effect: DWORD,
    hovered_is_valid: bool, /* If the currently hovered item is not valid there must not be any `HoveredFileCancelled` emitted */
}

pub struct DropHandler {
    pub data: *mut DropHandlerData,
}

#[allow(non_snake_case)]
impl DropHandler {
    pub fn new(window: HWND, send_event: Box<dyn Fn(Event)>) -> DropHandler {
        let data = Box::new(DropHandlerData {
            interface: IDropTarget { lpVtbl: &DROP_TARGET_VTBL as *const IDropTargetVtbl },
            refcount: AtomicUsize::new(1),
            window,
            send_event,
            cursor_effect: DROPEFFECT_NONE,
            hovered_is_valid: false,
        });
        DropHandler { data: Box::into_raw(data) }
    }

    // Implement IUnknown
    pub unsafe extern "system" fn QueryInterface(
        _this: *mut unknwnbase::IUnknown, _riid: REFIID, _ppvObject: *mut *mut c_void,
    ) -> HRESULT {
        // This function doesn't appear to be required for an `IDropTarget`.
        unimplemented!();
    }

    pub unsafe extern "system" fn AddRef(this: *mut unknwnbase::IUnknown) -> ULONG {
        let drop_handler_data = Self::from_interface(this);
        let count = drop_handler_data.refcount.fetch_add(1, Ordering::Release) + 1;
        count as ULONG
    }

    pub unsafe extern "system" fn Release(this: *mut unknwnbase::IUnknown) -> ULONG {
        let drop_handler = Self::from_interface(this);
        let count = drop_handler.refcount.fetch_sub(1, Ordering::Release) - 1;
        if count == 0 {
            // Destroy the underlying data
            drop(Box::from_raw(drop_handler as *mut DropHandlerData));
        }
        count as ULONG
    }

    // Implement IDropTarget
    pub unsafe extern "system" fn DragEnter(
        this: *mut IDropTarget, pDataObj: *const IDataObject, _grfKeyState: DWORD,
        _pt: *const POINTL, pdwEffect: *mut DWORD,
    ) -> HRESULT {
        let drop_handler = Self::from_interface(this);
        let hdrop = get_drop_data(pDataObj, |_data| {});
        // TODO better is_valid logic
        drop_handler.hovered_is_valid = hdrop.is_some();
        if drop_handler.hovered_is_valid {
            drop_handler.send_event(Event::Window(WindowEvent::DragEnter));
        }
        drop_handler.cursor_effect =
            if drop_handler.hovered_is_valid { DROPEFFECT_COPY } else { DROPEFFECT_NONE };
        *pdwEffect = drop_handler.cursor_effect;

        S_OK
    }

    pub unsafe extern "system" fn DragOver(
        this: *mut IDropTarget, _grfKeyState: DWORD, _pt: *const POINTL, pdwEffect: *mut DWORD,
    ) -> HRESULT {
        let drop_handler = Self::from_interface(this);
        *pdwEffect = drop_handler.cursor_effect;
        if drop_handler.hovered_is_valid {
            drop_handler.send_event(Event::Window(WindowEvent::Dragging));
        }

        S_OK
    }

    pub unsafe extern "system" fn DragLeave(this: *mut IDropTarget) -> HRESULT {
        let drop_handler = Self::from_interface(this);
        if drop_handler.hovered_is_valid {
            drop_handler.send_event(Event::Window(WindowEvent::DragLeave));
        }

        S_OK
    }

    pub unsafe extern "system" fn Drop(
        this: *mut IDropTarget, pDataObj: *const IDataObject, _grfKeyState: DWORD,
        _pt: *const POINTL, _pdwEffect: *mut DWORD,
    ) -> HRESULT {
        let drop_handler = Self::from_interface(this);
        let hdrop = get_drop_data(pDataObj, |data| {
            drop_handler.send_event(Event::Window(WindowEvent::Drop(data)));
        });
        if let Some(hdrop) = hdrop {
            shellapi::DragFinish(hdrop);
        }

        S_OK
    }

    unsafe fn from_interface<'a, InterfaceT>(this: *mut InterfaceT) -> &'a mut DropHandlerData {
        &mut *(this as *mut _)
    }
}

impl DropHandlerData {
    fn send_event(&self, event: Event) {
        (self.send_event)(event);
    }
}

impl Drop for DropHandler {
    fn drop(&mut self) {
        unsafe {
            DropHandler::Release(self.data as *mut unknwnbase::IUnknown);
        }
    }
}

static DROP_TARGET_VTBL: IDropTargetVtbl = IDropTargetVtbl {
    parent: unknwnbase::IUnknownVtbl {
        QueryInterface: DropHandler::QueryInterface,
        AddRef: DropHandler::AddRef,
        Release: DropHandler::Release,
    },
    DragEnter: DropHandler::DragEnter,
    DragOver: DropHandler::DragOver,
    DragLeave: DropHandler::DragLeave,
    Drop: DropHandler::Drop,
};
