use windows::{
    core::{implement, HRESULT, HSTRING},
    Win32::{
        Foundation::{
            BOOL, DATA_S_SAMEFORMATETC, DV_E_FORMATETC, E_NOTIMPL, E_OUTOFMEMORY,
            OLE_E_ADVISENOTSUPPORTED, S_FALSE, S_OK,
        },
        System::{
            Com::{
                IBindCtx, IDataObject, IDataObject_Impl, IStream, DATADIR_GET, FORMATETC,
                STGMEDIUM, STGMEDIUM_0, STREAM_SEEK_END, STREAM_SEEK_SET, TYMED, TYMED_HGLOBAL,
                TYMED_ISTREAM,
            },
            // Memory::{
            //     GlobalAlloc, GlobalFree, GlobalLock, GlobalSize, GlobalUnlock, GLOBAL_ALLOC_FLAGS,
            // },
            Ole::{ReleaseStgMedium, CF_DIB, CF_DIBV5, CF_HDROP, DROPEFFECT},
        },
    },
};

#[implement(IDataObject)]
pub struct DataObject {}

impl DataObject {
    pub fn create() -> IDataObject {
        let data_object = Self {};
        data_object.into()
    }
}

impl IDataObject_Impl for DataObject {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        //     let format = unsafe { &*pformatetcin };
        //     let format_file_descriptor = unsafe { RegisterClipboardFormatW(CFSTR_FILEDESCRIPTOR) };
        //     let format_file_contents = unsafe { RegisterClipboardFormatW(CFSTR_FILECONTENTS) };

        //     if format.cfFormat as u32 == format_file_contents {
        //         let stream = self
        //             .stream_for_virtual_file_index(format.lindex as usize, Self::is_local_request());
        //         return Ok(STGMEDIUM {
        //             tymed: TYMED_ISTREAM,
        //             Anonymous: STGMEDIUM_0 { pstm: ManuallyDrop::new(stream) },
        //             pUnkForRelease: windows::core::ManuallyDrop::none(),
        //         });
        //     }

        //     let needs_generate_bitmap = self.needs_synthetize_bitmap();

        //     let data = self.extra_data.borrow().get(&format.cfFormat).cloned().or_else(|| {
        //         if format.cfFormat as u32 == format_file_descriptor {
        //             self.data_for_file_group_descritor()
        //         } else if format.cfFormat == CF_HDROP.0 {
        //             self.data_for_hdrop()
        //         } else if needs_generate_bitmap && format.cfFormat == CF_DIB.0 {
        //             self.synthetize_bitmap_data(false).ok_log()
        //         } else if needs_generate_bitmap && format.cfFormat == CF_DIBV5.0 {
        //             self.synthetize_bitmap_data(true).ok_log()
        //         } else {
        //             self.data_for_format(format.cfFormat as u32, 0)
        //         }
        //     });

        //     // println!("DATA {:?} {:?}", data, format_to_string(format.cfFormat as u32));

        //     match data {
        //         Some(data) => {
        //             if (format.tymed & TYMED_HGLOBAL.0 as u32) != 0 {
        //                 let global = self.global_from_data(&data)?;
        //                 Ok(STGMEDIUM {
        //                     tymed: TYMED_HGLOBAL,
        //                     Anonymous: STGMEDIUM_0 { hGlobal: global },
        //                     pUnkForRelease: windows::core::ManuallyDrop::none(),
        //                 })
        //             } else if (format.tymed & TYMED_ISTREAM.0 as u32) != 0 {
        //                 let stream = unsafe { SHCreateMemStream(Some(&data)) };
        //                 let stream =
        //                     stream.ok_or_else(|| windows::core::Error::from(DV_E_FORMATETC))?;
        //                 unsafe {
        //                     stream.Seek(0, STREAM_SEEK_END, None)?;
        //                 }
        //                 Ok(STGMEDIUM {
        //                     tymed: TYMED_ISTREAM,
        //                     Anonymous: STGMEDIUM_0 { pstm: ManuallyDrop::new(Some(stream)) },
        //                     pUnkForRelease: windows::core::ManuallyDrop::none(),
        //                 })
        //             } else {
        //                 Err(DV_E_FORMATETC.into())
        //             }
        //         }
        //         None => Err(DV_E_FORMATETC.into()),
        //     }
        Err(DV_E_FORMATETC.into())
    }

    fn GetDataHere(
        &self, _pformatetc: *const windows::Win32::System::Com::FORMATETC,
        _pmedium: *mut windows::Win32::System::Com::STGMEDIUM,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(
        &self, pformatetc: *const windows::Win32::System::Com::FORMATETC,
    ) -> windows::core::HRESULT {
        // let format = unsafe { &*pformatetc };
        // let index = self.get_formats().iter().position(|e| {
        //     e.cfFormat == format.cfFormat
        //         && (e.tymed & format.tymed) != 0
        //         && e.dwAspect == format.dwAspect
        //         && e.lindex == format.lindex
        // });
        // match index {
        //     Some(_) => S_OK,
        //     None => {
        //         // possibly extra data
        //         if (format.tymed == TYMED_HGLOBAL.0 as u32
        //             || format.tymed == TYMED_ISTREAM.0 as u32)
        //             && self.extra_data.borrow().contains_key(&format.cfFormat)
        //         {
        //             S_OK
        //         } else {
        //             S_FALSE
        //         }
        //     }
        // }
        S_FALSE
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
        &self, pformatetc: *const windows::Win32::System::Com::FORMATETC,
        pmedium: *const windows::Win32::System::Com::STGMEDIUM,
        frelease: windows::Win32::Foundation::BOOL,
    ) -> windows::core::Result<()> {
        let format = unsafe { &*pformatetc };

        if format.tymed == TYMED_HGLOBAL.0 as u32 {
            // unsafe {
            //     let medium = &*pmedium;
            //     let size = GlobalSize(medium.Anonymous.hGlobal);
            //     let global_data = GlobalLock(medium.Anonymous.hGlobal);

            //     let v = slice::from_raw_parts(global_data as *const u8, size);
            //     let global_data: Vec<u8> = v.into();

            //     GlobalUnlock(medium.Anonymous.hGlobal);
            //     self.extra_data.borrow_mut().insert(format.cfFormat, global_data);

            //     if frelease.as_bool() {
            //         ReleaseStgMedium(pmedium as *mut _);
            //     }
            // }

            Ok(())
        } else if format.tymed == TYMED_ISTREAM.0 as u32 {
            // unsafe {
            //     let medium = &*pmedium;
            //     let stream = medium.Anonymous.pstm.as_ref().cloned();

            //     let stream_data = if let Some(stream) = stream {
            //         stream.Seek(0, STREAM_SEEK_SET, None)?;
            //         read_stream_fully(stream)
            //     } else {
            //         Vec::new()
            //     };

            //     self.extra_data.borrow_mut().insert(format.cfFormat, stream_data);

            //     if frelease.as_bool() {
            //         ReleaseStgMedium(pmedium as *mut _);
            //     }
            // }

            Ok(())
        } else {
            Err(DV_E_FORMATETC.into())
        }
    }

    fn EnumFormatEtc(
        &self, dwdirection: u32,
    ) -> windows::core::Result<windows::Win32::System::Com::IEnumFORMATETC> {
        if dwdirection == DATADIR_GET.0 as u32 {
            //unsafe { SHCreateStdEnumFmtEtc(&self.get_formats()) }
            Err(E_NOTIMPL.into())
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
