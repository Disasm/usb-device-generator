use usb_device::UsbDirection;
use failure::{Error, bail, err_msg};
use usb_device::endpoint::EndpointType;
use crate::builder::{EndpointBuilder, DeviceBuilder};
use crate::usb::USB_MAX_ENDPOINTS;

pub fn calculate_count_rx(mut size: u16) -> Result<(u16, u16), Error> {
    if size <= 62 {
        // Buffer size is in units of 2 bytes, 0 = 0 bytes
        size = (size + 1) & !0x01;

        let size_bits = size >> 1;

        Ok((size, (size_bits << 10) as u16))
    } else if size <= 1024 {
        // Buffer size is in units of 32 bytes, 0 = 32 bytes
        size = (size + 31) & !0x1f;

        let size_bits = (size >> 5) - 1;

        Ok((size, (0x8000 | (size_bits << 10)) as u16))
    } else {
        bail!("Invalid size")
    }
}

struct EndpointMemoryAllocation {
    address: u16,
    size: u16,
}

const BUFFER_TX: usize = 0;
const BUFFER_RX: usize = 1;
struct EndpointAllocation {
    address_index: u8,
    ep_type: EndpointType,
    tx_enabled: bool,
    rx_enabled: bool,
    double_buffered: bool,
    buffer_descriptor: EndpointMemoryAllocation,
    buffers: [Option<EndpointMemoryAllocation>; 2],
}

impl EndpointAllocation {
    fn has_direction(&self, direction: UsbDirection) -> bool {
        self.tx_enabled && direction == UsbDirection::In ||
        self.rx_enabled && direction == UsbDirection::Out
    }

    fn has_space(&self, ep_type: EndpointType, direction: UsbDirection) -> bool {
        if self.ep_type != ep_type {
            false
        } else if self.double_buffered {
            false
        } else {
            !self.has_direction(direction)
        }
    }
}

pub struct DeviceAllocator {
    endpoints: Vec<EndpointAllocation>,
    start_address: u16,
    end_address: u16,
}

const DEVICE_ENDPOINT_COUNT: usize = 8;
const ENDPOINT_MEMORY_SIZE: u16 = 512;

impl DeviceAllocator {
    pub fn new() -> DeviceAllocator {
        Self {
            endpoints: Vec::new(),
            start_address: 0,
            end_address: ENDPOINT_MEMORY_SIZE,
        }
    }

    fn allocate_endpoint_buffer(&mut self, size: u16) -> Result<EndpointMemoryAllocation, Error> {
        let size = (size + 1) & !0x01;
        if size <= (self.end_address - self.start_address) {
            self.end_address -= size;
            let address = self.end_address;
            Ok(EndpointMemoryAllocation {
                address,
                size,
            })
        } else {
            bail!("Can't allocate endpoint buffer: not enough space");
        }
    }

    fn allocate_buffer_descriptor(&mut self) -> Result<EndpointMemoryAllocation, Error> {
        assert_eq!(self.start_address % 8, 0);
        let size = 8;
        if size <= (self.end_address - self.start_address) {
            let address = self.start_address;
            self.start_address += size;
            Ok(EndpointMemoryAllocation {
                address,
                size,
            })
        } else {
            bail!("Can't allocate buffer descriptor: not enough space")
        }
    }

    fn allocate_rx_buffer(&mut self, max_packet_size: u16) -> Result<EndpointMemoryAllocation, Error> {
        let (size, _) = calculate_count_rx(max_packet_size)?;
        self.allocate_endpoint_buffer(size)
    }

    fn allocate_tx_buffer(&mut self, max_packet_size: u16) -> Result<EndpointMemoryAllocation, Error> {
        self.allocate_endpoint_buffer(max_packet_size)
    }

    fn get_free_address_index(&self) -> Result<u8, Error> {
        for index in 1..USB_MAX_ENDPOINTS {
            if !self.endpoints.iter().any(|ep| ep.address_index == index as u8) {
                return Ok(index as u8);
            }
        }
        bail!("All endpoint addressees are already allocated")
    }

    fn allocate_empty_endpoint(&mut self, ep_type: EndpointType) -> Result<usize, Error> {
        if self.endpoints.len() < DEVICE_ENDPOINT_COUNT {
            let address_index = self.get_free_address_index()?;
            let buffer_descriptor = self.allocate_buffer_descriptor()?;
            let ep = EndpointAllocation {
                address_index,
                ep_type,
                tx_enabled: false,
                rx_enabled: false,
                double_buffered: false,
                buffer_descriptor,
                buffers: [None, None],
            };
            let i = self.endpoints.len();
            self.endpoints.push(ep);
            Ok(i)
        } else {
            bail!("Can't allocate endpoint");
        }
    }

