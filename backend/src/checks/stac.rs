use crate::{
    Context, Finding, Section, Severity,
    geo::{self, GeoState},
};
use arrow_schema::{DataType, Field};
use jsonschema::{Retrieve, Uri, Validator};
use serde_json::{Value, json};
use std::{error::Error, sync::LazyLock};

const MAX_SCHEMA_ERRORS: usize = 20;
const REQUIRED_COLUMNS: [&str; 6] = [
    "stac_extensions",
    "id",
    "geometry",
    "bbox",
    "links",
    "assets",
];
const DATETIME_COLUMNS: [&str; 8] = [
    "datetime",
    "start_datetime",
    "end_datetime",
    "created",
    "updated",
    "expires",
    "published",
    "unpublished",
];
const BBOX_2D: [&str; 4] = ["xmin", "ymin", "xmax", "ymax"];
const BBOX_3D: [&str; 6] = ["xmin", "ymin", "zmin", "xmax", "ymax", "zmax"];
const SUPPORTED_VERSIONS: [&str; 2] = ["1.0.0", "1.1.0"];

const REF_GUIDELINES: &str = "https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#guidelines";
const REF_LINK: &str = "https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#link-struct";
const REF_ASSET: &str = "https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#asset-struct";
const REF_PARQUET_METADATA: &str = "https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#parquet-metadata";
const REF_COLLECTIONS: &str = "https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#stac-collection-objects";

struct PermissiveRetriever;

impl Retrieve for PermissiveRetriever {
    fn retrieve(&self, _: &Uri<String>) -> Result<Value, Box<dyn Error + Send + Sync>> {
        Ok(json!({"type": "object"}))
    }
}

static SCHEMA: LazyLock<Validator> = LazyLock::new(|| {
    let mut schema: Value =
        serde_json::from_str(include_str!("../../schemas/stac-geoparquet-metadata.json"))
            .expect("vendored schema is valid JSON");
    if let Some(version) = schema["properties"]["version"].as_object_mut() {
        version.remove("const");
    }
    jsonschema::options()
        .with_retriever(PermissiveRetriever)
        .build(&schema)
        .expect("vendored schema compiles")
});

fn finding(
    id: &'static str,
    severity: Severity,
    message: String,
    reference: &'static str,
) -> Finding {
    Finding::new(id, severity, Section::StacGeoparquet, message, reference)
}

fn error(id: &'static str, message: String, reference: &'static str) -> Finding {
    finding(id, Severity::Error, message, reference)
}

fn is_string(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
    )
}

fn is_float(data_type: &DataType) -> bool {
    matches!(data_type, DataType::Float32 | DataType::Float64)
}

fn struct_child<'a>(data_type: &'a DataType, name: &str) -> Option<&'a Field> {
    match data_type {
        DataType::Struct(fields) => fields.iter().find(|f| f.name() == name).map(|f| f.as_ref()),
        _ => None,
    }
}

fn list_item(data_type: &DataType) -> Option<&Field> {
    match data_type {
        DataType::List(item) | DataType::LargeList(item) => Some(item),
        _ => None,
    }
}

fn type_problem(name: &str, data_type: &DataType) -> Option<(String, &'static str)> {
    match name {
        "stac_extensions" => {
            let ok = list_item(data_type).is_some_and(|item| is_string(item.data_type()));
            (!ok).then(|| ("must be a list of strings".to_string(), REF_GUIDELINES))
        }
        "id" => (!is_string(data_type)).then(|| ("must be a string".to_string(), REF_GUIDELINES)),
        "bbox" => {
            let ok = match data_type {
                DataType::Struct(fields) => {
                    let names: Vec<&str> = fields.iter().map(|f| f.name().as_str()).collect();
                    fields.iter().all(|f| is_float(f.data_type()))
                        && (names == BBOX_2D || names == BBOX_3D)
                }
                _ => false,
            };
            (!ok).then(|| {
                (
                    "must be a struct of floats named xmin, ymin, [zmin,] xmax, ymax, [zmax]"
                        .to_string(),
                    REF_GUIDELINES,
                )
            })
        }
        "links" => {
            let ok = list_item(data_type).is_some_and(|item| {
                ["href", "rel"].iter().all(|child| {
                    struct_child(item.data_type(), child).is_some_and(|f| is_string(f.data_type()))
                })
            });
            (!ok).then(|| {
                (
                    "must be a list of structs with string `href` and `rel` fields".to_string(),
                    REF_LINK,
                )
            })
        }
        "assets" => {
            let ok = match data_type {
                DataType::Struct(fields) => fields
                    .iter()
                    .all(|asset| struct_child(asset.data_type(), "href").is_some()),
                _ => false,
            };
            (!ok).then(|| {
                (
                    "must be a struct whose children are structs with an `href` field".to_string(),
                    REF_ASSET,
                )
            })
        }
        name if DATETIME_COLUMNS.contains(&name) => {
            (!matches!(data_type, DataType::Timestamp(_, _)))
                .then(|| ("must be a timestamp".to_string(), REF_GUIDELINES))
        }
        _ => None,
    }
}

