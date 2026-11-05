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

#[tokio::test]
#[ignore = "requires network access"]
async fn geoparquet_1_0_0_beta_1_example_through_redirect() {
    let report = validate_href(
        "https://github.com/opengeospatial/geoparquet/raw/v1.0.0-beta.1/examples/example.parquet",
        &Options::default(),
    )
    .await;
    let ids: Vec<&str> = report.findings.iter().map(|f| f.id.as_ref()).collect();
    eprintln!("{ids:?}");
    assert!(report.summary.parquet_valid);
    assert!(report.summary.geoparquet_valid);
    assert!(!ids.contains(&"geoparquet.metadata_schema"));
    assert!(!ids.contains(&"geoparquet.unknown_version"));
    assert!(ids.contains(&"bp.version"));
}
