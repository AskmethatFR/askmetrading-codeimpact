use super::code_location::CodeLocation;

/// Warning produced when an I/O call is detected inside a loop.
#[derive(Clone, Debug, PartialEq)]
pub struct IoInLoopWarning {
    pub function: String,
    pub io_call: String,
    pub location: CodeLocation,
}