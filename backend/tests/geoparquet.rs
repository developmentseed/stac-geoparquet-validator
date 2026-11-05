mod common;

use arrow::{
    array::{Array, AsArray, RecordBatch, StructArray},
    datatypes::{DataType, Field, Fields, Schema},
};
use common::{Fixture, geo_metadata};
use object_store::{ObjectStoreExt, memory::InMemory, path::Path};
use serde_json::json;
use stac_geoparquet_validator::{Context, Finding, Severity, checks};
use std::sync::Arc;

async fn findings(fixture: &Fixture) -> Vec<Finding> {
    let store = Arc::new(InMemory::new());
    let path = Path::from("test.parquet");
    store.put(&path, fixture.bytes().into()).await.unwrap();
    let context = Context::load(store, path, "test.parquet".to_string())
        .await
        .unwrap();
    checks::run_footer_checks(&context)
}

fn has(findings: &[Finding], id: &str) -> bool {
    findings.iter().any(|finding| finding.id == id)
}

fn errors(findings: &[Finding]) -> Vec<&Finding> {
    findings
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .collect()
}

fn rename_bbox_children(batch: &RecordBatch, names: [&str; 4]) -> RecordBatch {
    let schema = batch.schema();
    let index = schema.index_of("bbox").unwrap();
    let bbox = batch.column(index).as_struct();
    let fields: Fields = names
        .into_iter()
        .map(|name| Field::new(name, DataType::Float64, true))
        .collect();
    let renamed = StructArray::new(
        fields.clone(),
        bbox.columns().to_vec(),
        bbox.nulls().cloned(),
    );
    let mut schema_fields: Vec<Field> = schema
        .fields()
        .iter()
        .map(|field| field.as_ref().clone())
        .collect();
    schema_fields[index] = Field::new("bbox", DataType::Struct(fields), true);
    let mut columns = batch.columns().to_vec();
    columns[index] = Arc::new(renamed);
    RecordBatch::try_new(Arc::new(Schema::new(schema_fields)), columns).unwrap()
}

#[tokio::test]
async fn valid_1_1_has_no_errors() {
    let findings = findings(&Fixture::valid()).await;
    assert!(errors(&findings).is_empty(), "{:?}", errors(&findings));
    assert!(has(&findings, "geoparquet.info"));
}

#[tokio::test]
async fn valid_1_0_has_no_errors() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("1.0.0");
    geo["columns"]["geometry"]
        .as_object_mut()
        .unwrap()
        .remove("covering");
    fixture.set_geo(Some(geo.to_string()));
    let findings = findings(&fixture).await;
    assert!(errors(&findings).is_empty(), "{:?}", errors(&findings));
}

#[tokio::test]
async fn no_geo_no_native_is_missing() {
    let mut fixture = Fixture::valid();
    fixture.set_geo(None);
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.metadata_missing"));
}

#[tokio::test]
async fn native_without_geo() {
    let mut fixture = Fixture::valid();
    fixture.set_geo(None);
    fixture.native = true;
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.native_without_metadata"));
    assert!(has(&findings, "geoparquet.native_types"));
    assert!(!has(&findings, "geoparquet.metadata_missing"));
}

#[tokio::test]
async fn valid_2_0_native() {
    let mut fixture = Fixture::valid();
    fixture.set_geo(Some(geo_metadata("2.0.0").to_string()));
    fixture.native = true;
    let findings = findings(&fixture).await;
    assert!(errors(&findings).is_empty(), "{:?}", errors(&findings));
    assert!(has(&findings, "geoparquet.native_types"));
}

#[tokio::test]
async fn version_2_0_without_native_is_encoding_error() {
    let mut fixture = Fixture::valid();
    fixture.set_geo(Some(geo_metadata("2.0.0").to_string()));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.encoding"));
}

#[tokio::test]
async fn geo_not_json() {
    let mut fixture = Fixture::valid();
    fixture.set_geo(Some("{not json".to_string()));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.metadata_invalid_json"));
}

