use winapi::{
    shared::{usbiodef::*, minwindef::*, winerror::*},
    um::{
        errhandlingapi::*, setupapi::*, fileapi::*, winnt::*, handleapi::*,
    },
};
use core::mem::{size_of, MaybeUninit};
use core::fmt;
use std::io;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

pub struct HostController {
    path: OsString, // path
    h_hc_dev: HANDLE, // host controller handle
}

impl HostController {
    // create a host controller from raw windows handle
    unsafe fn open_path(detail_data_buf: &[u16]) -> io::Result<Self> {
        let device_detail_data = detail_data_buf.as_ptr() 
            as *const SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        let h_hc_dev =
            CreateFileW(
                &(&*device_detail_data).DevicePath as LPCWSTR,
                GENERIC_WRITE,
                FILE_SHARE_WRITE,
                core::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                core::ptr::null_mut(),
            );
        if h_hc_dev == INVALID_HANDLE_VALUE {
            // println!("CreateFileW Error!");
            return Err(io::Error::last_os_error())
        }
        let offset = size_of::<DWORD>() / size_of::<u16>();
        let tail = detail_data_buf.len() - 1; // remove \0
        let path = OsString::from_wide(&detail_data_buf[offset..tail]);
        Ok(Self { path, h_hc_dev })
    }

    // get usb hcd driver key name
    pub fn driver_key(&self) -> io::Result<DriverKey> {
        #![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
        use winapi::um::ioapiset::DeviceIoControl;
        const IOCTL_GET_HCD_DRIVERKEY_NAME: DWORD = 0x220424;
        winapi::STRUCT!{struct USB_HCD_DRIVERKEY_NAME {
            ActualLength: ULONG,
            DriverKeyName: [WCHAR; 1],
        }}
        let mut required_length_bytes = MaybeUninit::uninit();
        let mut driver_key_name = MaybeUninit::<USB_HCD_DRIVERKEY_NAME>::uninit();
        let success = unsafe { 
            DeviceIoControl(
                self.h_hc_dev,
                IOCTL_GET_HCD_DRIVERKEY_NAME,
                core::ptr::null_mut(), // input buffer
                0, // input buffer
                driver_key_name.as_mut_ptr() as LPVOID,
                size_of::<USB_HCD_DRIVERKEY_NAME>() as DWORD,
                required_length_bytes.as_mut_ptr(),
                core::ptr::null_mut()
            )
        };
        if success == FALSE {
            return Err(io::Error::last_os_error())
        }
        let new_len = unsafe { driver_key_name.assume_init_ref() }.ActualLength as usize / size_of::<u16>();
        // println!("new-len = {}", new_len); // 47
        // println!("returned-bytes = {}", unsafe { *required_length_bytes.as_ptr() }); // 6
        let mut buf: Vec<u16> = Vec::new();
        buf.reserve(new_len);
        let success = unsafe { 
            DeviceIoControl(
                self.h_hc_dev,
                IOCTL_GET_HCD_DRIVERKEY_NAME,
                core::ptr::null_mut(), // input buffer
                0, // input buffer
                buf.as_mut_ptr() as LPVOID,
                (buf.capacity() * size_of::<u16>()) as u32,
                required_length_bytes.as_mut_ptr(),
                core::ptr::null_mut()
            )
        };
        if success == FALSE { 
            return Err(io::Error::last_os_error())
        }
        unsafe { buf.set_len(new_len) };
        let string = unsafe {
            core::slice::from_raw_parts(
                &(*(buf.as_ptr() as *const USB_HCD_DRIVERKEY_NAME)).DriverKeyName as *const u16,
                new_len - size_of::<ULONG>(),
            )
        };
        Ok(DriverKey {
            name: OsString::from_wide(string)
        })
    }
}

impl fmt::Debug for HostController {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.path.to_string_lossy())
    }
}

impl Drop for HostController {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.h_hc_dev) };
    }
}

// windows host controller enumerator
#[derive(Debug)]
pub struct HostControllers {
    device_info_set: HDEVINFO,
    member_index: DWORD,
    detail_data_buf: Vec<u16>,
}

