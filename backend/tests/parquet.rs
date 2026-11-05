mod common;

use bytes::Bytes;
use common::{Fixture, stac_batch};
use object_store::{ObjectStoreExt, memory::InMemory, path::Path};
use parquet::{
    basic::LogicalType, file::reader::FileReader, file::serialized_reader::SerializedFileReader,
};
use stac_geoparquet_validator::{Context, Finding, checks};
use std::sync::Arc;

async fn load(bytes: Bytes) -> Result<Context, Finding> {
    let store = Arc::new(InMemory::new());
    let path = Path::from("test.parquet");
    store.put(&path, bytes.into()).await.unwrap();
    Context::load(store, path, "test.parquet".to_string()).await
}

fn count(findings: &[Finding], id: &str) -> usize {
    findings.iter().filter(|finding| finding.id == id).count()
}

#[tokio::test]
async fn valid_file_emits_parquet_info() {
    let context = load(Fixture::valid().bytes()).await.unwrap();
    let findings = checks::run_footer_checks(&context);
    assert_eq!(count(&findings, "parquet.info"), 1);
    assert_eq!(count(&findings, "parquet.unreadable"), 0);
}

#[tokio::test]
async fn html_body_is_unreadable() {
    let finding = load(Bytes::from_static(b"<html>404</html>"))
        .await
        .unwrap_err();
    assert_eq!(finding.id, "parquet.unreadable");
}

#[tokio::test]
async fn empty_file_is_unreadable() {
    let finding = load(Bytes::new()).await.unwrap_err();
    assert_eq!(finding.id, "parquet.unreadable");
}

#[tokio::test]
async fn seven_bytes_is_unreadable() {
    let finding = load(Bytes::from_static(b"PAR1PAR")).await.unwrap_err();
    assert_eq!(finding.id, "parquet.unreadable");
}

#[tokio::test]
async fn footer_length_larger_than_file_is_unreadable() {
    let mut bytes = b"PAR1".to_vec();
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(b"PAR1");
    let finding = load(Bytes::from(bytes)).await.unwrap_err();
    assert_eq!(finding.id, "parquet.unreadable");
}

#[tokio::test]
async fn zero_row_file_loads() {
    let mut fixture = Fixture::valid();
    fixture.batch = stac_batch(0);
    let context = load(fixture.bytes()).await.unwrap();
    let findings = checks::run_footer_checks(&context);
    assert_eq!(count(&findings, "parquet.info"), 1);
}

#[test]
fn native_fixture_writes_geometry_logical_type() {
    let mut fixture = Fixture::valid();
    fixture.native = true;
    let reader = SerializedFileReader::new(fixture.bytes()).unwrap();
    let schema = reader.metadata().file_metadata().schema_descr();
    let column = schema
        .columns()
        .iter()
        .find(|column| column.name() == "geometry")
        .unwrap();
    assert!(matches!(
        column.logical_type_ref(),
        Some(LogicalType::Geometry { .. })
    ));
}
