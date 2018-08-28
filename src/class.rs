use ::Result;
pub use device::{ControlOutResult, ControlInResult};
pub use descriptor::DescriptorWriter;
use control;

pub trait UsbClass {
    fn reset(&self) -> Result<()> {
        Ok(())
    }

    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        let _ = writer;
        Ok (())
    }

    fn control_out(&self, req: &control::Request, data: &[u8]) -> ControlOutResult {
        let _ = (req, data);
        ControlOutResult::Ignore
    }

    fn control_in(&self, req: &control::Request, data: &mut [u8]) -> ControlInResult {
        let _ = (req, data);
        ControlInResult::Ignore
    }

    fn endpoint_out(&self, addr: u8) {
        let _ = addr;
    }

    fn endpoint_in_complete(&self, addr: u8) {
        let _ = addr;
    }
}