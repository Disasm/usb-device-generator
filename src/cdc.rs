use crate::builder::DeviceBuilder;
use crate::EndpointInfo;

pub const USB_CLASS_CDC: u8 = 0x02;
const USB_CLASS_DATA: u8 = 0x0a;
const CDC_SUBCLASS_ACM: u8 = 0x02;
const CDC_PROTOCOL_AT: u8 = 0x01;

const CS_INTERFACE: u8 = 0x24;
const CDC_TYPE_HEADER: u8 = 0x00;
const CDC_TYPE_CALL_MANAGEMENT: u8 = 0x01;
const CDC_TYPE_ACM: u8 = 0x02;
const CDC_TYPE_UNION: u8 = 0x06;

pub fn create_cdc_function(device: &mut DeviceBuilder, comm_ep: impl EndpointInfo, read_ep: impl EndpointInfo, write_ep: impl EndpointInfo) {
    let comm_if = device.alloc_interface();
    let data_if = device.alloc_interface();
    let comm_if_id = comm_if.descriptor.interface_number;
    let data_if_id = data_if.descriptor.interface_number;

    comm_if
        .interface_class(USB_CLASS_CDC)
        .interface_sub_class(CDC_SUBCLASS_ACM)
        .interface_protocol(CDC_PROTOCOL_AT)
        .descriptor(CS_INTERFACE, &[CDC_TYPE_HEADER, 0x10, 0x01])
        .descriptor(CS_INTERFACE, &[CDC_TYPE_CALL_MANAGEMENT, 0x00, data_if_id])
        .descriptor(CS_INTERFACE, &[CDC_TYPE_ACM, 0x00])
        .descriptor(CS_INTERFACE, &[CDC_TYPE_UNION, comm_if_id, data_if_id])
        .endpoint(comm_ep.descriptor().clone())
        .save(device);

    data_if
        .interface_class(USB_CLASS_DATA)
        .endpoint(write_ep.descriptor().clone())
        .endpoint(read_ep.descriptor().clone())
        .save(device);
}
