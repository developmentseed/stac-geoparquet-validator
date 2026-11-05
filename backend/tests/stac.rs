mod common;

use arrow::{
    array::{ArrayRef, RecordBatch, StringArray},
    datatypes::{DataType, Field, Fields, Schema},
};
use common::{Fixture, stac_metadata, validate_bytes};
use serde_json::json;
use stac_geoparquet_validator::{Finding, Severity};
use std::sync::Arc;

async fn findings(fixture: &Fixture) -> Vec<Finding> {
    validate_bytes(fixture.bytes(), "test.parquet")
        .await
        .findings
}

fn has(findings: &[Finding], id: &str) -> bool {
    findings.iter().any(|finding| finding.id == id)
}

fn replace_column(batch: &RecordBatch, name: &str, field: Field, column: ArrayRef) -> RecordBatch {
    let schema = batch.schema();
    let index = schema.index_of(name).unwrap();
    let mut fields: Vec<Field> = schema.fields().iter().map(|f| f.as_ref().clone()).collect();
    fields[index] = field;
    let mut columns = batch.columns().to_vec();
    columns[index] = column;
    RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).unwrap()
}

fn strings(batch: &RecordBatch) -> ArrayRef {
    Arc::new(StringArray::from(vec!["x"; batch.num_rows()]))
}

fn drop_column(batch: &RecordBatch, name: &str) -> RecordBatch {
    let schema = batch.schema();
    let index = schema.index_of(name).unwrap();
    batch
        .project(
            &(0..batch.num_columns())
                .filter(|i| *i != index)
                .collect::<Vec<_>>(),
        )
        .unwrap()
}

fn set_stac(fixture: &mut Fixture, value: Option<serde_json::Value>) {
    fixture.kv.retain(|(key, _)| key != "stac-geoparquet");
    if let Some(value) = value {
        fixture
            .kv
            .push(("stac-geoparquet".to_string(), value.to_string()));
    }
}

#[tokio::test]
async fn valid_stac_has_no_stac_errors() {
    let findings = findings(&Fixture::valid()).await;
    assert!(
        !findings
            .iter()
            .any(|f| f.id.starts_with("stac.") && f.severity == Severity::Error),
        "{findings:?}"
    );
}

#[tokio::test]
async fn assets_as_string_is_type_error() {
    let mut fixture = Fixture::valid();
    let batch = fixture.batch.clone();
    fixture.batch = replace_column(
        &batch,
        "assets",
        Field::new("assets", DataType::Utf8, true),
        strings(&batch),
    );
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stac.column_type"));
}

#[tokio::test]
async fn datetime_as_string_is_type_error() {
    let mut fixture = Fixture::valid();
    let batch = fixture.batch.clone();
    fixture.batch = replace_column(
        &batch,
        "datetime",
        Field::new("datetime", DataType::Utf8, true),
        strings(&batch),
    );
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stac.column_type"));
}

#[tokio::test]
async fn missing_links() {
    let mut fixture = Fixture::valid();
    fixture.batch = drop_column(&fixture.batch, "links");
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stac.column_missing"));
}

#[tokio::test]
async fn links_without_rel() {
    let mut fixture = Fixture::valid();
    let batch = fixture.batch.clone();
    let item = Arc::new(Field::new(
        "item",
        DataType::Struct(Fields::from(vec![Field::new("href", DataType::Utf8, true)])),
        true,
    ));
    let values = arrow::array::StructArray::new(
        Fields::from(vec![Field::new("href", DataType::Utf8, true)]),
        vec![strings(&batch)],
        None,
    );
    let links = arrow::array::ListArray::new(
        item.clone(),
        arrow::buffer::OffsetBuffer::from_lengths(vec![1; batch.num_rows()]),
        Arc::new(values),
        None,
    );
    fixture.batch = replace_column(
        &batch,
        "links",
        Field::new("links", DataType::List(item), true),
        Arc::new(links),
    );
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stac.column_type"));
}

#[tokio::test]
async fn no_stac_metadata() {
    let mut fixture = Fixture::valid();
    set_stac(&mut fixture, None);
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stac.metadata_missing"));
}

#[tokio::test]
async fn metadata_schema_violation() {
    let mut fixture = Fixture::valid();
    set_stac(
        &mut fixture,
        Some(json!({"version": "1.1.0", "collections": 3})),
    );
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stac.metadata_schema"));
}

#[tokio::test]
async fn bad_version() {
    let mut fixture = Fixture::valid();
    let mut metadata = stac_metadata();
    metadata["version"] = json!("0.9.0");
    set_stac(&mut fixture, Some(metadata));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stac.version"));
}

#[tokio::test]
async fn collection_key_mismatch() {
    let mut fixture = Fixture::valid();
    let mut metadata = stac_metadata();
    metadata["collections"]["test"]["id"] = json!("other");
    set_stac(&mut fixture, Some(metadata));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stac.collection_id_mismatch"));
}

#[tokio::test]
async fn deprecated_collection_warns() {
    let mut fixture = Fixture::valid();
    let mut metadata = stac_metadata();
    metadata["collection"] = metadata["collections"]["test"].clone();
    set_stac(&mut fixture, Some(metadata));
    let findings = findings(&fixture).await;
    let finding = findings
        .iter()
        .find(|f| f.id == "stac.deprecated_collection")
        .unwrap();
    assert_eq!(finding.severity, Severity::Warning);
}

#[tokio::test]
async fn type_column_warns() {
    let mut fixture = Fixture::valid();
    let batch = fixture.batch.clone();
    let mut fields: Vec<Field> = batch
        .schema()
        .fields()
        .iter()
        .map(|f| f.as_ref().clone())
        .collect();
    fields.push(Field::new("type", DataType::Utf8, true));
    let mut columns = batch.columns().to_vec();
    columns.push(strings(&batch));
    fixture.batch = RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).unwrap();
    let findings = findings(&fixture).await;
    let finding = findings
        .iter()
        .find(|f| f.id == "stac.type_column")
        .unwrap();
    assert_eq!(finding.severity, Severity::Warning);
}

#[tokio::test]
async fn utm_crs() {
    let mut fixture = Fixture::valid();
    fixture.native = true;
    fixture.native_metadata = json!({"crs": "EPSG:32633"});
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stac.crs"));
}

#[tokio::test]
async fn native_without_geo_metadata_is_not_stac_geoparquet() {
    let mut fixture = Fixture::valid();
    fixture.native = true;
    fixture.set_geo(None);
    let report = validate_bytes(fixture.bytes(), "test.parquet").await;
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.id == "stac.geoparquet_metadata_missing")
        .unwrap();
    assert_eq!(finding.severity, Severity::Error);
    assert!(has(&report.findings, "geoparquet.native_without_metadata"));
    assert!(!report.summary.stac_geoparquet_valid);
}

#[tokio::test]
async fn geo_metadata_present_is_not_flagged() {
    let findings = findings(&Fixture::valid()).await;
    assert!(!has(&findings, "stac.geoparquet_metadata_missing"));
}
