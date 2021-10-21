mod get_device_property;
use get_device_property::get_device_property;
mod host_controller;
use host_controller::host_controllers;
mod usb_path;
mod usb_subsystem;

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

pub mod api {
    #![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
    use winapi::{shared::minwindef::*, um::winnt::*};
    use winapi::{STRUCT, ENUM};
    pub const IOCTL_USB_GET_ROOT_HUB_NAME: DWORD = 0x220408;
    pub const IOCTL_GET_HCD_DRIVERKEY_NAME: DWORD = 0x220424;
    winapi::STRUCT!{struct USB_HCD_DRIVERKEY_NAME {
        ActualLength: ULONG,
        DriverKeyName: [WCHAR; 1],
    }}
    pub type PUSB_HCD_DRIVERKEY_NAME = *mut USB_HCD_DRIVERKEY_NAME;
    STRUCT!{struct USBUSER_POWER_INFO_REQUEST {
        Header: USBUSER_REQUEST_HEADER,
        PowerInformation: USB_POWER_INFO,
    }}
    pub type PUSBUSER_POWER_INFO_REQUEST = *mut USBUSER_POWER_INFO_REQUEST;
    STRUCT!{struct USBUSER_REQUEST_HEADER {
        UsbUserRequest: ULONG,
        UsbUserStatusCode: USB_USER_ERROR_CODE,
        RequestBufferLength: ULONG,
        ActualBufferLength: ULONG,
    }}
    pub type PUSBUSER_REQUEST_HEADER = *mut USBUSER_REQUEST_HEADER;
    ENUM!{enum USB_USER_ERROR_CODE {
        UsbUserSuccess,
        UsbUserNotSupported,
        UsbUserInvalidRequestCode,
        UsbUserFeatureDisabled,
        UsbUserInvalidHeaderParameter,
        UsbUserInvalidParameter,
        UsbUserMiniportError,
        UsbUserBufferTooSmall,
        UsbUserErrorNotMapped,
        UsbUserDeviceNotStarted,
        UsbUserNoDeviceConnected,
    }}
    STRUCT!{struct USB_POWER_INFO {
        SystemState: WDMUSB_POWER_STATE,
        HcDevicePowerState: WDMUSB_POWER_STATE,
        HcDeviceWake: WDMUSB_POWER_STATE,
        HcSystemWake: WDMUSB_POWER_STATE,
        RhDevicePowerState: WDMUSB_POWER_STATE,
        RhDeviceWake: WDMUSB_POWER_STATE,
        RhSystemWake: WDMUSB_POWER_STATE,
        LastSystemSleepState: WDMUSB_POWER_STATE,
        CanWakeup: BOOLEAN,
        IsPowered: BOOLEAN,
    }}
    pub type PUSB_POWER_INFO = *mut USB_POWER_INFO;
    ENUM!{enum WDMUSB_POWER_STATE {
        WdmUsbPowerNotMapped,
        WdmUsbPowerSystemUnspecified,
        WdmUsbPowerSystemWorking,
        WdmUsbPowerSystemSleeping1,
        WdmUsbPowerSystemSleeping2,
        WdmUsbPowerSystemSleeping3,
        WdmUsbPowerSystemHibernate,
        WdmUsbPowerSystemShutdown,
        WdmUsbPowerDeviceUnspecified,
        WdmUsbPowerDeviceD0,
        WdmUsbPowerDeviceD1,
        WdmUsbPowerDeviceD2,
        WdmUsbPowerDeviceD3,
    }}
    STRUCT!{struct USB_ROOT_HUB_NAME {
        ActualLength: ULONG,
        RootHubName: [WCHAR; 1],
    }}
    pub type PUSB_ROOT_HUB_NAME = *mut USB_ROOT_HUB_NAME;
}
use api::*;