#[tokio::test]
async fn geo_wrong_shape() {
    let mut fixture = Fixture::valid();
    fixture.set_geo(Some(
        json!({"version": "1.1.0", "primary_column": 5, "columns": []}).to_string(),
    ));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.metadata_schema"));
}

#[tokio::test]
async fn unknown_version() {
    let mut fixture = Fixture::valid();
    fixture.set_geo(Some(geo_metadata("9.0.0").to_string()));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.unknown_version"));
}

#[tokio::test]
async fn primary_column_not_in_columns() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("1.1.0");
    geo["primary_column"] = json!("other");
    fixture.set_geo(Some(geo.to_string()));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.primary_column_missing"));
}

#[tokio::test]
async fn geo_column_not_in_schema() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("1.1.0");
    geo["columns"]["other"] = json!({"encoding": "WKB", "geometry_types": []});
    fixture.set_geo(Some(geo.to_string()));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.column_missing"));
}

#[tokio::test]
async fn geo_column_nested() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("1.1.0");
    geo["columns"]["bbox.xmin"] = json!({"encoding": "WKB", "geometry_types": []});
    fixture.set_geo(Some(geo.to_string()));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.column_nested"));
}

#[tokio::test]
async fn geo_column_repeated() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("1.1.0");
    geo["columns"]["stac_extensions"] = json!({"encoding": "WKB", "geometry_types": []});
    fixture.set_geo(Some(geo.to_string()));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.column_repeated"));
}

#[tokio::test]
async fn duplicate_geometry_types() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("1.1.0");
    geo["columns"]["geometry"]["geometry_types"] = json!(["Point", "Point"]);
    fixture.set_geo(Some(geo.to_string()));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.geometry_types_duplicate"));
}

#[tokio::test]
async fn covering_wrong_child_name() {
    let mut fixture = Fixture::valid();
    fixture.batch = rename_bbox_children(&fixture.batch, ["minx", "ymin", "xmax", "ymax"]);
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.covering_invalid"));
}

#[tokio::test]
async fn covering_missing_column() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("1.1.0");
    for key in ["xmin", "ymin", "xmax", "ymax"] {
        geo["columns"]["geometry"]["covering"]["bbox"][key][0] = json!("nope");
    }
    fixture.set_geo(Some(geo.to_string()));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.covering_invalid"));
}

#[tokio::test]
async fn crs_mismatch() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("2.0.0");
    geo["columns"]["geometry"]["crs"] = json!({"id": {"authority": "EPSG", "code": 4326}});
    fixture.set_geo(Some(geo.to_string()));
    fixture.native = true;
    fixture.native_metadata = json!({"crs": "EPSG:32633"});
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.crs_mismatch"));
}

#[tokio::test]
async fn crs_matching_projjson_is_not_mismatch() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("2.0.0");
    geo["columns"]["geometry"]["crs"] = json!({"id": {"authority": "OGC", "code": "CRS84"}});
    fixture.set_geo(Some(geo.to_string()));
    fixture.native = true;
    let findings = findings(&fixture).await;
    assert!(!has(&findings, "geoparquet.crs_mismatch"));
}

#[tokio::test]
async fn srid_0_requires_null_crs() {
    let mut fixture = Fixture::valid();
    fixture.set_geo(Some(geo_metadata("2.0.0").to_string()));
    fixture.native = true;
    fixture.native_metadata = json!({});
    let findings = findings(&fixture).await;
    assert!(has(&findings, "geoparquet.crs_mismatch"));
}

#[tokio::test]
async fn srid_0_with_null_crs_is_ok() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("2.0.0");
    geo["columns"]["geometry"]["crs"] = json!(null);
    fixture.set_geo(Some(geo.to_string()));
    fixture.native = true;
    fixture.native_metadata = json!({});
    let findings = findings(&fixture).await;
    assert!(!has(&findings, "geoparquet.crs_mismatch"));
}
