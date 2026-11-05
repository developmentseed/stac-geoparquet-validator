mod common;

use arrow::{
    array::{BinaryArray, DictionaryArray, StructArray},
    datatypes::Int32Type,
};
use common::{
    Fixture, geo_metadata, row_group_props, scattered_points, stac_batch, validate_bytes,
    with_column, with_ids, with_points, wkb_point, wkb_polygon,
};
use object_store::{ObjectStore, ObjectStoreExt, memory::InMemory, path::Path};
use parquet::file::statistics::Statistics;
use stac_geoparquet_validator::{Context, Finding, Section, Severity, checks};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

async fn findings_at(fixture: &Fixture, path: &str, deadline: Instant) -> Vec<Finding> {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    let path = Path::from(path);
    store.put(&path, fixture.bytes().into()).await.unwrap();
    let context = Context::load(store.clone(), path.clone(), path.to_string())
        .await
        .unwrap();
    let findings = checks::data_checks(store, &path, &context, deadline).await;
    assert_no_nan(&findings);
    findings
}

fn assert_no_nan(findings: &[Finding]) {
    for finding in findings {
        assert!(!finding.message.contains("NaN"), "{finding:?}");
    }
}

async fn data_findings(fixture: &Fixture) -> Vec<Finding> {
    findings_at(
        fixture,
        "test.parquet",
        Instant::now() + Duration::from_secs(25),
    )
    .await
}

async fn findings(fixture: &Fixture) -> Vec<Finding> {
    findings_named(fixture, "test.parquet").await
}

async fn findings_named(fixture: &Fixture, name: &str) -> Vec<Finding> {
    let findings = validate_bytes(fixture.bytes(), name).await.findings;
    assert_no_nan(&findings);
    findings
}

fn has(findings: &[Finding], id: &str) -> bool {
    findings.iter().any(|finding| finding.id == id)
}

fn get<'a>(findings: &'a [Finding], id: &str) -> &'a Finding {
    findings.iter().find(|finding| finding.id == id).unwrap()
}

fn multi_row_group(n: usize, rows: usize) -> Fixture {
    let mut fixture = Fixture::valid();
    fixture.batch = stac_batch(n);
    fixture.props = row_group_props(rows);
    fixture
}

fn ids(values: impl Iterator<Item = usize>) -> Vec<String> {
    values.map(|i| format!("item-{i:06}")).collect()
}

#[tokio::test]
async fn shuffled_points_warn_spatial_order() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.batch = with_points(&fixture.batch, &scattered_points(1000));
    let findings = findings(&fixture).await;
    let finding = get(&findings, "bp.spatial_order");
    assert_eq!(finding.severity, Severity::Warning);
    assert_eq!(finding.section, Section::GeoparquetBestPractices);
}

#[tokio::test]
async fn sorted_points_ok() {
    let fixture = multi_row_group(1000, 250);
    let findings = data_findings(&fixture).await;
    assert!(!has(&findings, "bp.spatial_order"), "{findings:?}");
    assert!(
        !has(&findings, "stacbp.spatiotemporal_order"),
        "{findings:?}"
    );
    assert!(!has(&findings, "stacbp.id_prefix"), "{findings:?}");
    assert!(findings.is_empty(), "{findings:?}");
}

#[tokio::test]
async fn shuffled_points_and_times_warn_spatiotemporal_order() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.batch = with_points(&fixture.batch, &scattered_points(1000));
    let times = arrow::array::TimestampMicrosecondArray::from_iter_values(
        (0..1000i64).map(|i| 1_700_000_000_000_000 + ((i * 7919) % 1000) * 1_000_000),
    )
    .with_timezone("UTC");
    fixture.batch = with_column(&fixture.batch, "datetime", Arc::new(times));
    let findings = findings(&fixture).await;
    let finding = get(&findings, "stacbp.spatiotemporal_order");
    assert_eq!(finding.severity, Severity::Warning);
    assert_eq!(finding.section, Section::StacGeoparquetBestPractices);
}

