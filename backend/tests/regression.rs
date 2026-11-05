use stac_geoparquet_validator::{Options, validate_href};

#[tokio::test]
#[ignore = "requires network access"]
async fn stac_map_555() {
    let report = validate_href(
        "https://data.source.coop/tge-labs/s2-stac-geoparquet/sentinel-2-l2a/year=2016/items.parquet",
        &Options::default(),
    )
    .await;
    let ids: Vec<&str> = report.findings.iter().map(|f| f.id.as_ref()).collect();
    eprintln!("{ids:?}");
    assert!(ids.contains(&"stac.column_type"));
    assert!(ids.contains(&"geoparquet.native_types"));
    assert!(ids.contains(&"stacbp.hive_partitioning"));
    assert!(report.summary.parquet_valid);
}
