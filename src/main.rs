#![feature(maybe_uninit_ref, new_uninit)]
use winapi::{
    shared::{guiddef::GUID, usbiodef::*, minwindef::*, winerror::*},
    um::{
        errhandlingapi::*, setupapi::*, fileapi::*, winnt::*, handleapi::*, heapapi::*,
        ioapiset::*,
    },
};
use core::{
    mem::{size_of, MaybeUninit},
    ptr::NonNull
};

mod api {
    #![allow(non_snake_case, non_camel_case_types)]
    use winapi::{shared::minwindef::*, um::winnt::*};
    pub const IOCTL_GET_HCD_DRIVERKEY_NAME: DWORD = 0x220424;
    winapi::STRUCT!{struct USB_HCD_DRIVERKEY_NAME {
        ActualLength: ULONG,
        DriverKeyName: [WCHAR; 1],
    }}
    //pub type PUSB_HCD_DRIVERKEY_NAME = *mut USB_HCD_DRIVERKEY_NAME;
}
use api::*;

fn enumerate_host_controllers() {
    let (devices, hubs) = enumerate_all_devices();

    let device_info = unsafe {
        SetupDiGetClassDevsW(
            &GUID_CLASS_USB_HOST_CONTROLLER as *const _,
            core::ptr::null(),
            core::ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if device_info == INVALID_HANDLE_VALUE {
        println!("SetupDiGetClassDevsW Error!");
    }
    let mut device_info_data = MaybeUninit::<SP_DEVINFO_DATA>::uninit();
    unsafe { device_info_data.get_mut() }.cbSize = size_of::<SP_DEVINFO_DATA>() as DWORD;
    let mut device_interface_data = MaybeUninit::<SP_DEVICE_INTERFACE_DATA>::uninit();
    unsafe { device_interface_data.get_mut() }.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as DWORD;
    let mut index = 0;
    while unsafe { 
        SetupDiEnumDeviceInfo(
            device_info,
            index,
            device_info_data.as_mut_ptr()
        ) == TRUE
    } {
        println!("├ Host Controller Index: {}", index); 
        let success = unsafe {
            SetupDiEnumDeviceInterfaces(
                device_info,
                core::ptr::null_mut(),
                &GUID_CLASS_USB_HOST_CONTROLLER as *const _,
                index,
                device_interface_data.as_mut_ptr()
            )
        };
        if success == FALSE {
            println!("SetupDiEnumDeviceInterfaces Error!");
            continue;
        }
        let mut required_length = MaybeUninit::uninit();
        let success = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                device_info,
                device_interface_data.as_mut_ptr(),
                core::ptr::null_mut(),
                0,
                required_length.as_mut_ptr(),
                core::ptr::null_mut()
            )
        };
        if success == FALSE && unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            println!("SetupDiGetDeviceInterfaceDetailW Error[1]!");
            continue;
        }
        let heap_handle = unsafe { GetProcessHeap() };
        let device_detail_data = NonNull::new(unsafe { 
            HeapAlloc(heap_handle, 0, required_length.assume_init() as usize) as *mut _ 
        });
        let mut device_detail_data = if let Some(device_detail_data) = device_detail_data { 
            device_detail_data.cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>()
        } else {
            println!("Error HeapAlloc");
            continue;
        };
        unsafe { device_detail_data.as_mut() }.cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as DWORD;
        let success = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                device_info,
                device_interface_data.as_mut_ptr(),
                device_detail_data.as_mut(),
                *required_length.as_ptr(),
                required_length.as_mut_ptr(),
                core::ptr::null_mut()
            )
        };
        if success == FALSE {
            println!("SetupDiGetDeviceInterfaceDetailW Error[2]!");
            continue;
        }
        let h_hc_dev = unsafe {
            CreateFileW(
                &device_detail_data.as_ref().DevicePath as *const _,
                GENERIC_WRITE,
                FILE_SHARE_WRITE,
                core::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                core::ptr::null_mut(),
            )
        };
        if h_hc_dev == INVALID_HANDLE_VALUE {
            println!("CreateFileW Error!");
            continue;
        }
        enumerate_host_controller(h_hc_dev);
        unsafe { CloseHandle(h_hc_dev) };
        unsafe { HeapFree(heap_handle, 0, device_detail_data.cast().as_ptr()) };
        index += 1;
    }
}

struct DeviceNode {
    device_desc_name: std::ffi::OsString,
    device_driver_name: std::ffi::OsString,
    device_path: std::ffi::OsString,
    device_info_data: Box<SP_DEVINFO_DATA>,
}