#[tokio::test]
async fn bbox_column_fallback_without_covering() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.batch = with_points(&fixture.batch, &scattered_points(1000));
    let mut geo = geo_metadata("1.0.0");
    geo["columns"]["geometry"]
        .as_object_mut()
        .unwrap()
        .remove("covering");
    fixture.set_geo(Some(geo.to_string()));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "bp.spatial_order"), "{findings:?}");
}

#[tokio::test]
async fn native_statistics_spatial_order() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.batch = with_points(&fixture.batch, &scattered_points(1000));
    let constant = with_points(&fixture.batch, &vec![(0.0, 0.0); 1000]);
    fixture.batch = with_column(
        &fixture.batch,
        "bbox",
        Arc::new(
            constant
                .column_by_name("bbox")
                .unwrap()
                .as_any()
                .downcast_ref::<StructArray>()
                .unwrap()
                .clone(),
        ),
    );
    let mut geo = geo_metadata("2.0.0");
    geo["columns"]["geometry"]
        .as_object_mut()
        .unwrap()
        .remove("covering");
    fixture.set_geo(Some(geo.to_string()));
    fixture.native = true;
    let findings = findings(&fixture).await;
    assert!(has(&findings, "bp.spatial_order"), "{findings:?}");
}

#[tokio::test]
async fn single_row_group_no_order_checks() {
    let mut fixture = multi_row_group(1000, 100_000);
    fixture.batch = with_points(&fixture.batch, &scattered_points(1000));
    fixture.batch = with_ids(
        &fixture.batch,
        ids((0..1000).map(|i| (i * 7919) % 1000)),
        vec!["test".to_string(); 1000],
    );
    let findings = findings(&fixture).await;
    assert!(!has(&findings, "bp.spatial_order"), "{findings:?}");
    assert!(
        !has(&findings, "stacbp.spatiotemporal_order"),
        "{findings:?}"
    );
    assert!(!has(&findings, "stacbp.id_prefix"), "{findings:?}");
}

#[tokio::test]
async fn zero_rows_no_panic() {
    let fixture = multi_row_group(0, 250);
    let findings = data_findings(&fixture).await;
    assert!(findings.is_empty(), "{findings:?}");
}

#[tokio::test]
async fn all_points_same_location_no_nan() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.batch = with_points(&fixture.batch, &vec![(10.0, 10.0); 1000]);
    let findings = findings(&fixture).await;
    assert!(!has(&findings, "bp.spatial_order"), "{findings:?}");
    assert!(
        !has(&findings, "stacbp.spatiotemporal_order"),
        "{findings:?}"
    );
}

#[tokio::test]
async fn id_prefix_unsorted_warns() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.batch = with_ids(
        &fixture.batch,
        ids((0..1000).map(|i| (i * 7919) % 1000)),
        vec!["test".to_string(); 1000],
    );
    let findings = findings(&fixture).await;
    let finding = get(&findings, "stacbp.id_prefix");
    assert_eq!(finding.severity, Severity::Warning);
    assert_eq!(finding.section, Section::StacGeoparquetBestPractices);
}

#[tokio::test]
async fn id_prefix_unsorted_with_truncated_statistics_warns() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.props = row_group_props(250)
        .into_builder()
        .set_statistics_truncate_length(Some(8))
        .build();
    fixture.batch = with_ids(
        &fixture.batch,
        ids((0..1000).map(|i| (i * 7919) % 1000)),
        vec!["test".to_string(); 1000],
    );
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    let path = Path::from("test.parquet");
    store.put(&path, fixture.bytes().into()).await.unwrap();
    let context = Context::load(store, path.clone(), path.to_string())
        .await
        .unwrap();
    let id = context
        .metadata
        .file_metadata()
        .schema_descr()
        .columns()
        .iter()
        .position(|column| column.name() == "id")
        .unwrap();
    let Some(Statistics::ByteArray(stats)) = context.metadata.row_group(0).column(id).statistics()
    else {
        panic!("expected byte array statistics");
    };
    assert!(!stats.min_is_exact() || !stats.max_is_exact());
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stacbp.id_prefix"), "{findings:?}");
}

#[tokio::test]
async fn id_prefix_sorted_ok() {
    let fixture = multi_row_group(1000, 250);
    let findings = findings(&fixture).await;
    assert!(!has(&findings, "stacbp.id_prefix"), "{findings:?}");
}

