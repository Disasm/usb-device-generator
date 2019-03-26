use crate::builder::{DeviceConfig, EndpointBuilder};
use std::{fmt, fs};
use std::io::Write;
use std::fmt::Display;
use failure::Error;
use std::path::Path;
use crate::usb::UsbEndpointDescriptor;
use crate::EndpointInfo;
use crate::endpoint::TargetDeviceConfiguration;

struct TargetDeviceConfig {
    usb_config: DeviceConfig,
    device_config: TargetDeviceConfiguration,
}

impl TargetDeviceConfig {
    fn write_blob(&self, f: &mut fmt::Formatter, const_name: &str, blob: &[u8]) -> fmt::Result {
        write!(f, "const {}: [u8; {}] = [", const_name, blob.len())?;
        for b in blob {
            write!(f, "0x{:02x}, ", b)?;
        }
        writeln!(f, "];")?;
        Ok(())
    }

    fn write_descriptor_information(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{}", r#"
pub struct GeneratedDevice;

use ::usb_device::{Result, bus::UsbBus, device::{DescriptorProvider, CustomStringDescriptorProvider}, class::ControlIn};
impl<B: UsbBus> DescriptorProvider<B> for GeneratedDevice {
    fn get_device_descriptor() -> &'static [u8] {
        &DEVICE_DESCRIPTOR
    }

    fn get_configuration_descriptor() -> &'static [u8] {
        &CONFIGURATION_DESCRIPTOR
    }

    fn get_string_descriptor(_lang_id: u16, index: u8, xfer: ControlIn<B>) -> Result<()> {
        match index {"#
        )?;
        for (id, _descriptor) in &self.usb_config.string_descriptors {
            let name = format!("STRING_DESCRIPTOR_{}", id);
            writeln!(f, "{} => xfer.accept_with(&{}),", id, name)?;
        }
        for (id, index) in &self.usb_config.custom_strings {
            writeln!(f, "{} => <Self as CustomStringDescriptorProvider<B>>::get_custom_string_descriptor({}, xfer),", id, index)?;
        }

        writeln!(f, "{}", r#"
            _ => xfer.reject(),
        }
    }
}"#
        )?;

        if self.usb_config.custom_strings.is_empty() {
            writeln!(f, "impl<B: UsbBus> CustomStringDescriptorProvider<B> for GeneratedDevice {{}}")?;
        }
        Ok(())
    }

    fn write_endpoint_configuration(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{}", r#"
use ::stm32f103xx_usb::endpoint::{Endpoint, EndpointConfiguration};
use ::usb_device::endpoint::EndpointType;
impl EndpointConfiguration for GeneratedDevice {
    fn configure_endpoints(endpoints: &mut [Endpoint]) {"#
        )?;

        for (i, ep) in self.device_config.endpoints.iter().enumerate() {
            let prefix = format!("endpoints[{}]", i);

            assert_eq!(i, ep.ep_address as usize); // TODO: set endpoint address
            writeln!(f, "{}.set_ep_type(EndpointType::{:?});", prefix, ep.ep_type)?;

            if !ep.double_buffered {
                if ep.buffer0_size_words != 0 {
                    writeln!(f, "{}.set_in_buf(0x{:x}, 0x{:x});", prefix,
                             ep.buffer0_offset_words << 1,
                             ep.buffer0_size_words << 1)?;
                }
                if ep.buffer1_size_words != 0 {
                    writeln!(f, "{}.set_out_buf(0x{:x}, (0x{:x}, 0x{:x}));", prefix,
                             ep.buffer1_offset_words << 1,
                             ep.buffer1_size_words << 1,
                             ep.buffer_descriptor_data[3])?;
                }
            } else {
                panic!("Double-buffered endpoints are not supported yet");
            }

            writeln!(f)?;
        }

        writeln!(f, "{}", r#"
    }
}"#
        )?;
        Ok(())
    }
}

impl Display for TargetDeviceConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "mod generated {{")?;
        self.write_blob(f, "DEVICE_DESCRIPTOR", &self.usb_config.device_descriptor)?;
        self.write_blob(f, "CONFIGURATION_DESCRIPTOR", &self.usb_config.configuration_descriptor)?;
        for (id, descriptor) in &self.usb_config.string_descriptors {
            let name = format!("STRING_DESCRIPTOR_{}", id);
            self.write_blob(f, &name, &descriptor)?;
        }
        self.write_descriptor_information(f)?;
        self.write_endpoint_configuration(f)?;
        writeln!(f, "}}")?; // mod generated
        Ok(())
    }
}

pub fn generate_file(filename: impl AsRef<Path>, usb_config: DeviceConfig, device_config: TargetDeviceConfiguration) -> Result<(), Error> {
    let mut file = fs::File::create(filename)?;
    let config = TargetDeviceConfig {
        usb_config,
        device_config,
    };
    write!(file, "{}", config)?;
    Ok(())
}

pub struct DeviceEndpoint {
    descriptor: UsbEndpointDescriptor,
}

impl EndpointInfo for DeviceEndpoint {
    fn descriptor(&self) -> &UsbEndpointDescriptor {
        &self.descriptor
    }
}

impl From<EndpointBuilder> for DeviceEndpoint {
    fn from(builder: EndpointBuilder) -> Self {
        DeviceEndpoint {
            descriptor: builder.build()
        }
    }
}
