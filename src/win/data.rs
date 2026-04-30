// Functions for working with IDataObjects and event::Data

// Taken from https://github.com/rust-windowing/winit/blob/master/src/platform_impl/windows/drop_handler.rs
use std::{
    ffi::{OsStr, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
    ptr,
};

use winapi::{
    shared::{minwindef::UINT, wtypes::CLIPFORMAT},
    um::{
        objidl::{IDataObject, STGMEDIUM},
        shellapi,
    },
};

use crate::event::Data;

/// Result of reading a drag payload from an [`IDataObject`].
///
/// `hdrop` is present when the payload was a [`CF_HDROP`] file list; the caller must pass it to
/// [`shellapi::DragFinish`] after handling the drop. URL-only drags have `hdrop: None` but may
/// still set `had_payload` to true.
pub struct DropDataOutcome {
    pub hdrop: Option<shellapi::HDROP>,
    pub had_payload: bool,
}

pub unsafe fn get_drop_data<F>(data_obj: *const IDataObject, callback: F) -> DropDataOutcome
where
    F: FnMut(Data),
{
    iterate_filenames(data_obj, callback)
}

unsafe fn iterate_filenames<F>(data_obj: *const IDataObject, mut callback: F) -> DropDataOutcome
where
    F: FnMut(Data),
{
    use winapi::{
        shared::{
            winerror::{DV_E_FORMATETC, SUCCEEDED},
            wtypes::{CLIPFORMAT, DVASPECT_CONTENT},
        },
        um::{
            objidl::{FORMATETC, TYMED_HGLOBAL},
            shellapi::DragQueryFileW,
            winuser::CF_HDROP,
        },
    };

    let drop_format = FORMATETC {
        cfFormat: CF_HDROP as CLIPFORMAT,
        ptd: ptr::null(),
        dwAspect: DVASPECT_CONTENT,
        lindex: -1,
        tymed: TYMED_HGLOBAL,
    };

    let mut medium = std::mem::zeroed();
    let get_data_result = (*data_obj).GetData(&drop_format, &mut medium);
    if SUCCEEDED(get_data_result) {
        // This works for data dropped from windows explorer, but its hGlobal contains a pointer
        // That points to a
        // let hglobal = (*medium.u).hGlobal();
        // let hdrop = (*hglobal) as shellapi::HDROP;
        let hdrop = (medium.u) as shellapi::HDROP;

        // The second parameter (0xFFFFFFFF) instructs the function to return the item count
        let item_count = DragQueryFileW(hdrop, 0xFFFFFFFF, ptr::null_mut(), 0);

        if item_count > 0 {
            for i in 0..item_count {
                // Get the length of the path string NOT including the terminating null character.
                // Previously, this was using a fixed size array of MAX_PATH length, but the
                // Windows API allows longer paths under certain circumstances.
                let character_count = DragQueryFileW(hdrop, i, ptr::null_mut(), 0) as usize;
                let str_len = character_count + 1;

                // Fill path_buf with the null-terminated file name
                let mut path_buf = Vec::with_capacity(str_len);
                DragQueryFileW(hdrop, i, path_buf.as_mut_ptr(), str_len as UINT);
                path_buf.set_len(str_len);

                let buf = OsString::from_wide(&path_buf[0..character_count]).into();
                callback(Data::Filepath(buf));
            }

            return DropDataOutcome { hdrop: Some(hdrop), had_payload: true };
        }
    } else if get_data_result != DV_E_FORMATETC {
        println!("Unexpected error occured while processing dropped/hovered item.");
        return DropDataOutcome { hdrop: None, had_payload: false };
    }

    // Not a file list (or empty HDROP): try URL drag formats used by browsers and shell.
    if let Some(url) = try_get_uniform_resource_locator(data_obj) {
        callback(Data::String(url));
        return DropDataOutcome { hdrop: None, had_payload: true };
    }

    DropDataOutcome { hdrop: None, had_payload: false }
}

#[link(name = "ole32")]
extern "system" {
    fn ReleaseStgMedium(pmedium: *mut STGMEDIUM);
}

/// Reads `UniformResourceLocatorW` then `UniformResourceLocator` (ANSI) from the data object.
unsafe fn try_get_uniform_resource_locator(
    data_obj: *const IDataObject,
) -> Option<std::string::String> {
    use winapi::um::winuser::{RegisterClipboardFormatA, RegisterClipboardFormatW};

    let url_w: Vec<u16> =
        OsStr::new("UniformResourceLocatorW").encode_wide().chain(std::iter::once(0)).collect();
    let cf_w = RegisterClipboardFormatW(url_w.as_ptr());
    if cf_w != 0 {
        if let Some(s) = get_url_string_from_data_object(data_obj, cf_w as CLIPFORMAT) {
            return Some(s);
        }
    }

    let cf_a = RegisterClipboardFormatA(b"UniformResourceLocator\0".as_ptr() as *const i8);
    if cf_a != 0 {
        if let Some(s) = get_ansi_url_string_from_data_object(data_obj, cf_a as CLIPFORMAT) {
            return Some(s);
        }
    }

    None
}

unsafe fn get_url_string_from_data_object(
    data_obj: *const IDataObject, cf_format: CLIPFORMAT,
) -> Option<std::string::String> {
    use winapi::{
        shared::wtypes::DVASPECT_CONTENT,
        um::objidl::{FORMATETC, STGMEDIUM, TYMED_HGLOBAL},
    };

    let fmt = FORMATETC {
        cfFormat: cf_format,
        ptd: ptr::null(),
        dwAspect: DVASPECT_CONTENT,
        lindex: -1,
        tymed: TYMED_HGLOBAL,
    };
    let mut medium: STGMEDIUM = std::mem::zeroed();
    let hr = (*data_obj).GetData(&fmt, &mut medium);
    if !winapi::shared::winerror::SUCCEEDED(hr) {
        return None;
    }
    let out = read_utf16_null_terminated_from_medium(&mut medium);
    ReleaseStgMedium(&mut medium);
    out
}

unsafe fn get_ansi_url_string_from_data_object(
    data_obj: *const IDataObject, cf_format: CLIPFORMAT,
) -> Option<std::string::String> {
    use winapi::{
        shared::wtypes::DVASPECT_CONTENT,
        um::objidl::{FORMATETC, STGMEDIUM, TYMED_HGLOBAL},
    };

    let fmt = FORMATETC {
        cfFormat: cf_format,
        ptd: ptr::null(),
        dwAspect: DVASPECT_CONTENT,
        lindex: -1,
        tymed: TYMED_HGLOBAL,
    };
    let mut medium: STGMEDIUM = std::mem::zeroed();
    let hr = (*data_obj).GetData(&fmt, &mut medium);
    if !winapi::shared::winerror::SUCCEEDED(hr) {
        return None;
    }
    let out = read_ansi_c_string_from_medium(&mut medium);
    ReleaseStgMedium(&mut medium);
    out
}

unsafe fn read_utf16_null_terminated_from_medium(
    medium: &mut STGMEDIUM,
) -> Option<std::string::String> {
    use winapi::um::{
        objidl::TYMED_HGLOBAL,
        winbase::{GlobalLock, GlobalSize, GlobalUnlock},
    };

    if medium.tymed != TYMED_HGLOBAL || medium.u.is_null() {
        return None;
    }
    let hglobal = *(*medium.u).hGlobal();
    if hglobal.is_null() {
        return None;
    }
    let size = GlobalSize(hglobal) as usize;
    if size < 2 {
        return None;
    }
    let p = GlobalLock(hglobal);
    if p.is_null() {
        return None;
    }
    let wide = std::slice::from_raw_parts(p as *const u16, size / 2);
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    let s: std::string::String = std::char::decode_utf16(wide[..end].iter().copied())
        .map(|r| r.unwrap_or(std::char::REPLACEMENT_CHARACTER))
        .collect();
    let _ = GlobalUnlock(hglobal);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

unsafe fn read_ansi_c_string_from_medium(medium: &mut STGMEDIUM) -> Option<std::string::String> {
    use winapi::um::{
        objidl::TYMED_HGLOBAL,
        winbase::{GlobalLock, GlobalSize, GlobalUnlock},
    };

    if medium.tymed != TYMED_HGLOBAL || medium.u.is_null() {
        return None;
    }
    let hglobal = *(*medium.u).hGlobal();
    if hglobal.is_null() {
        return None;
    }
    let size = GlobalSize(hglobal) as usize;
    if size == 0 {
        return None;
    }
    let p = GlobalLock(hglobal);
    if p.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(p as *const u8, size);
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let s: std::string::String = String::from_utf8_lossy(&bytes[..end]).into_owned();
    let _ = GlobalUnlock(hglobal);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

// Debugging methods
// fn print_bytes<T>(value: &T) {
//     let value_bytes: &[u8] = unsafe {
//         std::slice::from_raw_parts(value as *const _ as *const u8, std::mem::size_of::<T>())
//     };

//     println!("{}", std::any::type_name::<T>());
//     for (i, b) in value_bytes.iter().enumerate() {
//         print!("{:02x} ", b); // print byte in hexadecimal with leading 0
//         if (i + 1) % 8 == 0 {
//             // print 8 bytes per line
//             println!();
//         }
//     }
//     println!();
// }

// pub fn print_su8(bytes: &[u8]) {
//     for b in bytes {
//         print!("{:02x} ", b);
//     }
//     println!();
// }
// pub fn print_su16(bytes: &[u16]) {
//     for b in bytes {
//         print!("{:02x} ", b);
//     }
//     println!();
// }
