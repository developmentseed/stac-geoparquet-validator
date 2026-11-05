mod common;

use arrow::array::RecordBatch;
use bytes::Bytes;
use common::{Fixture, geo_metadata, stac_batch, validate_bytes};
use parquet::{
    arrow::ArrowWriter,
    basic::Compression,
    file::{metadata::KeyValue, properties::WriterProperties},
};
use stac_geoparquet_validator::Finding;

async fn findings_at(bytes: Bytes, name: &str) -> Vec<Finding> {
    validate_bytes(bytes, name).await.findings
}

async fn findings(fixture: &Fixture) -> Vec<Finding> {
    findings_at(fixture.bytes(), "test.parquet").await
}

fn has(findings: &[Finding], id: &str) -> bool {
    findings.iter().any(|finding| finding.id == id)
}

fn write_row_groups(sizes: &[usize]) -> Bytes {
    let fixture = Fixture::valid();
    let total: usize = sizes.iter().sum();
    let batch: RecordBatch = stac_batch(total);
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(Default::default()))
        .set_key_value_metadata(Some(
            fixture
                .kv
                .iter()
                .map(|(key, value)| KeyValue::new(key.clone(), value.clone()))
                .collect(),
        ))
        .build();
    let mut buffer = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut buffer, batch.schema(), Some(props)).unwrap();
    let mut offset = 0;
    for size in sizes {
        writer.write(&batch.slice(offset, *size)).unwrap();
        writer.flush().unwrap();
        offset += size;
    }
    writer.close().unwrap();
    Bytes::from(buffer)
}

#[tokio::test]
async fn snappy_warns() {
    let mut fixture = Fixture::valid();
    fixture.props = fixture
        .props
        .into_builder()
        .set_compression(Compression::SNAPPY)
        .build();
    assert!(has(&findings(&fixture).await, "bp.compression"));
}

#[tokio::test]
async fn zstd_does_not_warn() {
    assert!(!has(&findings(&Fixture::valid()).await, "bp.compression"));
}

#[tokio::test]
async fn small_row_groups_warn() {
    let findings = findings_at(
        write_row_groups(&[1_000, 1_000, 1_000, 500]),
        "test.parquet",
    )
    .await;
    assert!(has(&findings, "bp.row_group_size"));
}

#[tokio::test]
async fn small_last_row_group_ok() {
    let findings = findings_at(write_row_groups(&[100_000, 500]), "test.parquet").await;
    assert!(!has(&findings, "bp.row_group_size"));
}

#[tokio::test]
async fn single_small_row_group_ok() {
    assert!(!has(
        &findings(&Fixture::valid()).await,
        "bp.row_group_size"
    ));
}

#[tokio::test]
async fn version_1_0_warns() {
    let mut fixture = Fixture::valid();
    fixture.set_geo(Some(geo_metadata("1.0.0").to_string()));
    assert!(has(&findings(&fixture).await, "bp.version"));
}

#[tokio::test]
async fn version_1_1_without_covering_warns() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("1.1.0");
    geo["columns"]["geometry"]
        .as_object_mut()
        .unwrap()
        .remove("covering");
    fixture.set_geo(Some(geo.to_string()));
    assert!(has(&findings(&fixture).await, "bp.version"));
}

#[tokio::test]
async fn version_1_1_with_covering_ok() {
    assert!(!has(&findings(&Fixture::valid()).await, "bp.version"));
}

#[tokio::test]
async fn version_2_0_ok() {
    let mut fixture = Fixture::valid();
    let mut geo = geo_metadata("2.0.0");
    geo["columns"]["geometry"]
        .as_object_mut()
        .unwrap()
        .remove("covering");
    fixture.set_geo(Some(geo.to_string()));
    fixture.native = true;
    assert!(!has(&findings(&fixture).await, "bp.version"));
}

#[tokio::test]
async fn geoparquet_extension_warns() {
    let findings = findings_at(Fixture::valid().bytes(), "x.geoparquet").await;
    assert!(has(&findings, "bp.extension"));
}

#[tokio::test]
async fn parquet_extension_ok() {
    assert!(!has(&findings(&Fixture::valid()).await, "bp.extension"));
}