fn enumerate_host_controllers() {
    let (devices, hubs) = enumerate_all_devices(); //todo
    println!("{:#?}", devices);
    println!("{:#?}", hubs);
    
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
    unsafe { device_info_data.assume_init_mut() }.cbSize = size_of::<SP_DEVINFO_DATA>() as DWORD;
    let mut device_interface_data = MaybeUninit::<SP_DEVICE_INTERFACE_DATA>::uninit();
    unsafe { device_interface_data.assume_init_mut() }.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as DWORD;
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
    device_desc_name: Option<std::ffi::OsString>,
    device_driver_name: Option<std::ffi::OsString>,
    device_path: std::ffi::OsString,
    device_info_data: SP_DEVINFO_DATA,
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
        let mut device_info_data: MaybeUninit<SP_DEVINFO_DATA> = MaybeUninit::uninit();
        unsafe { device_info_data.assume_init_mut() }.cbSize = size_of::<SP_DEVINFO_DATA>() as DWORD;
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
        use std::os::windows::prelude::*;
        // let heap_handle = unsafe { GetProcessHeap() };
        // let name_device_desc = get_device_property(device_info, device_info_data.as_mut_ptr(), SPDRP_DEVICEDESC)
        //     .map(|(property_buffer, property_buffer_len)| {
        //         let ans = std::ffi::OsString::from_wide(unsafe { core::slice::from_raw_parts(
        //             property_buffer as *const _,
        //             property_buffer_len as usize / 2 - 1
        //         ) });
        //         unsafe { HeapFree(heap_handle, 0, property_buffer as *mut _) };
        //         ans
        //     });
        let mut buf = vec![0; 100];
        let name_device_desc = 
            get_device_property(device_info, device_info_data.as_mut_ptr(), SPDRP_DEVICEDESC, &mut buf)
            .ok();

        let name_driver = 
            get_device_property(device_info, device_info_data.as_mut_ptr(), SPDRP_DRIVER, &mut buf)
            .ok();
        // print (?)
        // println!("{:?}", name_device_desc);
        // println!("{:?}", name_driver);
        // end print

        let mut device_interface_data = MaybeUninit::<SP_DEVICE_INTERFACE_DATA>::uninit();
        unsafe { device_interface_data.assume_init_mut() }.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as DWORD;
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
        unsafe { HeapFree(heap_handle, 0, device_detail_data.cast().as_ptr())};
        // println!("{:?}", name_path);
        let node = DeviceNode {
            device_desc_name: name_device_desc,
            device_driver_name: name_driver,
            device_path: name_path,
            device_info_data: unsafe { device_info_data.assume_init() }
        };
        nodes.push(node);
        
        index += 1;
    }
    nodes
}

fn driver_name_to_device_inst(
    driver_name: &std::ffi::OsStr,
) -> Option<(HDEVINFO, SP_DEVINFO_DATA)> {
    let device_info = unsafe {
        SetupDiGetClassDevsW (
            core::ptr::null(),
            core::ptr::null(),
            core::ptr::null_mut(),
            DIGCF_ALLCLASSES | DIGCF_PRESENT
        )
    };
    if device_info == INVALID_HANDLE_VALUE {
        println!("SetupDiGetClassDevsW Error!");
    }
    let mut index = 0;
    loop {
        let mut device_info_data: MaybeUninit<SP_DEVINFO_DATA> = MaybeUninit::uninit();
        unsafe { device_info_data.assume_init_mut() }.cbSize = size_of::<SP_DEVINFO_DATA>() as DWORD;
        let success = unsafe { 
            SetupDiEnumDeviceInfo(
                device_info,
                index,
                device_info_data.as_mut_ptr()
            ) 
        };
        index += 1;
        if success == FALSE {
            break;
        }
        let mut buf = vec![0; 300];
        let buf_string = {
            let tmp = get_device_property(device_info, device_info_data.as_mut_ptr(), SPDRP_DRIVER, &mut buf);
            if let Ok(a) = tmp {
                // println!("string: {:?}", a);
                a
            } else {
                // println!("SPDRP_DRIVER");
                continue
            }
        };

        // println!("{}: {:?}; {:?}", index, buf_string, driver_name);
        // println!("{}: {:#?}", index, 
        //     get_device_pnp_strings(buf_string.clone(), device_info, device_info_data.clone())
        // );
        if buf_string == driver_name {
            // println!("{}: {:#?}", index, 
            //     get_device_pnp_strings(buf_string.clone(), device_info, device_info_data.as_mut() as *const _ as *mut _)
            // );
            // println!("111 {:?}", buf_string.clone());
            // println!("222 {:?}", device_info);
            // println!("333 {:?}", device_info_data.as_mut_ptr());
            // unsafe { SetupDiDestroyDeviceInfoList(device_info) };
            return Some((device_info, unsafe { device_info_data.assume_init() }));
        }
    }
    unsafe { SetupDiDestroyDeviceInfoList(device_info) };
    None
}

