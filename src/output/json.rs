use serde::Serialize;

pub trait JsonOutput {
    fn to_json(&self) -> Result<String, serde_json::Error>;
    fn to_json_pretty(&self) -> Result<String, serde_json::Error>;
}

impl<T: Serialize> JsonOutput for T {
    fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