use core::fmt;
impl fmt::Debug for DeviceNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceNode")
            .field("desc", &self.device_desc_name)
            .field("driver", &self.device_driver_name)
            .field("path", &self.device_path)
            .finish()
    }
}

fn enumerate_all_devices() -> (Vec<DeviceNode>, Vec<DeviceNode>){
    let devices = enumerate_all_devices_with_guid(&GUID_DEVINTERFACE_USB_DEVICE as *const _);
    // println!("{:#?}", devices);
    let hubs = enumerate_all_devices_with_guid(&GUID_DEVINTERFACE_USB_HUB as *const _);
    // println!("{:#?}", hubs);
    (devices, hubs)
}

fn enumerate_all_devices_with_guid(guid: *const GUID) -> Vec<DeviceNode> {
    let device_info = unsafe {
        SetupDiGetClassDevsW(
            guid,
            core::ptr::null(),
            core::ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if device_info == INVALID_HANDLE_VALUE {
        println!("SetupDiGetClassDevsW Error!");
    }
    let mut index = 0;
    let mut nodes = Vec::new();
    loop {
        let mut device_info_data = Box::<SP_DEVINFO_DATA>::new_uninit();
        unsafe { device_info_data.get_mut() }.cbSize = size_of::<SP_DEVINFO_DATA>() as DWORD;
        let success = unsafe { 
            SetupDiEnumDeviceInfo(
                device_info,
                index,
                device_info_data.as_mut_ptr()
            ) 
        };
        if success == FALSE {
            break;
        }
        // println!("{}", index);
        let (device_desc_pb, device_desc_pb_len) = get_device_property(device_info, device_info_data.as_mut_ptr(), SPDRP_DEVICEDESC);
        let (driver_pb, driver_pb_len) = get_device_property(device_info, device_info_data.as_mut_ptr(), SPDRP_DRIVER);

        // print (?)
        use std::os::windows::prelude::*;
        let name_device_desc = std::ffi::OsString::from_wide(unsafe { core::slice::from_raw_parts(
            device_desc_pb as *mut _,
            device_desc_pb_len as usize / 2 - 1
        ) }); 
        // println!("{:?}", name_device_desc);
        let name_driver = std::ffi::OsString::from_wide(unsafe { core::slice::from_raw_parts(
            driver_pb as *mut _,
            driver_pb_len as usize / 2 - 1
        ) }); 
        // println!("{:?}", name_driver);
        // end print

        let mut device_interface_data = MaybeUninit::<SP_DEVICE_INTERFACE_DATA>::uninit();
        unsafe { device_interface_data.get_mut() }.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as DWORD;
        let success = unsafe {
            SetupDiEnumDeviceInterfaces(
                device_info,
                core::ptr::null_mut(),
                guid,
                index,
                device_interface_data.as_mut_ptr()
            )
        };
        if success == FALSE {
            println!("SetupDiEnumDeviceInterfaces Error!");
            continue;
        }
        let mut required_length = MaybeUninit::uninit();
        let success = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                device_info,
                device_interface_data.as_mut_ptr(),
                core::ptr::null_mut(),
                0,
                required_length.as_mut_ptr(),
                core::ptr::null_mut()
            )
        };
        if success == FALSE && unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            println!("SetupDiGetDeviceInterfaceDetailW Error[1]!");
            continue;
        }
        let heap_handle = unsafe { GetProcessHeap() };
        let device_detail_data = NonNull::new(unsafe { 
            HeapAlloc(heap_handle, 0, required_length.assume_init() as usize) as *mut _ 
        });
        let mut device_detail_data = if let Some(device_detail_data) = device_detail_data { 
            device_detail_data.cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>()
        } else {
            println!("Error HeapAlloc");
            continue;
        };
        unsafe { device_detail_data.as_mut() }.cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as DWORD;
        let success = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                device_info,
                device_interface_data.as_mut_ptr(),
                device_detail_data.as_mut(),
                *required_length.as_ptr(),
                required_length.as_mut_ptr(),
                core::ptr::null_mut()
            )
        };
        if success == FALSE {
            println!("SetupDiGetDeviceInterfaceDetailW Error[2]!");
            continue;
        }
        let path = &unsafe { device_detail_data.as_ref() }.DevicePath;
        let name_path = std::ffi::OsString::from_wide(unsafe { core::slice::from_raw_parts(
            path as *const _ as *mut _,
            required_length.assume_init() as usize / 2 - 3
        ) }); 
        // println!("{:?}", name_path);
        let node = DeviceNode {
            device_desc_name: name_device_desc,
            device_driver_name: name_driver,
            device_path: name_path,
            device_info_data: unsafe { device_info_data.assume_init() }
        };
        nodes.push(node);
        unsafe { HeapFree(heap_handle, 0, driver_pb as *const _ as *mut _) };
        unsafe { HeapFree(heap_handle, 0, device_desc_pb as *const _ as *mut _) };
        index += 1;
    }
    nodes
}