// We may enumerate more about this device here
#[derive(Debug)]
struct DevicePnpStrings {
    device_id: std::ffi::OsString,
    device_desc: Option<std::ffi::OsString>,
    device_hw_id: Option<std::ffi::OsString>,
    service: Option<std::ffi::OsString>,
    device_class: Option<std::ffi::OsString>,
}

fn get_device_pnp_strings(
    device_id: std::ffi::OsString,
    device_info: HDEVINFO, 
    device_info_data: PSP_DEVINFO_DATA
) -> DevicePnpStrings {
    let mut buf = vec![0; 200];
    let device_desc = get_device_property(device_info, device_info_data, SPDRP_DEVICEDESC, &mut buf).ok();
    let device_hw_id = get_device_property(device_info, device_info_data, SPDRP_HARDWAREID, &mut buf).ok();
    let service = get_device_property(device_info, device_info_data, SPDRP_SERVICE, &mut buf).ok();
    let device_class = get_device_property(device_info, device_info_data, SPDRP_CLASS, &mut buf).ok();

    // println!("ID {:?}", device_id);
    // println!("DESC {:?}", device_desc);
    // println!("HWID {:?}", device_hw_id);
    // println!("SERV {:?}", service);
    // println!("CLAS {:?}", device_class);
        
    DevicePnpStrings {
        device_id,
        device_desc,
        device_hw_id,
        service,
        device_class,
    }
}

fn driver_name_to_device_properties(
    driver_name: &std::ffi::OsStr
) -> Option<DevicePnpStrings> {
    // println!("driver name {:?}", driver_name);
    let (device_info, mut device_info_data) = 
        if let Some(ans) = driver_name_to_device_inst(driver_name) {
            ans
        } else {
            return None;
        };
    let mut len = 0;
    let success = unsafe {
        SetupDiGetDeviceInstanceIdW(
            device_info,
            &mut device_info_data,
            core::ptr::null_mut(),
            0,
            &mut len as *const _ as *mut _
        )
    };
    if success == FALSE && unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        println!("SetupDiGetDeviceInstanceIdW Error[1]! {}", unsafe { GetLastError() });
    }
    len += 1;
    let heap_handle = unsafe { GetProcessHeap() };
    let device_id_buf = NonNull::new(unsafe { 
        HeapAlloc(heap_handle, 0, len as usize) as *mut _ 
    });
    let device_id_buf = if let Some(device_id_buf) = device_id_buf { 
        device_id_buf
    } else {
        println!("Error HeapAlloc");
        return None;
    };
    let success = unsafe {
        SetupDiGetDeviceInstanceIdW(
            device_info,
            &mut device_info_data,
            device_id_buf.cast().as_mut(),
            len,
            &mut len as *const _ as *mut _
        )
    };
    if success == FALSE {
        println!("SetupDiGetDeviceInstanceIdW Error[2]!");
    }
    use std::os::windows::prelude::*;    
    let device_id = std::ffi::OsString::from_wide(unsafe { core::slice::from_raw_parts(
        device_id_buf.cast().as_ref(),
        len as usize - 1
    ) });

    unsafe { HeapFree(heap_handle, 0, device_id_buf.cast().as_ptr()) };

    // println!("111 {:?}", device_id.clone());
    // println!("222 {:?}", device_info);
    // println!("333 {:?}", device_info_data.as_mut() as *const _ as PSP_DEVINFO_DATA);
    
    Some(get_device_pnp_strings(device_id, device_info, &mut device_info_data))
}

