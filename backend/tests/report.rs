use stac_geoparquet_validator::{Finding, Report, Section, Severity};

fn finding(id: &'static str, severity: Severity, section: Section) -> Finding {
    Finding::new(id, severity, section, "message", "ref")
}

fn report(findings: Vec<Finding>) -> Report {
    Report::new("test.parquet".to_string(), findings, None, false)
}

#[test]
fn summary_counts_by_severity() {
    let report = report(vec![
        finding("parquet.unreadable", Severity::Error, Section::Parquet),
        finding(
            "bp.compression",
            Severity::Warning,
            Section::GeoparquetBestPractices,
        ),
        finding(
            "bp.row_group_size",
            Severity::Warning,
            Section::GeoparquetBestPractices,
        ),
        finding("parquet.info", Severity::Info, Section::Parquet),
    ]);
    assert_eq!(report.summary.counts.error, 1);
    assert_eq!(report.summary.counts.warning, 2);
    assert_eq!(report.summary.counts.compat, 0);
    assert_eq!(report.summary.counts.info, 1);
    assert!(!report.summary.parquet_valid);
}

#[test]
fn geoparquet_invalid_does_not_affect_parquet_valid() {
    let report = report(vec![finding(
        "geoparquet.metadata_missing",
        Severity::Error,
        Section::Geoparquet,
    )]);
    assert!(report.summary.parquet_valid);
    assert!(!report.summary.geoparquet_valid);
    assert!(!report.summary.stac_geoparquet_valid);
}

#[test]
fn best_practice_warnings_keep_valid() {
    let report = report(vec![
        finding(
            "bp.compression",
            Severity::Warning,
            Section::GeoparquetBestPractices,
        ),
        finding(
            "geoparquet.native_types",
            Severity::Compat,
            Section::Geoparquet,
        ),
        finding(
            "stac.type_column",
            Severity::Warning,
            Section::StacGeoparquet,
        ),
    ]);
    assert!(report.summary.parquet_valid);
    assert!(report.summary.geoparquet_valid);
    assert!(report.summary.stac_geoparquet_valid);
}

#[test]
fn findings_are_sorted_by_section_then_severity() {
    let report = report(vec![
        finding(
            "stac.type_column",
            Severity::Warning,
            Section::StacGeoparquet,
        ),
        finding("geoparquet.info", Severity::Info, Section::Geoparquet),
        finding("geoparquet.encoding", Severity::Error, Section::Geoparquet),
        finding("parquet.info", Severity::Info, Section::Parquet),
    ]);
    let ids: Vec<_> = report.findings.iter().map(|f| f.id.as_ref()).collect();
    assert_eq!(
        ids,
        [
            "parquet.info",
            "geoparquet.encoding",
            "geoparquet.info",
            "stac.type_column"
        ]
    );
}

#[test]
fn serializes_report_version() {
    let report = report(vec![finding(
        "geoparquet.native_types",
        Severity::Compat,
        Section::Geoparquet,
    )]);
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["report_version"], "1");
    assert_eq!(value["findings"][0]["severity"], "compat");
    assert_eq!(value["findings"][0]["section"], "geoparquet");
    let roundtrip: Report = serde_json::from_value(value).unwrap();
    assert_eq!(roundtrip.findings[0].id, "geoparquet.native_types");
}
