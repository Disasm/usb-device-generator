use usb_device::endpoint::EndpointAddress;

/// Maximum number of endpoints in one direction. Specified by the USB specification.
pub const USB_MAX_ENDPOINTS: usize = 16;

/// Standard descriptor types
pub enum UsbDescriptorType {
    Device = 1,
    Configuration = 2,
    String = 3,
    Interface = 4,
    Endpoint = 5,
}

#[derive(Clone, Debug)]
pub struct UsbDeviceDescriptor {
    pub device_class: u8,
    pub device_sub_class: u8,
    pub device_protocol: u8,
    pub max_packet_size_0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_release: u16,
    pub manufacturer: UsbString,
    pub product: UsbString,
    pub serial_number: UsbString,
}

#[derive(Clone)]
pub struct UsbConfigurationDescriptor {
    pub configuration_value: u8,
    pub configuration_string: UsbString,
    pub attributes: u8,
    pub max_power: u8,
}

#[derive(Clone, Debug)]
pub struct UsbInterfaceDescriptor {
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub interface_class: u8,
    pub interface_sub_class: u8,
    pub interface_protocol: u8,
    pub interface_string: UsbString,
}

#[derive(Clone, Debug)]
pub struct UsbEndpointDescriptor {
    pub address: EndpointAddress,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}

#[derive(Clone, Debug)]
pub struct UsbCustomDescriptor {
    pub descriptor_type: u8,
    pub data: Vec<u8>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum UsbString {
    None,
    Const(String),
    Custom(usize),
}

pub struct UsbStringAllocator {
    strings: Vec<UsbString>,
}

impl UsbStringAllocator {
    pub fn new() -> Self {
        Self {
            strings: vec![UsbString::None],
        }
    }

    pub fn alloc(&mut self, string: &UsbString) -> u8 {
        if let Some(index) = self.get_index(string) {
            index
        } else {
            let index = self.strings.len() as u8;
            self.strings.push(string.clone());
            index
        }
    }

    pub fn get_index(&self, string: &UsbString) -> Option<u8> {
        self.strings
            .iter()
            .enumerate()
            .find(|(_, s)| *s == string)
            .map(|(i, _)| i as u8)
    }

    pub fn into_inner(self) -> Vec<UsbString> {
        self.strings
    }
}

pub struct UsbDescriptorWriter {
    buf: Vec<u8>,
    configuration_offset: Option<usize>,
    num_interfaces_mark: Option<usize>,
    num_endpoints_mark: Option<usize>,
}

impl UsbDescriptorWriter {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            configuration_offset: None,
            num_interfaces_mark: None,
            num_endpoints_mark: None,
        }
    }

    pub fn write(&mut self, descriptor_type: u8, descriptor: &[u8]) {
        let length = descriptor.len();
        self.buf.push((length + 2) as u8);
        self.buf.push(descriptor_type);
        self.buf.extend_from_slice(descriptor);
    }

    fn position(&self) -> usize {
        self.buf.len()
    }

    pub fn custom_descriptor(&mut self, descriptor: &UsbCustomDescriptor) {
        self.write(descriptor.descriptor_type, &descriptor.data);
    }

    pub fn device(
        &mut self,
        device: &UsbDeviceDescriptor,
        num_configurations: u8,
        alloc: &UsbStringAllocator,
    ) {
        self.write(
            UsbDescriptorType::Device as u8,
            &[
                0x00,
                0x02,                     // bcdUSB
                device.device_class,      // bDeviceClass
                device.device_sub_class,  // bDeviceSubClass
                device.device_protocol,   // bDeviceProtocol
                device.max_packet_size_0, // bMaxPacketSize0
                device.vendor_id as u8,
                (device.vendor_id >> 8) as u8, // idVendor
                device.product_id as u8,
                (device.product_id >> 8) as u8, // idProduct
                device.device_release as u8,
                (device.device_release >> 8) as u8, // bcdDevice
                alloc.get_index(&device.manufacturer).unwrap(), // iManufacturer
                alloc.get_index(&device.product).unwrap(), // iProduct
                alloc.get_index(&device.serial_number).unwrap(), // iSerialNumber
                num_configurations,                 // bNumConfigurations
            ],
        );
    }

    pub fn configuration(&mut self, conf: &UsbConfigurationDescriptor, alloc: &UsbStringAllocator) {
        self.update_configuration_length();
        self.configuration_offset = Some(self.position());
        self.num_interfaces_mark = Some(self.position() + 4);

        self.write(
            UsbDescriptorType::Configuration as u8,
            &[
                0,
                0,                                                    // wTotalLength
                0,                                                    // bNumInterfaces
                conf.configuration_value,                             // bConfigurationValue
                alloc.get_index(&conf.configuration_string).unwrap(), // iConfiguration
                conf.attributes,                                      // bmAttributes
                conf.max_power,                                       // bMaxPower
            ],
        );
    }

    fn update_configuration_length(&mut self) {
        if let Some(offset) = self.configuration_offset {
            let length = self.position() as u16 - offset as u16;
            self.buf[offset + 2..offset + 4].copy_from_slice(&length.to_le_bytes());
        }
    }

    pub fn interface(&mut self, interface: &UsbInterfaceDescriptor, alloc: &UsbStringAllocator) {
        self.buf[self.num_interfaces_mark.unwrap()] += 1;

        self.num_endpoints_mark = Some(self.position() + 4);

        self.write(
            UsbDescriptorType::Interface as u8,
            &[
                interface.interface_number,                            // bInterfaceNumber
                interface.alternate_setting,                           // bAlternateSetting
                0,                                                     // bNumEndpoints
                interface.interface_class,                             // bInterfaceClass
                interface.interface_sub_class,                         // bInterfaceSubClass
                interface.interface_protocol,                          // bInterfaceProtocol
                alloc.get_index(&interface.interface_string).unwrap(), // iInterface
            ],
        );
    }

    pub fn endpoint(&mut self, endpoint: &UsbEndpointDescriptor) {
        self.buf[self.num_endpoints_mark.unwrap()] += 1;

        let mps = endpoint.max_packet_size;

        self.write(
            UsbDescriptorType::Endpoint as u8,
            &[
                endpoint.address.into(), // bEndpointAddress
                endpoint.attributes, // bmAttributes
                mps as u8,
                (mps >> 8) as u8,  // wMaxPacketSize
                endpoint.interval, // bInterval
            ],
        );
    }

    pub fn string(&mut self, string: &str) {
        let mut buf = Vec::new();
        string
            .encode_utf16()
            .for_each(|c| buf.extend_from_slice(&c.to_le_bytes()));
        self.write(UsbDescriptorType::String as u8, &buf);
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.update_configuration_length();
        self.buf
    }
}