// create an enumerator for windows usb host controllers
pub fn host_controllers() -> io::Result<HostControllers> {
    let device_info_set = unsafe {
        SetupDiGetClassDevsW(
            &GUID_CLASS_USB_HOST_CONTROLLER as *const _,
            core::ptr::null(),
            core::ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if device_info_set == INVALID_HANDLE_VALUE {
        // println!("SetupDiGetClassDevsW Error!");
        return Err(io::Error::last_os_error())
    }
    // remarks: The caller of SetupDiGetClassDevs must delete the returned device information set 
    // when it is no longer needed by calling SetupDiDestroyDeviceInfoList. (msdn docs)
    Ok(HostControllers {
        device_info_set,
        member_index: 0,
        detail_data_buf: vec![0u16; 4], // must be enough to put cbSize in
    })
}

impl Iterator for HostControllers {
    type Item = io::Result<HostController>;
    fn next(&mut self) -> Option<Self::Item> {
        // The SetupDiEnumDeviceInfo function returns a SP_DEVINFO_DATA structure 
        // that specifies a device information element in a device information set. (msdn)
        let mut device_info_data = MaybeUninit::<SP_DEVINFO_DATA>::uninit();
        unsafe { device_info_data.assume_init_mut() }.cbSize = size_of::<SP_DEVINFO_DATA>() as DWORD;
        let success = unsafe {
            SetupDiEnumDeviceInfo(
                self.device_info_set, // in
                self.member_index, // in
                device_info_data.as_mut_ptr() // out
            )
        };
        if success == FALSE {
            if unsafe { GetLastError() } == ERROR_NO_MORE_ITEMS {
                return None
            } else {
                return Some(Err(io::Error::last_os_error()))
            }
        } // host controller enumeration has finished
        let mut device_interface_data = MaybeUninit::<SP_DEVICE_INTERFACE_DATA>::uninit();
        unsafe { device_interface_data.assume_init_mut() }.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as DWORD;
        let success = unsafe {
            SetupDiEnumDeviceInterfaces(
                self.device_info_set,
                core::ptr::null_mut(),
                &GUID_CLASS_USB_HOST_CONTROLLER as *const _,
                self.member_index,
                device_interface_data.as_mut_ptr()
            )
        };
        if success == FALSE {
            // println!("SetupDiEnumDeviceInterfaces Error!");
            return Some(Err(io::Error::last_os_error()))
        }
        if self.detail_data_buf.capacity() < size_of::<DWORD>() / size_of::<u16>() {
            self.detail_data_buf.reserve(size_of::<DWORD>() / size_of::<u16>()); // DWORD - len() is enough
        }
        unsafe { &mut *(self.detail_data_buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W)}.cbSize = 
            size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as DWORD;
        let mut required_size_bytes = MaybeUninit::uninit();
        let success = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                self.device_info_set,
                device_interface_data.as_mut_ptr(),
                self.detail_data_buf.as_mut_ptr() as *mut _,
                (self.detail_data_buf.capacity() * size_of::<u16>()) as DWORD,
                required_size_bytes.as_mut_ptr(),
                core::ptr::null_mut()
            )
        };
        if success == FALSE && unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return Some(Err(io::Error::last_os_error()))
        }
        let new_len = unsafe { required_size_bytes.assume_init() } as usize / size_of::<u16>();
        // println!("len = {}", new_len);
        if success == FALSE { // && unsafe { GetLastError() } == ERROR_INSUFFICIENT_BUFFER 
            let additional = new_len - self.detail_data_buf.len();
            self.detail_data_buf.reserve(additional);
            let success = unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    self.device_info_set,
                    device_interface_data.as_mut_ptr(),
                    self.detail_data_buf.as_mut_ptr() as *mut _,
                    (self.detail_data_buf.capacity() * size_of::<u16>()) as DWORD,
                    required_size_bytes.as_mut_ptr(),
                    core::ptr::null_mut()
                )
            };
            if success == FALSE { 
                return Some(Err(io::Error::last_os_error()))
            }
        } 
        unsafe { self.detail_data_buf.set_len(new_len) };
        self.member_index += 1; // next host controller
        Some(unsafe { HostController::open_path(&self.detail_data_buf) })
    }
}

impl Drop for HostControllers {
    fn drop(&mut self) {
        unsafe { SetupDiDestroyDeviceInfoList(self.device_info_set) };
    }
}

pub struct DriverKey {
    name: OsString,
}

impl DriverKey {
    
}

impl fmt::Debug for DriverKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.name.to_string_lossy())
    }
}