#[tokio::test]
async fn duplicate_ids_error() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.batch = with_ids(
        &fixture.batch,
        ids((0..1000).map(|i| i / 2)),
        vec!["test".to_string(); 1000],
    );
    let findings = findings(&fixture).await;
    let finding = get(&findings, "stac.duplicate_id");
    assert_eq!(finding.severity, Severity::Error);
    assert_eq!(finding.section, Section::StacGeoparquet);
}

#[tokio::test]
async fn same_id_different_collection_ok() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.batch = with_ids(
        &fixture.batch,
        ids((0..1000).map(|i| i % 500)),
        (0..1000)
            .map(|i| if i < 500 { "a" } else { "b" }.to_string())
            .collect(),
    );
    let findings = findings(&fixture).await;
    assert!(!has(&findings, "stac.duplicate_id"), "{findings:?}");
}

#[tokio::test]
async fn declared_point_but_polygon_present() {
    let mut fixture = Fixture::valid();
    let polygon = wkb_polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)]);
    let geometry = BinaryArray::from_iter_values((0..10).map(|i| {
        if i == 5 {
            polygon.clone()
        } else {
            wkb_point(0.0, 0.0)
        }
    }));
    fixture.batch = with_column(&fixture.batch, "geometry", Arc::new(geometry));
    let findings = findings(&fixture).await;
    let finding = get(&findings, "geoparquet.geometry_types_mismatch");
    assert_eq!(finding.severity, Severity::Warning);
    assert_eq!(finding.section, Section::Geoparquet);
}

#[tokio::test]
async fn declared_points_match() {
    let findings = findings(&Fixture::valid()).await;
    assert!(
        !has(&findings, "geoparquet.geometry_types_mismatch"),
        "{findings:?}"
    );
}

#[tokio::test]
async fn hive_path_info() {
    let findings = findings_named(&Fixture::valid(), "year=2016/items.parquet").await;
    let finding = get(&findings, "stacbp.hive_partitioning");
    assert_eq!(finding.severity, Severity::Info);
    assert_eq!(finding.section, Section::StacGeoparquetBestPractices);
}

#[tokio::test]
async fn past_deadline_skips() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.batch = with_ids(
        &fixture.batch,
        ids((0..1000).map(|i| i / 2)),
        vec!["test".to_string(); 1000],
    );
    let findings = findings_at(&fixture, "test.parquet", Instant::now()).await;
    let skipped: Vec<&Finding> = findings
        .iter()
        .filter(|finding| finding.id == "sample.skipped")
        .collect();
    assert_eq!(skipped.len(), 2, "{findings:?}");
    assert!(skipped.iter().all(|f| f.severity == Severity::Info));
    assert!(skipped.iter().any(|f| f.section == Section::StacGeoparquet));
    assert!(skipped.iter().any(|f| f.section == Section::Geoparquet));
    assert!(!has(&findings, "stac.duplicate_id"), "{findings:?}");
}

#[tokio::test]
async fn same_id_different_dictionary_collection_ok() {
    let mut fixture = multi_row_group(1000, 250);
    fixture.batch = with_ids(
        &fixture.batch,
        ids((0..1000).map(|i| i % 500)),
        vec!["test".to_string(); 1000],
    );
    let collections: DictionaryArray<Int32Type> =
        (0..1000).map(|i| if i < 500 { "a" } else { "b" }).collect();
    fixture.batch = with_column(&fixture.batch, "collection", Arc::new(collections));
    let findings = findings(&fixture).await;
    assert!(!has(&findings, "stac.duplicate_id"), "{findings:?}");
}

#[tokio::test]
async fn dictionary_ids_duplicate_error() {
    let mut fixture = multi_row_group(1000, 250);
    let ids: DictionaryArray<Int32Type> = ids((0..1000).map(|i| i / 2))
        .iter()
        .map(String::as_str)
        .collect();
    fixture.batch = with_column(&fixture.batch, "id", Arc::new(ids));
    let findings = findings(&fixture).await;
    assert!(has(&findings, "stac.duplicate_id"), "{findings:?}");
}
