/// Dynamic JSON values cross Tauri's IPC as JSON objects, arrays, and scalars.
/// Keep the wire contract broad without recursively expanding `serde_json::Value`.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct JsonValue(pub serde_json::Value);

impl specta::Type for JsonValue {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <specta_typescript::Unknown as specta::Type>::definition(types)
    }
}

#[cfg(test)]
mod tests {
    use super::JsonValue;

    #[test]
    fn json_value_round_trips_without_changing_the_ipc_shape() {
        let value = serde_json::json!({ "enabled": true, "port": 1080, "bypass": ["localhost"] });
        let wire = JsonValue(value.clone());
        let encoded = serde_json::to_value(&wire).expect("serialize JSON wire value");
        let decoded: JsonValue = serde_json::from_value(encoded.clone()).expect("deserialize JSON wire value");

        assert_eq!(encoded, value);
        assert_eq!(decoded.0, value);
    }
}
