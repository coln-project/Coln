use std::fmt;

use wasm_bindgen::JsValue;

pub(crate) fn set_panic_hook() {
    console_error_panic_hook::set_once();
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryError {
    message: String,
}

impl BoundaryError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BoundaryError {}

pub(crate) fn js_error(error: impl fmt::Display) -> JsValue {
    js_sys::Error::new(&error.to_string()).into()
}