/// Checks the root columns of the Arrow schema against the stac-geoparquet spec.
pub(super) fn columns(context: &Context) -> Vec<Finding> {
    let schema = &context.arrow_schema;
    let mut findings = Vec::new();
    for name in REQUIRED_COLUMNS {
        if schema.field_with_name(name).is_err() {
            findings.push(error(
                "stac.column_missing",
                format!("the required column `{name}` is missing"),
                REF_GUIDELINES,
            ));
        }
    }
    for field in schema.fields() {
        if let Some((problem, reference)) = type_problem(field.name(), field.data_type()) {
            findings.push(error(
                "stac.column_type",
                format!(
                    "the column `{}` has type {}, which {problem}",
                    field.name(),
                    field.data_type()
                ),
                reference,
            ));
        }
    }
    if schema.field_with_name("type").is_ok() {
        findings.push(finding(
            "stac.type_column",
            Severity::Warning,
            "the `type` column is redundant and should be omitted".to_string(),
            REF_GUIDELINES,
        ));
    }
    findings
}

/// Checks that the file has `geo` metadata, which stac-geoparquet requires even when the
/// geometry uses native Parquet geospatial types.
pub(super) fn geoparquet_metadata(context: &Context) -> Vec<Finding> {
    if context.key_value.contains_key("geo") {
        return Vec::new();
    }
    vec![error(
        "stac.geoparquet_metadata_missing",
        "stac-geoparquet files must be valid GeoParquet with `geo` metadata, but the file has none"
            .to_string(),
        REF_GUIDELINES,
    )]
}

/// Checks that the `geometry` column's CRS is OGC:CRS84.
pub(super) fn crs(context: &Context) -> Vec<Finding> {
    if context.arrow_schema.field_with_name("geometry").is_err() {
        return Vec::new();
    }
    let geo_crs = match context.geo() {
        GeoState::Valid(metadata) => metadata.columns.get("geometry").and_then(|c| c.crs.clone()),
        _ => None,
    };
    let natives = context.native_geo_columns();
    let native_crs = natives
        .iter()
        .find(|(name, _)| name == "geometry")
        .and_then(|(_, native)| native.crs.as_deref());
    if geo::is_crs84(geo_crs.as_ref(), native_crs) {
        Vec::new()
    } else {
        vec![error(
            "stac.crs",
            "the `geometry` column's CRS is not OGC:CRS84".to_string(),
            REF_GUIDELINES,
        )]
    }
}

/// Checks the `stac-geoparquet` file metadata.
pub(super) fn metadata(context: &Context) -> Vec<Finding> {
    let Some(raw) = context.key_value.get("stac-geoparquet") else {
        return vec![error(
            "stac.metadata_missing",
            "the file has no `stac-geoparquet` metadata".to_string(),
            REF_PARQUET_METADATA,
        )];
    };
    let value: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(err) => {
            return vec![error(
                "stac.metadata_schema",
                format!("the `stac-geoparquet` metadata is not valid JSON: {err}"),
                REF_PARQUET_METADATA,
            )];
        }
    };
    let mut findings: Vec<Finding> = SCHEMA
        .iter_errors(&value)
        .take(MAX_SCHEMA_ERRORS)
        .map(|err| {
            error(
                "stac.metadata_schema",
                format!(
                    "the `stac-geoparquet` metadata at `{}`: {err}",
                    err.instance_path()
                ),
                REF_PARQUET_METADATA,
            )
        })
        .collect();
    match value.get("version").and_then(Value::as_str) {
        Some(version) if SUPPORTED_VERSIONS.contains(&version) => {}
        Some(version) => findings.push(error(
            "stac.version",
            format!("the stac-geoparquet version `{version}` is not supported"),
            REF_PARQUET_METADATA,
        )),
        None => findings.push(error(
            "stac.version",
            "the `stac-geoparquet` metadata has no string `version`".to_string(),
            REF_PARQUET_METADATA,
        )),
    }
    if let Some(collections) = value.get("collections").and_then(Value::as_object) {
        for (key, collection) in collections {
            if collection.get("id").and_then(Value::as_str) != Some(key) {
                findings.push(error(
                    "stac.collection_id_mismatch",
                    format!("the collection key `{key}` does not match the collection's `id`"),
                    REF_COLLECTIONS,
                ));
            }
        }
    }
    if value.get("collection").is_some() {
        findings.push(finding(
            "stac.deprecated_collection",
            Severity::Warning,
            "the singular `collection` key is deprecated; use `collections`".to_string(),
            REF_COLLECTIONS,
        ));
    }
    findings
}
