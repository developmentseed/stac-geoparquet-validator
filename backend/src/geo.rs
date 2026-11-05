//! GeoParquet `geo` metadata and native Parquet geospatial types.

use crate::Context;
use parquet::basic::LogicalType;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::BTreeMap;

/// The parsed `geo` file metadata.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GeoMetadata {
    /// The GeoParquet version.
    pub version: String,
    /// The name of the primary geometry column.
    pub primary_column: String,
    /// Metadata for each geometry column, by name.
    pub columns: BTreeMap<String, GeoColumn>,
}

/// Metadata for a single geometry column.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GeoColumn {
    /// The geometry encoding, e.g. `WKB`.
    pub encoding: String,
    /// The declared geometry types.
    pub geometry_types: Vec<String>,
    /// The PROJJSON crs: `None` when the key is absent, `Some(None)` when it is `null`.
    #[serde(default, deserialize_with = "deserialize_present")]
    pub crs: Option<Option<Value>>,
    /// The covering object, if any.
    #[serde(default)]
    pub covering: Option<Value>,
    /// The column's bounding box, if any.
    #[serde(default)]
    pub bbox: Option<Vec<f64>>,
}

fn deserialize_present<'de, D>(deserializer: D) -> Result<Option<Option<Value>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<Value>::deserialize(deserializer).map(Some)
}

/// The state of a file's `geo` metadata.
#[derive(Debug, Clone, PartialEq)]
pub enum GeoState {
    /// There is no `geo` key.
    Missing,
    /// The `geo` value is not JSON.
    InvalidJson,
    /// The `geo` value is JSON, but does not have the expected shape.
    Invalid(Value),
    /// The `geo` value was parsed.
    Valid(GeoMetadata),
}

impl GeoState {
    pub(crate) fn parse(geo: Option<&str>) -> GeoState {
        let Some(geo) = geo else {
            return GeoState::Missing;
        };
        let Ok(value) = serde_json::from_str::<Value>(geo) else {
            return GeoState::InvalidJson;
        };
        match GeoMetadata::deserialize(&value) {
            Ok(metadata) => GeoState::Valid(metadata),
            Err(_) => GeoState::Invalid(value),
        }
    }
}

/// A root column with a native Parquet `GEOMETRY` or `GEOGRAPHY` logical type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeGeo {
    /// The logical type's `crs` property. A `projjson:<key>` reference is resolved to the
    /// referenced file metadata value when that key exists.
    pub crs: Option<String>,
    /// True for `GEOGRAPHY`, false for `GEOMETRY`.
    pub geography: bool,
}

impl Context {
    /// Returns the state of the file's `geo` metadata, parsed once.
    pub fn geo(&self) -> GeoState {
        self.geo
            .get_or_init(|| GeoState::parse(self.key_value.get("geo").map(String::as_str)))
            .clone()
    }

    /// Returns the declared GeoParquet version, if the `geo` metadata parsed.
    pub fn geoparquet_version(&self) -> Option<String> {
        match self.geo() {
            GeoState::Valid(metadata) => Some(metadata.version),
            _ => None,
        }
    }

    /// Returns the root columns that use native Parquet geospatial logical types.
    pub fn native_geo_columns(&self) -> Vec<(String, NativeGeo)> {
        self.metadata
            .file_metadata()
            .schema_descr()
            .root_schema()
            .get_fields()
            .iter()
            .filter(|field| field.is_primitive())
            .filter_map(|field| {
                let (crs, geography) = match field.get_basic_info().logical_type_ref()? {
                    LogicalType::Geometry(geometry) => (geometry.crs.clone(), false),
                    LogicalType::Geography(geography) => (geography.crs.clone(), true),
                    _ => return None,
                };
                let crs = crs.map(|crs| self.resolve_projjson_reference(crs));
                Some((field.name().to_string(), NativeGeo { crs, geography }))
            })
            .collect()
    }

    fn resolve_projjson_reference(&self, crs: String) -> String {
        crs.strip_prefix("projjson:")
            .and_then(|key| self.key_value.get(key))
            .cloned()
            .unwrap_or(crs)
    }
}

/// Returns true if both crs values describe OGC:CRS84 (or the equivalent EPSG:4326).
///
/// An absent value counts as OGC:CRS84. A `geo` crs of `null` does not. A PROJJSON crs
/// matches when its `id` is `OGC`/`CRS84` or `EPSG`/`4326`, and a native crs string
/// matches when it is `OGC:CRS84`, `EPSG:4326`, or such a PROJJSON object.
pub fn is_crs84(geo_crs: Option<&Option<Value>>, native_crs: Option<&str>) -> bool {
    let geo = match geo_crs {
        None => true,
        Some(None) => false,
        Some(Some(value)) => value_is_crs84(value),
    };
    let native = match native_crs {
        None => true,
        Some(crs) => string_is_crs84(crs),
    };
    geo && native
}

fn string_is_crs84(crs: &str) -> bool {
    match crs {
        "OGC:CRS84" | "EPSG:4326" => true,
        _ => serde_json::from_str::<Value>(crs)
            .map(|value| value.is_object() && value_is_crs84(&value))
            .unwrap_or(false),
    }
}

fn value_is_crs84(value: &Value) -> bool {
    match value {
        Value::String(crs) => string_is_crs84(crs),
        Value::Object(_) => {
            let id = &value["id"];
            let code = match &id["code"] {
                Value::String(code) => code.clone(),
                Value::Number(code) => code.to_string(),
                _ => return false,
            };
            matches!(
                (id["authority"].as_str(), code.as_str()),
                (Some("OGC"), "CRS84") | (Some("EPSG"), "4326")
            )
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn absent_is_crs84() {
        assert!(super::is_crs84(None, None));
    }

    #[test]
    fn null_is_not_crs84() {
        assert!(!super::is_crs84(Some(&None), None));
    }

    #[test]
    fn projjson_ids() {
        for id in [
            json!({"authority": "OGC", "code": "CRS84"}),
            json!({"authority": "EPSG", "code": 4326}),
            json!({"authority": "EPSG", "code": "4326"}),
        ] {
            assert!(super::is_crs84(Some(&Some(json!({ "id": id }))), None));
        }
        let utm = json!({"id": {"authority": "EPSG", "code": 32633}});
        assert!(!super::is_crs84(Some(&Some(utm)), None));
    }

    #[test]
    fn native_strings() {
        assert!(super::is_crs84(None, Some("OGC:CRS84")));
        assert!(super::is_crs84(None, Some("EPSG:4326")));
        assert!(super::is_crs84(
            None,
            Some(r#"{"id":{"authority":"OGC","code":"CRS84"}}"#)
        ));
        assert!(!super::is_crs84(None, Some("EPSG:32633")));
        assert!(!super::is_crs84(None, Some("srid:0")));
    }
}