    fn allocate_from_builder(&mut self, builder: EndpointBuilder, double_buffered: bool) -> Result<EndpointBuilder, Error> {
        let ep_type = builder.ep_type.ok_or_else(|| err_msg("Endpoint type is not set"))?;
        let direction = builder.direction.ok_or_else(|| err_msg("Endpoint direction is not set"))?;
        let max_packet_size = builder.max_packet_size.ok_or_else(|| err_msg("Max packet size is not set"))?;

        let ep_index;
        if let Some(address_index) = builder.number {
            if let Some((i, ep)) = self.endpoints.iter().enumerate().find(|(_, ep)| ep.address_index == address_index) {
                if double_buffered || ep.double_buffered || ep.has_direction(direction) {
                    bail!("Endpoint with given address is already exists");
                }
                ep_index = i;
            } else {
                let i = self.allocate_empty_endpoint(ep_type)?;
                self.endpoints[i].address_index = address_index;
                ep_index = i;
            }
        } else {
            if let Some((i, _)) = self.endpoints.iter().enumerate().find(|(_, ep)| ep.has_space(ep_type, direction)) {
                ep_index = i;
            } else {
                ep_index = self.allocate_empty_endpoint(ep_type)?;
            }
        }

        if double_buffered {
            let buf0;
            let buf1;
            if direction == UsbDirection::In {
                buf0 = self.allocate_tx_buffer(max_packet_size)?;
                buf1 = self.allocate_tx_buffer(max_packet_size)?;
            } else {
                buf0 = self.allocate_rx_buffer(max_packet_size)?;
                buf1 = self.allocate_rx_buffer(max_packet_size)?;
            }

            let ep = &mut self.endpoints[ep_index];
            ep.tx_enabled = direction == UsbDirection::In;
            ep.rx_enabled = direction == UsbDirection::Out;
            ep.double_buffered = true;
            ep.buffers[0] = Some(buf0);
            ep.buffers[1] = Some(buf1);
        } else {
            let buffer = if direction == UsbDirection::In {
                self.allocate_tx_buffer(max_packet_size)?
            } else {
                self.allocate_rx_buffer(max_packet_size)?
            };

            let ep = &mut self.endpoints[ep_index];
            if direction == UsbDirection::In {
                ep.tx_enabled = true;
                ep.buffers[BUFFER_TX] = Some(buffer);
            } else {
                ep.rx_enabled = true;
                ep.buffers[BUFFER_RX] = Some(buffer);
            }
        }

        Ok(if builder.number.is_none() {
            builder.number(self.endpoints[ep_index].address_index)
        } else {
            builder
        })
    }

    fn allocate_ep0_from_builfer(&mut self, builder: DeviceBuilder) -> Result<DeviceBuilder, Error> {
        let max_packet_size = builder.descriptor.max_packet_size_0 as u16;

        if self.endpoints.iter().any(|ep| ep.address_index == 0) {
            bail!("Endpoint 0 is already allocated!");
        }
        let buffer_descriptor = self.allocate_buffer_descriptor()?;
        let buffer_tx = self.allocate_tx_buffer(max_packet_size)?;
        let buffer_rx = self.allocate_rx_buffer(max_packet_size)?;
        let ep = EndpointAllocation {
            address_index: 0,
            ep_type: EndpointType::Control,
            tx_enabled: true,
            rx_enabled: true,
            double_buffered: false,
            buffer_descriptor,
            buffers: [Some(buffer_tx), Some(buffer_rx)],
        };
        self.endpoints.push(ep);
        Ok(builder)
    }
}

pub trait EndpointBuilderEx {
    fn allocate(self, allocator: &mut DeviceAllocator) -> Self;

    fn allocate_double_buffered(self, allocator: &mut DeviceAllocator) -> Self;
}

impl EndpointBuilderEx for EndpointBuilder {
    fn allocate(self, allocator: &mut DeviceAllocator) -> Self {
        allocator.allocate_from_builder(self, false).unwrap()
    }

    fn allocate_double_buffered(self, allocator: &mut DeviceAllocator) -> Self {
        allocator.allocate_from_builder(self, true).unwrap()
    }
}

pub trait DeviceBuilderEx {
    fn allocate(self, allocator: &mut DeviceAllocator) -> Self;
}

impl DeviceBuilderEx for DeviceBuilder {
    fn allocate(self, allocator: &mut DeviceAllocator) -> Self {
        allocator.allocate_ep0_from_builfer(self).unwrap()
    }
}
