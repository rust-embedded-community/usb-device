use core::mem;
use ::{Result, UsbError};

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Direction {
    HostToDevice = 0,
    DeviceToHost = 1,
}

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RequestType {
    Standard = 0,
    Class = 1,
    Vendor = 2,
    Reserved = 3,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Recipient {
    Device = 0,
    Interface = 1,
    Endpoint = 2,
    Other = 3,
    Reserved = 4,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Request {
    pub direction: Direction,
    pub request_type: RequestType,
    pub recipient: Recipient,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl Request {
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

// TODO: Maybe move parsing standard requests here altogether

pub mod standard_request {
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
}
