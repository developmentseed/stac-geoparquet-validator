//! Validation report types.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

const REPORT_VERSION: &str = "1";

/// How serious a finding is, from most to least severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The file violates a requirement.
    Error,
    /// The file violates a recommendation.
    Warning,
    /// The file is valid but may not work with some readers.
    Compat,
    /// Informational only.
    Info,
}

/// The part of the specification a finding belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    /// Basic Parquet readability.
    Parquet,
    /// GeoParquet requirements.
    Geoparquet,
    /// GeoParquet best practices.
    GeoparquetBestPractices,
    /// stac-geoparquet requirements.
    StacGeoparquet,
    /// stac-geoparquet best practices.
    StacGeoparquetBestPractices,
}

/// A single result of a validation check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// The stable check identifier.
    pub id: Cow<'static, str>,
    /// How serious the finding is.
    pub severity: Severity,
    /// The specification section the check belongs to.
    pub section: Section,
    /// A human-readable description.
    pub message: String,
    /// A link or citation for the relevant specification text.
    pub reference: Cow<'static, str>,
}

impl Finding {
    /// Creates a new finding.
    pub fn new(
        id: &'static str,
        severity: Severity,
        section: Section,
        message: impl Into<String>,
        reference: &'static str,
    ) -> Finding {
        Finding {
            id: Cow::Borrowed(id),
            severity,
            section,
            message: message.into(),
            reference: Cow::Borrowed(reference),
        }
    }
}

/// Number of findings per severity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    /// Number of errors.
    pub error: usize,
    /// Number of warnings.
    pub warning: usize,
    /// Number of compatibility notes.
    pub compat: usize,
    /// Number of informational findings.
    pub info: usize,
}

/// Overall validity, derived from the findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    /// True if there are no Parquet errors.
    pub parquet_valid: bool,
    /// True if the file is valid Parquet and has no GeoParquet errors.
    pub geoparquet_valid: bool,
    /// The GeoParquet version declared by the file, if any.
    pub geoparquet_version: Option<String>,
    /// Whether the file uses native Parquet geospatial types.
    pub native_geospatial_types: bool,
    /// True if there are no Parquet, GeoParquet or stac-geoparquet errors.
    pub stac_geoparquet_valid: bool,
    /// Finding counts by severity.
    pub counts: Counts,
}

/// A complete validation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// The report format version.
    pub report_version: Cow<'static, str>,
    /// The href that was validated.
    pub href: String,
    /// The derived summary.
    pub summary: Summary,
    /// All findings, sorted by section and severity.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Builds a report, sorting the findings and deriving the summary.
    pub fn new(
        href: String,
        mut findings: Vec<Finding>,
        geoparquet_version: Option<String>,
        native_geospatial_types: bool,
    ) -> Report {
        findings.sort_by_key(|f| (f.section, f.severity));
        let has_error = |sections: &[Section]| {
            findings
                .iter()
                .any(|f| f.severity == Severity::Error && sections.contains(&f.section))
        };
        let parquet_valid = !has_error(&[Section::Parquet]);
        let geoparquet_valid = !has_error(&[Section::Parquet, Section::Geoparquet]);
        let stac_geoparquet_valid = !has_error(&[
            Section::Parquet,
            Section::Geoparquet,
            Section::StacGeoparquet,
        ]);
        let mut counts = Counts::default();
        for finding in &findings {
            match finding.severity {
                Severity::Error => counts.error += 1,
                Severity::Warning => counts.warning += 1,
                Severity::Compat => counts.compat += 1,
                Severity::Info => counts.info += 1,
            }
        }
        Report {
            report_version: Cow::Borrowed(REPORT_VERSION),
            href,
            summary: Summary {
                parquet_valid,
                geoparquet_valid,
                geoparquet_version,
                native_geospatial_types,
                stac_geoparquet_valid,
                counts,
            },
            findings,
        }
    }
}
