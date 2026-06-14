/// Milliseconds since Unix epoch, matching the on-disk i64 encoding.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub i64);

impl AsRef<i64> for Timestamp {
    fn as_ref(&self) -> &i64 {
        &self.0
    }
}

impl Timestamp {
    // TODO make this a commit option like automerge does

    #[cfg(target_arch = "wasm32")]
    pub fn now() -> Self {
        Self(js_sys::Date::now() as i64)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self(ms)
    }
}
