pub use usb_device::UsbDirection;
pub use usb_device::endpoint::{EndpointType, EndpointAddress};
pub mod builder;
pub mod usb;
pub mod generator;
pub mod cdc;


pub trait EndpointInfo {
    fn descriptor(&self) -> &usb::UsbEndpointDescriptor;

    fn address(&self) -> EndpointAddress {
        self.descriptor().address.into()
    }

    fn ep_type(&self) -> EndpointType {
        match self.descriptor().attributes {
            0b00 => EndpointType::Control,
            0b01 => EndpointType::Isochronous,
            0b10 => EndpointType::Bulk,
            0b11 => EndpointType::Interrupt,
            _ => unreachable!(),
        }
    }

    fn direction(&self) -> UsbDirection {
        self.address().direction()
    }
}

impl EndpointInfo for usb::UsbEndpointDescriptor {
    fn descriptor(&self) -> &usb::UsbEndpointDescriptor {
        self
    }
}