fn get_device_property(
    device_info_set: HDEVINFO,
    device_info_data: PSP_DEVINFO_DATA,
    property: DWORD,
) -> (*mut u8, DWORD) {
    let mut required_length = MaybeUninit::uninit();
    let success = unsafe { 
        SetupDiGetDeviceRegistryPropertyW(
            device_info_set,
            device_info_data,
            property,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            0,
            required_length.as_mut_ptr()
        )
    };
    if success == FALSE && unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        println!("SetupDiGetDeviceRegistryPropertyW Error[1]!");
    }
    let heap_handle = unsafe { GetProcessHeap() };
    let property_buffer = NonNull::new(unsafe { 
        HeapAlloc(heap_handle, 0, required_length.assume_init() as usize) as *mut _ 
    });
    let property_buffer = if let Some(property_buffer) = property_buffer { 
        property_buffer.as_ptr()
    } else {
        println!("Error HeapAlloc");
        panic!()
    };
    let success = unsafe { 
        SetupDiGetDeviceRegistryPropertyW(
            device_info_set,
            device_info_data,
            property,
            core::ptr::null_mut(),
            property_buffer,
            required_length.assume_init(),
            required_length.as_mut_ptr()
        )
    };
    if success == FALSE {
        println!("SetupDiGetDeviceRegistryPropertyW Error[2]!");
    }
    (property_buffer, unsafe { required_length.assume_init() })
}


fn enumerate_host_controller(h_hc_dev: HANDLE) {
    // get HCD driver key name from handle; GetHCDDriverKeyName
    let mut driver_key_name = MaybeUninit::<USB_HCD_DRIVERKEY_NAME>::uninit();
    unsafe { driver_key_name.get_mut() }.ActualLength = 0;
    let nbytes = 0;
    let success = unsafe { 
        DeviceIoControl(
            h_hc_dev,
            IOCTL_GET_HCD_DRIVERKEY_NAME,
            driver_key_name.as_mut_ptr() as *mut _,
            size_of::<USB_HCD_DRIVERKEY_NAME>() as DWORD,
            driver_key_name.as_mut_ptr() as *mut _,
            size_of::<USB_HCD_DRIVERKEY_NAME>() as DWORD,
            &nbytes as *const _ as *mut _,
            core::ptr::null_mut()
        )
    };
    if success == FALSE {
        println!("DeviceIoControl Error[1]!");
        return;
    } 
    let nbytes = unsafe { driver_key_name.get_mut() }.ActualLength;
    let heap_handle = unsafe { GetProcessHeap() };
    let driver_key_name_w = NonNull::new(unsafe { 
        HeapAlloc(heap_handle, 0, nbytes as usize) as *mut _ 
    });
    let driver_key_name_w = if let Some(driver_key_name_w) = driver_key_name_w { 
        driver_key_name_w.cast::<USB_HCD_DRIVERKEY_NAME>()
    } else {
        println!("Error HeapAlloc");
        return;
    };
    let success = unsafe { 
        DeviceIoControl(
            h_hc_dev,
            IOCTL_GET_HCD_DRIVERKEY_NAME,
            driver_key_name_w.cast().as_ptr(),
            nbytes,
            driver_key_name_w.cast().as_ptr(),
            nbytes,
            &nbytes as *const _ as *mut _,
            core::ptr::null_mut()
        )
    };
    if success == FALSE {
        println!("DeviceIoControl Error[2]!");
        return;
    } 
    use std::os::windows::prelude::*;
    let name = std::ffi::OsString::from_wide(unsafe { core::slice::from_raw_parts(
        &driver_key_name_w.as_ref().DriverKeyName as *const _,
        (nbytes as usize - size_of::<ULONG>()) / 2 - 2 
    ) }); // cut two trailing \0 bytes
    println!("│ Driver Key Name: {:?}", name);

    // find device instance matching the driver name
    

    // Clean-up
    unsafe { HeapFree(heap_handle, 0, driver_key_name_w.cast().as_ptr()) };
}

fn main() {
    enumerate_host_controllers();
}
