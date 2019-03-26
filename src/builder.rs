use crate::usb::{
    UsbConfigurationDescriptor, UsbCustomDescriptor, UsbDescriptorType, UsbDescriptorWriter,
    UsbDeviceDescriptor, UsbEndpointDescriptor, UsbInterfaceDescriptor, UsbString,
    UsbStringAllocator,
};
use bit_field::BitField;
use std::collections::HashMap;
use usb_device::descriptor::lang_id;
use usb_device::endpoint::{EndpointAddress, EndpointType};
use usb_device::UsbDirection;

/// A USB vendor ID and product ID pair.
pub struct UsbVidPid(pub u16, pub u16);

macro_rules! generate_field_setters {
    ( $( $(#[$meta:meta])* $name:ident: $type:ty, )* ) => {
        $(
            $(#[$meta])*
            pub fn $name(mut self, $name: $type) -> Self {
                self.descriptor.$name = $name;
                self
            }
        )*
    }
}

#[derive(Debug)]
pub struct DeviceConfig {
    pub device_descriptor: Vec<u8>,
    pub configuration_descriptor: Vec<u8>,
    pub string_descriptors: HashMap<u8, Vec<u8>>,
    pub custom_strings: HashMap<u8, usize>,
    pub endpoints: Vec<UsbEndpointDescriptor>,
}

pub struct DeviceBuilder {
    pub descriptor: UsbDeviceDescriptor,
    pub configuration_desc: UsbConfigurationDescriptor,
    pub interfaces: Vec<InterfaceBuilder>,
}

impl DeviceBuilder {
    pub fn new(vid_pid: UsbVidPid) -> Self {
        Self {
            descriptor: UsbDeviceDescriptor {
                device_class: 0,
                device_sub_class: 0,
                device_protocol: 0,
                max_packet_size_0: 8,
                vendor_id: vid_pid.0,
                product_id: vid_pid.1,
                device_release: 0x0010,
                manufacturer: UsbString::None,
                product: UsbString::None,
                serial_number: UsbString::None,
            },
            configuration_desc: UsbConfigurationDescriptor {
                configuration_value: 1,
                configuration_string: UsbString::None,
                attributes: 0x80,
                max_power: 50,
            },
            interfaces: Vec::new(),
        }
    }

    generate_field_setters! {
        /// Sets the device class code assigned by USB.org. Set to `0xff` for vendor-specific
        /// devices that do not conform to any class.
        ///
        /// Default: `0x00` (class code specified by interfaces)
        device_class: u8,

        /// Sets the device sub-class code. Depends on class.
        ///
        /// Default: `0x00`
        device_sub_class: u8,

        /// Sets the device protocol code. Depends on class and sub-class.
        ///
        /// Default: `0x00`
        device_protocol: u8,

        /// Sets the device release version in BCD.
        ///
        /// Default: `0x0010` ("0.1")
        device_release: u16,
    }

    /// Sets the maximum packet size in bytes for the control endpoint 0.
    ///
    /// Valid values are 8, 16, 32 and 64. There's generally no need to change this from the default
    /// value of 8 bytes unless a class uses control transfers for sending large amounts of data, in
    /// which case using a larger packet size may be more efficient.
    ///
    /// Default: 8 bytes
    pub fn max_packet_size_0(mut self, max_packet_size_0: u8) -> Self {
        match max_packet_size_0 {
            8 | 16 | 32 | 64 => {}
            _ => panic!("invalid max_packet_size_0"),
        }

        self.descriptor.max_packet_size_0 = max_packet_size_0;
        self
    }

    /// Sets whether the device may have an external power source.
    ///
    /// This should be set to `true` even if the device is sometimes self-powered and may not
    /// always draw power from the USB bus.
    ///
    /// Default: `false`
    ///
    /// See also: `max_power`
    pub fn self_powered(mut self, self_powered: bool) -> Self {
        self.configuration_desc.attributes.set_bit(6, self_powered);
        self
    }

    /// Sets whether the device supports remotely waking up the host is requested.
    ///
    /// Default: `false`
    pub fn supports_remote_wakeup(mut self, supports_remote_wakeup: bool) -> Self {
        self.configuration_desc
            .attributes
            .set_bit(5, supports_remote_wakeup);
        self
    }

    /// Sets the maximum current drawn from the USB bus by the device in milliamps.
    ///
    /// The default is 100 mA. If your device always uses an external power source and never draws
    /// power from the USB bus, this can be set to 0.
    ///
    /// See also: `self_powered`
    ///
    /// Default: 100mA
    pub fn max_power(mut self, max_power_ma: usize) -> Self {
        if max_power_ma > 500 {
            panic!("max_power is too much")
        }

        self.configuration_desc.max_power = (max_power_ma / 2) as u8;
        self
    }

    /// Sets the manufacturer name string descriptor.
    ///
    /// Default: (none)
    pub fn manufacturer(mut self, manufacturer: impl Into<String>) -> Self {
        self.descriptor.manufacturer = UsbString::Const(manufacturer.into());
        self
    }

    /// Sets the product name string descriptor.
    ///
    /// Default: (none)
    pub fn product(mut self, product: impl Into<String>) -> Self {
        self.descriptor.product = UsbString::Const(product.into());
        self
    }

    /// Sets the serial number string descriptor.
    ///
    /// Default: (none)
    pub fn serial_number(mut self, serial_number: impl Into<String>) -> Self {
        self.descriptor.serial_number = UsbString::Const(serial_number.into());
        self
    }

    /// Sets the configuration string descriptor.
    ///
    /// Default: (none)
    pub fn configuration(mut self, configuration: impl Into<String>) -> Self {
        self.configuration_desc.configuration_string = UsbString::Const(configuration.into());
        self
    }

    fn add_interface(&mut self, interface: InterfaceBuilder) {
        let index = interface.descriptor.interface_number as usize;
        assert!(index < self.interfaces.len());
        assert_eq!(interface.descriptor.alternate_setting, 0); // Alternate settings are not supported yet
        assert!(!interface.endpoints.is_empty());

        self.interfaces[index] = interface;
    }

    pub fn alloc_interface(&mut self) -> InterfaceBuilder {
        let index = self.interfaces.len();
        let builder = InterfaceBuilder::new(index as u8);
        self.interfaces.push(builder.clone());
        builder
    }

    pub fn build(self) -> DeviceConfig {
        assert!(!self.interfaces.is_empty());

        // Allocate strings
        let mut str_alloc = UsbStringAllocator::new();
        str_alloc.alloc(&self.descriptor.manufacturer);
        str_alloc.alloc(&self.descriptor.product);
        str_alloc.alloc(&self.descriptor.serial_number);
        str_alloc.alloc(&self.configuration_desc.configuration_string);
        for interface in &self.interfaces {
            str_alloc.alloc(&interface.descriptor.interface_string);
        }
        str_alloc.alloc(&UsbString::Custom(42));

        // Generate device descriptor
        let mut w = UsbDescriptorWriter::new();
        w.device(&self.descriptor, 1, &str_alloc);
        let device_descriptor = w.finish();

        // Generate configuration descriptor
        let mut w = UsbDescriptorWriter::new();
        w.configuration(&self.configuration_desc, &str_alloc);
        for interface in &self.interfaces {
            w.interface(&interface.descriptor, &str_alloc);
            for custom in &interface.custom_descriptors {
                w.custom_descriptor(custom);
            }
            for endpoint in &interface.endpoints {
                w.endpoint(&endpoint);
            }
        }
        let configuration_descriptor = w.finish();

        // Generate string descriptors
        let mut string_descriptors = HashMap::new();
        let mut custom_strings = HashMap::new();
        let strings = str_alloc.into_inner();
        for (i, s) in strings.into_iter().enumerate() {
            match s {
                UsbString::None => {
                    let mut w = UsbDescriptorWriter::new();
                    // list of supported languages
                    let supported_languages = lang_id::ENGLISH_US.to_le_bytes();
                    w.write(UsbDescriptorType::String as u8, &supported_languages);
                    string_descriptors.insert(i as u8, w.finish());
                }
                UsbString::Const(s) => {
                    let mut w = UsbDescriptorWriter::new();
                    w.string(&s);
                    string_descriptors.insert(i as u8, w.finish());
                }
                UsbString::Custom(id) => {
                    custom_strings.insert(i as u8, id);
                }
            }
        }

        // Generate endpoint list
        let mut endpoints = Vec::new();
        endpoints.push(UsbEndpointDescriptor {
            address: EndpointAddress::from_parts(0, UsbDirection::Out).into(),
            attributes: EndpointType::Control as u8,
            max_packet_size: u16::from(self.descriptor.max_packet_size_0),
            interval: 0,
        });
        endpoints.push(UsbEndpointDescriptor {
            address: EndpointAddress::from_parts(0, UsbDirection::In).into(),
            attributes: EndpointType::Control as u8,
            max_packet_size: u16::from(self.descriptor.max_packet_size_0),
            interval: 0,
        });
        for interface in self.interfaces {
            for endpoint in interface.endpoints {
                endpoints.push(endpoint);
            }
        }

        DeviceConfig {
            device_descriptor,
            configuration_descriptor,
            string_descriptors,
            custom_strings,
            endpoints,
        }
    }
}

#[derive(Clone)]
pub struct InterfaceBuilder {
    pub descriptor: UsbInterfaceDescriptor,
    pub custom_descriptors: Vec<UsbCustomDescriptor>,
    pub endpoints: Vec<UsbEndpointDescriptor>,
}

impl InterfaceBuilder {
    fn new(interface_number: u8) -> Self {
        Self {
            descriptor: UsbInterfaceDescriptor {
                interface_number,
                alternate_setting: 0,
                interface_class: 0,
                interface_sub_class: 0,
                interface_protocol: 0,
                interface_string: UsbString::None,
            },
            custom_descriptors: Vec::new(),
            endpoints: Vec::new(),
        }
    }

    generate_field_setters! {
        alternate_setting: u8,
        interface_class: u8,
        interface_sub_class: u8,
        interface_protocol: u8,
    }

    pub fn descriptor(mut self, descriptor_type: u8, descriptor: &[u8]) -> Self {
        let custom_descriptor = UsbCustomDescriptor {
            descriptor_type,
            data: descriptor.to_vec(),
        };
        self.custom_descriptors.push(custom_descriptor);
        self
    }

    pub fn endpoint(mut self, endpoint: UsbEndpointDescriptor) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    pub fn save(self, device: &mut DeviceBuilder) {
        device.add_interface(self)
    }
}

pub struct EndpointBuilder {
    pub number: Option<u8>,
    pub direction: Option<UsbDirection>,
    pub ep_type: Option<EndpointType>,
    pub max_packet_size: Option<u16>,
    pub interval: u8,
}

impl EndpointBuilder {
    pub fn new() -> Self {
        Self {
            number: None,
            direction: None,
            ep_type: None,
            max_packet_size: None,
            interval: 0,
        }
    }

    pub fn number(mut self, number: u8) -> Self {
        self.number = Some(number);
        self
    }

    pub fn direction(mut self, direction: UsbDirection) -> Self {
        self.direction = Some(direction);
        self
    }

    pub fn ep_type(mut self, ep_type: EndpointType) -> Self {
        self.ep_type = Some(ep_type);
        self
    }

    pub fn max_packet_size(mut self, max_packet_size: u16) -> Self {
        self.max_packet_size = Some(max_packet_size);
        self
    }

    pub fn interval(mut self, interval: u8) -> Self {
        self.interval = interval;
        self
    }

    pub fn build(self) -> UsbEndpointDescriptor {
        UsbEndpointDescriptor {
            address: EndpointAddress::from_parts(
                self.number.unwrap() as usize,
                self.direction.unwrap(),
            )
            .into(),
            attributes: self.ep_type.unwrap() as u8,
            max_packet_size: self.max_packet_size.unwrap(),
            interval: self.interval,
        }
    }
}
