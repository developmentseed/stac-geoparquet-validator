use crate::{
    Context, Finding, Section, Severity,
    geo::{self, GeoColumn, GeoMetadata, GeoState, NativeGeo},
};
use jsonschema::{Retrieve, Uri, Validator, error::ValidationErrorKind};
use parquet::{
    basic::{Repetition, Type as PhysicalType},
    schema::types::{Type, TypePtr},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    error::Error,
    sync::LazyLock,
};

const MAX_SCHEMA_ERRORS: usize = 20;
const WKB_1_1_ENCODINGS: [&str; 7] = [
    "WKB",
    "point",
    "linestring",
    "polygon",
    "multipoint",
    "multilinestring",
    "multipolygon",
];
const BBOX_2D: [&str; 4] = ["xmin", "ymin", "xmax", "ymax"];
const BBOX_3D: [&str; 6] = ["xmin", "ymin", "zmin", "xmax", "ymax", "zmax"];

const REF_METADATA: &str =
    "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#metadata";
const REF_FILE_METADATA: &str = "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#file-metadata";
const REF_VERSION: &str = "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#version-and-schema";
const REF_COLUMN_METADATA: &str = "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#column-metadata";
const REF_NESTING: &str =
    "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#nesting";
const REF_REPETITION: &str = "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#repetition";
const REF_ENCODING: &str =
    "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#encoding";
const REF_CRS: &str = "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#crs-parquet-property";
const REF_GEOMETRY_TYPES: &str = "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#geometry_types";
const REF_COVERING: &str =
    "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#covering";
const REF_GEOMETRY_COLUMNS: &str = "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#geometry-columns";

struct PermissiveRetriever;

impl Retrieve for PermissiveRetriever {
    fn retrieve(&self, _: &Uri<String>) -> Result<Value, Box<dyn Error + Send + Sync>> {
        Ok(json!({"type": "object"}))
    }
}

static SCHEMAS: LazyLock<Vec<(&'static str, Validator)>> = LazyLock::new(|| {
    [
        ("1.0", include_str!("../../schemas/geoparquet-1.0.0.json")),
        ("1.1", include_str!("../../schemas/geoparquet-1.1.0.json")),
        ("2.0", include_str!("../../schemas/geoparquet-2.0.0.json")),
    ]
    .into_iter()
    .map(|(version, schema)| {
        let mut schema: Value =
            serde_json::from_str(schema).expect("vendored schema is valid JSON");
        if let Some(version) = schema["properties"]["version"].as_object_mut() {
            version.remove("const");
        }
        let validator = jsonschema::options()
            .with_retriever(PermissiveRetriever)
            .build(&schema)
            .expect("vendored schema compiles");
        (version, validator)
    })
    .collect()
});

fn finding(
    id: &'static str,
    severity: Severity,
    message: String,
    reference: &'static str,
) -> Finding {
    Finding::new(id, severity, Section::Geoparquet, message, reference)
}

fn error(id: &'static str, message: String, reference: &'static str) -> Finding {
    finding(id, Severity::Error, message, reference)
}

fn major_minor(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
}

