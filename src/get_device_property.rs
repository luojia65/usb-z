use winapi::{
    shared::{minwindef::*, winerror::*},
    um::{errhandlingapi::*, setupapi::*},
};
use core::mem::{size_of, MaybeUninit};
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;

/*
bResult = GetDeviceProperty(DeviceList->DeviceInfo,
    &pNode->DeviceInfoData,
    SPDRP_DEVICEDESC,
    &pNode->DeviceDescName);
*/
pub fn get_device_property(
    device_info_set: HDEVINFO, // in
    device_info_data: PSP_DEVINFO_DATA, // in
    property: DWORD, // in
    buf: &mut Vec<u16>, // out
) -> io::Result<OsString> {
    // println!("[property] {}", property);
    let mut required_length_bytes = MaybeUninit::uninit();
    let success = unsafe { 
        SetupDiGetDeviceRegistryPropertyW(
            device_info_set,
            device_info_data,
            property,
            core::ptr::null_mut(),
            buf.as_mut_ptr() as *mut u8,
            (buf.capacity() * size_of::<u16>()) as u32,
            required_length_bytes.as_mut_ptr()
        )
    };
    // println!("[1] buf cap = {}, len = {}, req size = {}", buf.capacity(), buf.len(), unsafe { required_length_bytes.assume_init() });
    if success == FALSE && unsafe { GetLastError() } == ERROR_INSUFFICIENT_BUFFER {
        let new_len = unsafe { required_length_bytes.assume_init() } as usize / size_of::<u16>();
        let additional = new_len - buf.len();
        buf.reserve(additional);
        let success = unsafe { 
            SetupDiGetDeviceRegistryPropertyW(
                device_info_set,
                device_info_data,
                property,
                core::ptr::null_mut(),
                buf.as_mut_ptr() as *mut u8,
                (buf.capacity() * size_of::<u16>()) as u32,
                required_length_bytes.as_mut_ptr()
            )
        };
        if success == FALSE { 
            return Err(io::Error::last_os_error())
        }
        unsafe { buf.set_len(new_len - 1) } // remove \0;
        // println!("[2] buf cap = {}, len = {}, req size = {}", buf.capacity(), buf.len(), unsafe { required_length_bytes.assume_init() });
        Ok(OsString::from_wide(buf))
    } else if success == TRUE {
        let new_len = unsafe { required_length_bytes.assume_init() } as usize / size_of::<u16>();
        unsafe { buf.set_len(new_len - 1) } // remove \0;
        // println!("[3] buf cap = {}, len = {}, req size = {}", buf.capacity(), buf.len(), unsafe { required_length_bytes.assume_init() });
        Ok(OsString::from_wide(buf))
    } else {
        // panic!("[4] error {}", io::Error::last_os_error());
        Err(io::Error::last_os_error())
    }
}
