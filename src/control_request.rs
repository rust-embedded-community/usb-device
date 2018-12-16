use ::{Result, UsbError};
use core::mem;

/// Control request direction.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Direction {
    /// Host-to-device direction (control OUT transfer)
    HostToDevice = 0,
    /// Device-to-host direction (control IN transfer)
    DeviceToHost = 1,
}

/// Control request type.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RequestType {
    /// Request is a USB standard request. Usually handled by [`UsbDevice`](::device::UsbDevice).
    Standard = 0,
    /// Request is intended for a USB class.
    Class = 1,
    /// Request is vendor-specific.
    Vendor = 2,
    /// Reserved.
    Reserved = 3,
}

/// Control request recipient.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Recipient {
    /// Request is intended for the entire device.
    Device = 0,
    /// Request is intended for an interface. Generally, the `index` field of the reques specifies
    /// the interface number.
    Interface = 1,
    /// Request is intended for an endpoint. Generally, the `index` field of the request specifies
    /// the endpoint address.
    Endpoint = 2,
    /// None of the above.
    Other = 3,
    /// Reserved.
    Reserved = 4,
}

/// A control request read from a SETUP packet.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Request {
    /// Direction of the request.
    pub direction: Direction,
    /// Type of the request.
    pub request_type: RequestType,
    /// Recipient of the request.
    pub recipient: Recipient,
    /// Request code. The meaning of the value depends on the previous fields.
    pub request: u8,
    /// Request value. The meaning of the value depends on the previous fields.
    pub value: u16,
    /// Request index. The meaning of the value depends on the previous fields.
    pub index: u16,
    /// Length of the DATA stage. For control OUT transfers this is the exact length of the data the
    /// host sent. For control IN transfers this is the maximum length of data the device should
    /// return.
    pub length: u16,
}

impl Request {
    pub const GET_STATUS: u8 = 0;
    pub const CLEAR_FEATURE: u8 = 1;
    pub const SET_FEATURE: u8 = 3;
    pub const SET_ADDRESS: u8 = 5;
    pub const GET_DESCRIPTOR: u8 = 6;
    pub const SET_DESCRIPTOR: u8 = 7;
    pub const GET_CONFIGURATION: u8 = 8;
    pub const SET_CONFIGURATION: u8 = 9;
    pub const GET_INTERFACE: u8 = 10;
    pub const SET_INTERFACE: u8 = 11;
    pub const SYNCH_FRAME: u8 = 12;

    pub(crate) fn parse(buf: &[u8]) -> Result<Request> {
        if buf.len() != 8 {
            return Err(UsbError::InvalidSetupPacket);
        }

        let rt = buf[0];
        let recipient = rt & 0b11111;

        Ok(Request {
            direction: unsafe { mem::transmute(rt >> 7) },
            request_type: unsafe { mem::transmute((rt >> 5) & 0b11) },
            recipient:
                if recipient <= 3 { unsafe { mem::transmute(recipient) } }
                else { Recipient::Reserved },
            request: buf[1],
            value: (buf[2] as u16) | ((buf[3] as u16) << 8),
            index: (buf[4] as u16) | ((buf[5] as u16) << 8),
            length: (buf[6] as u16) | ((buf[7] as u16) << 8),
        })
    }
}