fn enumerate_host_controller(h_hc_dev: HANDLE) {
    // get HCD driver key name from handle; GetHCDDriverKeyName
    let mut driver_key_name = MaybeUninit::<USB_HCD_DRIVERKEY_NAME>::uninit();
    unsafe { driver_key_name.assume_init_mut() }.ActualLength = 0;
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
    let nbytes = unsafe { driver_key_name.assume_init_mut() }.ActualLength;
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
    let driver_key_name = std::ffi::OsString::from_wide(unsafe { core::slice::from_raw_parts(
        &driver_key_name_w.as_ref().DriverKeyName as *const _,
        (nbytes as usize - size_of::<ULONG>()) / 2 - 2 
    ) }); // cut two trailing \0 bytes
    println!("│ (HCD Driver Key Name: {:?})", driver_key_name);
    unsafe { HeapFree(heap_handle, 0, driver_key_name_w.cast().as_ptr()) };

    // find device instance matching the driver name
    let dev_props = driver_name_to_device_properties(&driver_key_name)
        .expect("get dev props");
    // println!("{:#?}", dev_props);
    println!("│ Id: {:?}", dev_props.device_id);
    println!("│ Desc: {:?}", dev_props.device_desc.unwrap());
    println!("│ Service: {:?}", dev_props.service.unwrap());
    println!("│ Class: {:?}", dev_props.device_class.unwrap());
    
    // // Get the USB Host Controller power map
    // let hc_info = get_host_controller_power_map(h_hc_dev);
    // println!("| Info: {:?}", hc_info);

    // Get bus, device and function

    // Get the USB Host Controller info

    // Get name of root hub, enumerate root hub
    let root_hub_name = get_root_hub_name(h_hc_dev);
    println!("│ ├ Root hub");
    println!("│ │ Name: {:?}", root_hub_name);

}

fn get_root_hub_name(h_hc_dev: HANDLE) -> std::ffi::OsString {
    let mut root_hub_name_w: MaybeUninit<USB_ROOT_HUB_NAME> = MaybeUninit::uninit();
    unsafe { root_hub_name_w.assume_init_mut() }.ActualLength = 0;
    let mut n_bytes = 0;
    let success = unsafe {
        DeviceIoControl(
            h_hc_dev, 
            IOCTL_USB_GET_ROOT_HUB_NAME, 
            core::ptr::null_mut(), 
            0,
            root_hub_name_w.as_mut_ptr() as *mut _, 
            size_of::<USB_ROOT_HUB_NAME>() as DWORD, 
            &mut n_bytes as *const _ as *mut _, 
            core::ptr::null_mut()
        )
    };
    if success == FALSE && unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        println!("DeviceIoControl Error[1]! {}", unsafe { GetLastError() });
    }
    // println!("{:?} {:?}",
    //     unsafe { root_hub_name_w.get_mut() }.ActualLength, 
    //     n_bytes
    // );
    n_bytes += unsafe { root_hub_name_w.assume_init_mut() }.ActualLength;
    let heap_handle = unsafe { GetProcessHeap() };
    let root_hub_name_w = NonNull::new(unsafe { 
        HeapAlloc(heap_handle, 0, n_bytes as usize) as *mut _ 
    });
    let root_hub_name_w = if let Some(root_hub_name_w) = root_hub_name_w { 
        root_hub_name_w.cast::<USB_ROOT_HUB_NAME>()
    } else {
        println!("Error HeapAlloc");
        unimplemented!()
    };
    let success = unsafe {
        DeviceIoControl(
            h_hc_dev, 
            IOCTL_USB_GET_ROOT_HUB_NAME, 
            core::ptr::null_mut(), 
            0,
            root_hub_name_w.cast().as_ptr(), 
            n_bytes, 
            &mut n_bytes as *const _ as *mut _, 
            core::ptr::null_mut()
        )
    };
    if success == FALSE {
        println!("DeviceIoControl Error[2]!");
    }
    use std::os::windows::prelude::*;
    let root_hub_name = std::ffi::OsString::from_wide(unsafe { core::slice::from_raw_parts(
        &root_hub_name_w.as_ref().RootHubName as *const _ as *mut _,
        (n_bytes as usize - size_of::<DWORD>()) / 2 - 1
    ) }); 
    unsafe { HeapFree(heap_handle, 0, root_hub_name_w.cast().as_ptr()) };
    root_hub_name
}

// #[derive(Debug)]
// struct UsbHostControllerInfo {

// }

// fn get_host_controller_power_map(
//     h_hc_dev: HANDLE,
// ) -> UsbHostControllerInfo {
//     let _ = h_hc_dev;
//     unimplemented!()
// }

fn main() {
    enumerate_host_controllers();

    let host_controllers = host_controllers().unwrap().collect::<Vec<_>>();
    for host_controller in host_controllers {
        let host_controller = host_controller.unwrap();
        println!("Host controller: {:?}", host_controller);
    }
}
