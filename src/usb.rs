use std::{
    cell::RefCell, sync::{Arc, Mutex}
};

use anyhow::Result;
use rusb::{
    Device, DeviceDescriptor, DeviceHandle, Direction, GlobalContext, Hotplug, TransferType, UsbContext
};
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Copy)]
pub struct Endpoints {
    pub config: u8,
    pub iface: u8,
    pub setting: u8,
    pub read_addr: u8,
    pub write_addr: u8,
}

#[derive(Debug)]
pub struct UsbInner {
    pub endpoints: Endpoints,
    pub handle: RefCell<DeviceHandle<GlobalContext>>,
}

#[derive(Debug)]
pub(crate) struct UsbDevice {
    pub connect_count: usize,
    pub inner: Option<UsbInner>,
    listener: Option<JoinHandle<()>>,
}

impl UsbDevice {
    pub fn new(vid: u16, pid: u16) -> Result<Arc<Mutex<UsbDevice>>> {
        let usb: Arc<Mutex<UsbDevice>> = Arc::new(Mutex::new(UsbDevice {
            connect_count: 0,
            inner: None,
            listener: None,
        }));
        usb.lock().unwrap().listener = Some(start_usb_listener(vid, pid, usb.clone()));
        Ok(usb)
    }
}

fn find_endpoints<T>(
    device: &Device<T>,
    device_desc: &DeviceDescriptor,
    transfer_type: TransferType
) -> Option<Endpoints>
where
    T: UsbContext,
{
    for n in 0..device_desc.num_configurations() {
        let config_desc = match device.config_descriptor(n) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for interface in config_desc.interfaces() {
            for interface_desc in interface.descriptors() {
                let endpoint_desc_in = interface_desc.endpoint_descriptors().find(|d| d.direction() == Direction::In && d.transfer_type() == transfer_type);
                let endpoint_desc_out = interface_desc.endpoint_descriptors().find(|d| d.direction() == Direction::Out && d.transfer_type() == transfer_type);

                if let (Some(epd_in),Some(epd_out)) = (endpoint_desc_in, endpoint_desc_out) {
                    return Some(Endpoints {
                        config: config_desc.number(),
                        iface: interface_desc.interface_number(),
                        setting: interface_desc.setting_number(),
                        read_addr: epd_in.address(),
                        write_addr: epd_out.address(),
                    })
                } else {
                    return None
                }
            }
        }
    }

    None
}

fn configure_endpoints<T: UsbContext>(handle: &mut DeviceHandle<T>, endpoints: &Endpoints) -> Result<()> {
    handle.set_active_configuration(endpoints.config)?;
    handle.claim_interface(endpoints.iface)?;
    handle.set_alternate_setting(endpoints.iface, endpoints.setting)?;
    Ok(())
}

// Hotplug listener
pub(crate) struct UsbHotplugHandler {
    usb: Arc<Mutex<UsbDevice>>,
}

impl Hotplug<GlobalContext> for UsbHotplugHandler {
    fn device_arrived(&mut self, device: Device<GlobalContext>) {
        tracing::debug!("arexx device arrived: {:?}", device);

        let desc = device.device_descriptor().expect("cannot read device descriptor");
        let mut handle = device.open().expect("cannot open device");

        let endpoints = find_endpoints(&device, &desc, TransferType::Bulk).expect("could not find r/w endpoints for bulk transfer type");
        
        match handle.kernel_driver_active(endpoints.iface) {
            Ok(true) => {
                handle.detach_kernel_driver(endpoints.iface).expect("cannot detach kernel driver");
                true
            }
            _ => false,
        };

        configure_endpoints(&mut handle, &endpoints).expect("cannot configure endpoints");

        tracing::trace!("endpoints = {:?}", endpoints);

        let mut usb = self.usb.lock().unwrap();
        tracing::trace!("found arexx endpoints: {:?}", endpoints);
        usb.connect_count += 1;
        usb.inner = Some(UsbInner {
            endpoints,
            handle: RefCell::new(handle),
        })
    }

    fn device_left(&mut self, device: Device<GlobalContext>) {
        tracing::debug!("arexx device left: {:?}", device);

        // cleanup device
        {
            if let Some(inner) = self.usb.lock().unwrap().inner.as_ref() {
                let handle = inner.handle.borrow_mut();
                handle.release_interface(inner.endpoints.iface).expect("cannot release interface");
                match handle.kernel_driver_active(inner.endpoints.iface) {
                    Ok(true) => handle.attach_kernel_driver(inner.endpoints.iface).expect("cannot attach kernel driver"),
                    _ => ()
                }
            }
        }

        self.usb.lock().unwrap().inner = None;
    }
}

fn start_usb_listener(vid: u16, pid: u16, usb: Arc<Mutex<UsbDevice>>) -> JoinHandle<()> {
    let context = GlobalContext::default();

    let usb_handler = Box::new(UsbHotplugHandler { usb });
    let reg: Result<rusb::Registration<GlobalContext>, rusb::Error> = rusb::HotplugBuilder::new()
        .vendor_id(vid)
        .product_id(pid)
        .enumerate(true)
        .register(context, usb_handler);

    tokio::task::spawn_blocking(move || {
        let _reg = Some(reg.unwrap());
        loop {
            match context.handle_events(None) {
                Ok(_reg) => {}
                Err(e) => {
                    tracing::error!("error handling USB events: {:?}", e);
                    break;
                }
            }
        }
    })
}