/// Validates the `geo` metadata and its geometry columns.
pub(super) fn metadata(context: &Context) -> Vec<Finding> {
    let natives = context.native_geo_columns();
    match context.geo() {
        GeoState::Missing if natives.is_empty() => vec![error(
            "geoparquet.metadata_missing",
            "the file has no `geo` metadata and no native geospatial columns".to_string(),
            REF_FILE_METADATA,
        )],
        GeoState::Missing => vec![finding(
            "geoparquet.native_without_metadata",
            Severity::Warning,
            format!(
                "native geospatial columns ({}) have no `geo` metadata",
                natives
                    .iter()
                    .map(|(name, _)| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            REF_METADATA,
        )],
        GeoState::InvalidJson => {
            let reason = context
                .key_value
                .get("geo")
                .and_then(|geo| serde_json::from_str::<Value>(geo).err())
                .map(|err| err.to_string())
                .unwrap_or_default();
            vec![error(
                "geoparquet.metadata_invalid_json",
                format!("the `geo` metadata is not valid JSON: {reason}"),
                REF_FILE_METADATA,
            )]
        }
        GeoState::Invalid(value) => {
            let mut findings = schema_findings(&value);
            if !findings
                .iter()
                .any(|finding| finding.id == "geoparquet.metadata_schema")
            {
                let reason = GeoMetadata::deserialize(&value)
                    .err()
                    .map(|err| err.to_string())
                    .unwrap_or_default();
                findings.push(error(
                    "geoparquet.metadata_schema",
                    format!("the `geo` metadata does not have the expected shape: {reason}"),
                    REF_FILE_METADATA,
                ));
            }
            findings
        }
        GeoState::Valid(metadata) => {
            let value = context
                .key_value
                .get("geo")
                .and_then(|geo| serde_json::from_str::<Value>(geo).ok())
                .unwrap_or(Value::Null);
            let mut findings = schema_findings(&value);
            findings.extend(duplicate_geometry_types(&metadata));
            if !findings
                .iter()
                .any(|finding| finding.id == "geoparquet.metadata_schema")
            {
                findings.extend(column_checks(context, &metadata, &natives));
            }
            findings.push(info(&metadata));
            findings
        }
    }
}

/// Notes each column that uses a native Parquet geospatial logical type.
pub(super) fn native_types(context: &Context) -> Vec<Finding> {
    context
        .native_geo_columns()
        .into_iter()
        .map(|(name, native)| {
            finding(
                "geoparquet.native_types",
                Severity::Compat,
                format!(
                    "column `{name}` uses the native Parquet {} logical type, which some readers \
                     cannot read (known failures: gpq, and arrow-go before v18)",
                    if native.geography {
                        "GEOGRAPHY"
                    } else {
                        "GEOMETRY"
                    }
                ),
                REF_GEOMETRY_COLUMNS,
            )
        })
        .collect()
}

fn schema_findings(value: &Value) -> Vec<Finding> {
    let version = value.get("version").and_then(Value::as_str);
    let validator = version.and_then(|version| {
        let version = major_minor(version);
        SCHEMAS
            .iter()
            .find(|(candidate, _)| *candidate == version)
            .map(|(_, validator)| validator)
    });
    let Some(validator) = validator else {
        let message = match version {
            Some(version) => format!(
                "GeoParquet version `{version}` is not one of 1.0, 1.1 or 2.0, so the metadata was not checked against a schema"
            ),
            None => "the `geo` metadata has no string `version`".to_string(),
        };
        return vec![error("geoparquet.unknown_version", message, REF_VERSION)];
    };
    validator
        .iter_errors(value)
        .filter(|err| {
            !(matches!(err.kind(), ValidationErrorKind::UniqueItems)
                && err.instance_path().as_str().ends_with("/geometry_types"))
        })
        .take(MAX_SCHEMA_ERRORS)
        .map(|err| {
            let path = err.instance_path().as_str();
            let path = if path.is_empty() { "/" } else { path };
            error(
                "geoparquet.metadata_schema",
                format!("`geo` metadata at `{path}`: {err}"),
                REF_FILE_METADATA,
            )
        })
        .collect()
}

fn duplicate_geometry_types(metadata: &GeoMetadata) -> Vec<Finding> {
    metadata
        .columns
        .iter()
        .filter_map(|(name, column)| {
            let mut seen = BTreeSet::new();
            let duplicates: BTreeSet<&str> = column
                .geometry_types
                .iter()
                .filter(|geometry_type| !seen.insert(geometry_type.as_str()))
                .map(String::as_str)
                .collect();
            (!duplicates.is_empty()).then(|| {
                error(
                    "geoparquet.geometry_types_duplicate",
                    format!(
                        "column `{name}` lists geometry types more than once: {}",
                        duplicates.into_iter().collect::<Vec<_>>().join(", ")
                    ),
                    REF_GEOMETRY_TYPES,
                )
            })
        })
        .collect()
}

fn column_checks(
    context: &Context,
    metadata: &GeoMetadata,
    natives: &[(String, NativeGeo)],
) -> Vec<Finding> {
    let schema = context.metadata.file_metadata().schema_descr();
    let roots = schema.root_schema().get_fields();
    let natives: HashMap<&str, &NativeGeo> = natives
        .iter()
        .map(|(name, native)| (name.as_str(), native))
        .collect();
    let mut findings = Vec::new();
    if !metadata.columns.contains_key(&metadata.primary_column) {
        findings.push(error(
            "geoparquet.primary_column_missing",
            format!(
                "primary column `{}` is not in `columns`",
                metadata.primary_column
            ),
            REF_FILE_METADATA,
        ));
    }
    for (name, column) in &metadata.columns {
        let Some(field) = roots.iter().find(|field| field.name() == name) else {
            let nested = schema.columns().iter().any(|leaf| {
                let path = leaf.path();
                path.string() == *name || path.parts()[1..].contains(name)
            });
            findings.push(if nested {
                error(
                    "geoparquet.column_nested",
                    format!("geometry column `{name}` is nested, not at the schema root"),
                    REF_NESTING,
                )
            } else {
                error(
                    "geoparquet.column_missing",
                    format!("geometry column `{name}` does not exist in the Parquet schema"),
                    REF_COLUMN_METADATA,
                )
            });
            continue;
        };
        if field.get_basic_info().repetition() == Repetition::REPEATED
            || (column.encoding == "WKB" && has_repeated_descendant(field))
        {
            findings.push(error(
                "geoparquet.column_repeated",
                format!("geometry column `{name}` is repeated"),
                REF_REPETITION,
            ));
        }
        if let Some(message) = encoding_problem(
            &metadata.version,
            name,
            column,
            field,
            natives.contains_key(name.as_str()),
        ) {
            findings.push(error("geoparquet.encoding", message, REF_ENCODING));
        }
        if let Some(native) = natives.get(name.as_str())
            && crs_mismatch(column, native)
        {
            findings.push(error(
                "geoparquet.crs_mismatch",
                format!(
                    "column `{name}` has `geo` crs {} but native crs {}",
                    describe_crs(column.crs.as_ref()),
                    native.crs.as_deref().unwrap_or("OGC:CRS84 (default)")
                ),
                REF_CRS,
            ));
        }
        if let Some(covering) = &column.covering
            && let Some(message) = covering_problem(roots, field, covering)
        {
            findings.push(error(
                "geoparquet.covering_invalid",
                format!("column `{name}` covering: {message}"),
                REF_COVERING,
            ));
        }
    }
    findings
}

fn has_repeated_descendant(field: &Type) -> bool {
    !field.is_primitive()
        && field.get_fields().iter().any(|child| {
            child.get_basic_info().repetition() == Repetition::REPEATED
                || has_repeated_descendant(child)
        })
}

fn encoding_problem(
    version: &str,
    name: &str,
    column: &GeoColumn,
    field: &Type,
    native: bool,
) -> Option<String> {
    let encoding = column.encoding.as_str();
    match major_minor(version).as_str() {
        "1.0" if encoding != "WKB" => {
            return Some(format!(
                "column `{name}` encoding `{encoding}` is not allowed in GeoParquet 1.0 (only `WKB`)"
            ));
        }
        "1.1" if !WKB_1_1_ENCODINGS.contains(&encoding) => {
            return Some(format!(
                "column `{name}` encoding `{encoding}` is not allowed in GeoParquet 1.1"
            ));
        }
        "2.0" if encoding != "WKB" => {
            return Some(format!(
                "column `{name}` encoding `{encoding}` is not allowed in GeoParquet 2.0 (only `WKB`)"
            ));
        }
        "2.0" if !native => {
            return Some(format!(
                "column `{name}` must use the native Parquet GEOMETRY or GEOGRAPHY logical type in GeoParquet 2.0"
            ));
        }
        _ => {}
    }
    (encoding == "WKB"
        && !(field.is_primitive() && field.get_physical_type() == PhysicalType::BYTE_ARRAY))
        .then(|| format!("column `{name}` uses WKB encoding but is not a BYTE_ARRAY column"))
}

fn crs_mismatch(column: &GeoColumn, native: &NativeGeo) -> bool {
    match (&column.crs, native.crs.as_deref()) {
        (geo_crs, Some("srid:0")) => !matches!(geo_crs, Some(None)),
        (Some(None), _) => true,
        (geo_crs, native_crs) => {
            geo::is_crs84(geo_crs.as_ref(), None) != geo::is_crs84(None, native_crs)
        }
    }
}

fn covering_problem(roots: &[TypePtr], geometry: &Type, covering: &Value) -> Option<String> {
    let bbox = covering.get("bbox")?;
    let Some(bbox) = bbox.as_object() else {
        return Some("`bbox` is not an object".to_string());
    };
    let mut column_name: Option<&str> = None;
    for (key, path) in bbox {
        let parts: Option<Vec<&str>> = path
            .as_array()
            .and_then(|parts| parts.iter().map(Value::as_str).collect());
        let Some([column, child]) = parts.as_deref() else {
            return Some(format!("`{key}` is not a two-element path"));
        };
        if child != key {
            return Some(format!("`{key}` points to child `{child}`"));
        }
        if column_name.is_some_and(|name| name != *column) {
            return Some("all entries must use the same column".to_string());
        }
        column_name = Some(column);
    }
    let column_name = column_name?;
    let Some(field) = roots.iter().find(|field| field.name() == column_name) else {
        return Some(format!(
            "column `{column_name}` does not exist at the schema root"
        ));
    };
    if field.is_primitive() {
        return Some(format!("column `{column_name}` is not a group"));
    }
    let children: Vec<&str> = field
        .get_fields()
        .iter()
        .map(|child| child.name())
        .collect();
    if children != BBOX_2D && children != BBOX_3D {
        return Some(format!(
            "column `{column_name}` has children {}, expected `xmin, ymin, [zmin,] xmax, ymax[, zmax]`",
            children.join(", ")
        ));
    }
    if let Some(key) = bbox.keys().find(|key| !children.contains(&key.as_str())) {
        return Some(format!("column `{column_name}` has no child `{key}`"));
    }
    let physical_types: BTreeSet<Option<PhysicalType>> = field
        .get_fields()
        .iter()
        .map(|child| child.is_primitive().then(|| child.get_physical_type()))
        .collect();
    if physical_types != BTreeSet::from([Some(PhysicalType::FLOAT)])
        && physical_types != BTreeSet::from([Some(PhysicalType::DOUBLE)])
    {
        return Some(format!(
            "children of `{column_name}` must all be FLOAT or all be DOUBLE"
        ));
    }
    let repetition = field.get_basic_info().repetition();
    let geometry_repetition = geometry.get_basic_info().repetition();
    (repetition != geometry_repetition).then(|| {
        format!(
            "column `{column_name}` repetition {repetition} does not match the geometry column's {geometry_repetition}"
        )
    })
}

fn describe_crs(crs: Option<&Option<Value>>) -> String {
    match crs {
        None => "OGC:CRS84 (default)".to_string(),
        Some(None) => "null (undefined)".to_string(),
        Some(Some(value)) => {
            let id = &value["id"];
            let code = match &id["code"] {
                Value::String(code) => Some(code.clone()),
                Value::Number(code) => Some(code.to_string()),
                _ => None,
            };
            match (id["authority"].as_str(), code) {
                (Some(authority), Some(code)) => format!("{authority}:{code}"),
                _ => value["name"]
                    .as_str()
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| "PROJJSON without an id".to_string()),
            }
        }
    }
}

fn info(metadata: &GeoMetadata) -> Finding {
    let primary = metadata.columns.get(&metadata.primary_column);
    let geometry_types = primary
        .map(|column| {
            if column.geometry_types.is_empty() {
                "any".to_string()
            } else {
                column.geometry_types.join(", ")
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    let crs = primary
        .map(|column| describe_crs(column.crs.as_ref()))
        .unwrap_or_else(|| "unknown".to_string());
    let bbox = primary
        .and_then(|column| column.bbox.as_ref())
        .map(|bbox| format!("{bbox:?}"))
        .unwrap_or_else(|| "not set".to_string());
    finding(
        "geoparquet.info",
        Severity::Info,
        format!(
            "GeoParquet {}, primary column `{}`, geometry types {geometry_types}, CRS {crs}, bbox {bbox}",
            metadata.version, metadata.primary_column
        ),
        REF_COLUMN_METADATA,
    )
}
