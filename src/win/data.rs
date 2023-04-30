// Functions for working with IDataObjects and event::Data

// Taken from https://github.com/rust-windowing/winit/blob/master/src/platform_impl/windows/drop_handler.rs
use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr};

use winapi::{
    shared::minwindef::UINT,
    um::{objidl::IDataObject, shellapi},
};

use crate::event::Data;

pub unsafe fn get_drop_data<F>(data_obj: *const IDataObject, callback: F) -> Option<shellapi::HDROP>
where
    F: FnMut(Data),
{
    iterate_filenames(data_obj, callback)
}
unsafe fn iterate_filenames<F>(
    data_obj: *const IDataObject, mut callback: F,
) -> Option<shellapi::HDROP>
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

        Some(hdrop)
    } else if get_data_result == DV_E_FORMATETC {
        // If the dropped item is not a file this error will occur.
        // In this case it is OK to return without taking further action.
        println!("Error occured while processing dropped/hovered item: item is not a file.");
        None
    } else {
        println!("Unexpected error occured while processing dropped/hovered item.");
        None
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
