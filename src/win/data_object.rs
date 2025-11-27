// https://github.com/superlistapp/super_native_extensions/blob/beabd4aca7f353a94f41b635aace9e625ca89aff/super_native_extensions/rust/src/win32/drag.rs
// used as a reference
// Perhaps https://github.com/DenisKolodin/nativeshell/blob/bc20a943be05eadb63a7d3323279d95328744e5c/nativeshell/src/shell/platform/win32/drag_context.rs#L83 may provide some ideas as well

use windows::{
    core::implement,
    Win32::{
        Foundation::{
            GlobalFree, DATA_S_SAMEFORMATETC, DV_E_FORMATETC, E_NOTIMPL, E_OUTOFMEMORY, HGLOBAL,
            OLE_E_ADVISENOTSUPPORTED, POINT, S_FALSE, S_OK,
        },
        System::{
            Com::{
                IDataObject, IDataObject_Impl, DATADIR_GET, FORMATETC, STGMEDIUM, STGMEDIUM_0,
                STREAM_SEEK_END, TYMED_HGLOBAL, TYMED_ISTREAM,
            },
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GLOBAL_ALLOC_FLAGS},
            Ole::CF_HDROP,
        },
        UI::Shell::{SHCreateMemStream, SHCreateStdEnumFmtEtc, DROPFILES, HDROP},
    },
};

use crate::event::Data;

#[derive(Debug, Clone)]
#[implement(IDataObject)]
pub struct DataObject {
    data: Data,
}

impl DataObject {
    pub fn create(data: Data) -> IDataObject {
        let data_object = Self { data };
        data_object.into()
    }

    fn global_from_data(data: &[u8]) -> windows::core::Result<HGLOBAL> {
        unsafe {
            let global =
                GlobalAlloc(GLOBAL_ALLOC_FLAGS(0), data.len() + std::mem::size_of::<HDROP>())?;
            let hdrop_ptr = GlobalLock(global);
            if hdrop_ptr.is_null() {
                GlobalFree(global)?;
                Err(E_OUTOFMEMORY.into())
            } else {
                std::ptr::copy_nonoverlapping(data.as_ptr(), hdrop_ptr as *mut u8, data.len());
                let _ = GlobalUnlock(global);
                Ok(global)
            }
        }
    }

    fn data_for_hdrop(path: &std::path::PathBuf) -> Vec<u8> {
        let mut res = Vec::new();

        let drop_files = DROPFILES {
            pFiles: std::mem::size_of::<DROPFILES>() as u32,
            pt: POINT { x: 0, y: 0 },
            fNC: false.into(),
            fWide: true.into(),
        };

        let drop_files = unsafe {
            ::std::slice::from_raw_parts(
                (&drop_files as *const DROPFILES) as *const u8,
                ::std::mem::size_of::<DROPFILES>(),
            )
        };
        res.extend_from_slice(drop_files);

        let mut file_str: Vec<u16> =
            path.clone().into_os_string().into_string().unwrap().encode_utf16().collect();
        // https://learn.microsoft.com/en-us/windows/win32/shell/clipboard#cf_hdrop
        file_str.push(0);
        file_str.push(0); // Double null terminated

        let data = unsafe {
            ::std::slice::from_raw_parts(
                file_str.as_ptr() as *const u8,
                file_str.len() * ::std::mem::size_of::<u16>(),
            )
        };
        res.extend_from_slice(&data[..]);

        res
    }
}

impl IDataObject_Impl for DataObject {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        match &self.data {
            Data::Filepath(p) => {
                let format = unsafe { &*pformatetcin };
                let data = Self::data_for_hdrop(p);

                if (format.tymed & TYMED_HGLOBAL.0 as u32) != 0 {
                    let global = Self::global_from_data(&data)?;
                    let s = STGMEDIUM {
                        tymed: TYMED_HGLOBAL.0 as u32,
                        u: STGMEDIUM_0 { hGlobal: global },
                        pUnkForRelease: std::mem::ManuallyDrop::new(None),
                    };

                    Ok(s)
                } else if (format.tymed & TYMED_ISTREAM.0 as u32) != 0 {
                    let stream = unsafe { SHCreateMemStream(Some(&data)) };
                    let stream =
                        stream.ok_or_else(|| windows::core::Error::from(DV_E_FORMATETC))?;
                    unsafe {
                        stream.Seek(0, STREAM_SEEK_END, None)?;
                    }
                    Ok(STGMEDIUM {
                        tymed: TYMED_ISTREAM.0 as u32,
                        u: STGMEDIUM_0 { pstm: std::mem::ManuallyDrop::new(Some(stream)) },
                        pUnkForRelease: std::mem::ManuallyDrop::new(None),
                    })
                } else {
                    Err(DV_E_FORMATETC.into())
                }
            }
            _ => Err(DV_E_FORMATETC.into()),
        }
    }

    fn GetDataHere(
        &self, _pformatetc: *const FORMATETC, _pmedium: *mut windows::Win32::System::Com::STGMEDIUM,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> windows::core::HRESULT {
        let format = unsafe { &*pformatetc };
        // Only supporting file drops
        if (format.tymed == TYMED_HGLOBAL.0 as u32 || format.tymed == TYMED_ISTREAM.0 as u32)
            && format.cfFormat == CF_HDROP.0
        {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn GetCanonicalFormatEtc(
        &self, pformatectin: *const FORMATETC, pformatetcout: *mut FORMATETC,
    ) -> ::windows::core::HRESULT {
        let fmt_out = unsafe { &mut *pformatetcout };
        let fmt_in = unsafe { &*pformatectin };
        *fmt_out = *fmt_in;
        DATA_S_SAMEFORMATETC
    }

    fn SetData(
        &self, _pformatetc: *const FORMATETC,
        _pmedium: *const windows::Win32::System::Com::STGMEDIUM,
        _frelease: windows::Win32::Foundation::BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(
        &self, dwdirection: u32,
    ) -> windows::core::Result<windows::Win32::System::Com::IEnumFORMATETC> {
        if dwdirection == DATADIR_GET.0 as u32 {
            unsafe {
                SHCreateStdEnumFmtEtc(&[
                    FORMATETC {
                        cfFormat: CF_HDROP.0,
                        ptd: std::ptr::null_mut(),
                        dwAspect: 1, // DVASPECT_CONTENT
                        lindex: -1,  // -1 = all items
                        tymed: TYMED_HGLOBAL.0 as u32,
                    },
                    FORMATETC {
                        cfFormat: CF_HDROP.0,
                        ptd: std::ptr::null_mut(),
                        dwAspect: 1, // DVASPECT_CONTENT
                        lindex: -1,  // -1 = all items
                        tymed: TYMED_ISTREAM.0 as u32,
                    },
                ])
            }
        } else {
            Err(E_NOTIMPL.into())
        }
    }

    fn DAdvise(
        &self, _pformatetc: *const windows::Win32::System::Com::FORMATETC, _advf: u32,
        _padvsink: core::option::Option<&windows::Win32::System::Com::IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _dwconnection: u32) -> windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<windows::Win32::System::Com::IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}
