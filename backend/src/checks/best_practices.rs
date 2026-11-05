use crate::{Context, Finding, Section, Severity, geo::GeoState};
use parquet::basic::Compression;
use std::collections::BTreeSet;

const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const ROW_GROUP_ROWS: std::ops::RangeInclusive<i64> = 50_000..=150_000;

const REF_COMPRESSION: &str = "https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md#compression";
const REF_ROW_GROUP_SIZE: &str = "https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md#row-group-size";
const REF_SPATIAL_ACCESS: &str = "https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md#efficient-spatial-access";
const REF_PARTITIONING: &str = "https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md#spatial-partitioning";
const REF_EXTENSION: &str = "https://github.com/opengeospatial/geoparquet/blob/main/format-specs/geoparquet.md#file-extension";

fn finding(id: &'static str, message: String, reference: &'static str) -> Finding {
    Finding::new(
        id,
        Severity::Warning,
        Section::GeoparquetBestPractices,
        message,
        reference,
    )
}

fn codec_name(compression: Compression) -> String {
    match compression {
        Compression::UNCOMPRESSED => "UNCOMPRESSED".to_string(),
        Compression::SNAPPY => "SNAPPY".to_string(),
        Compression::GZIP(_) => "GZIP".to_string(),
        Compression::LZO => "LZO".to_string(),
        Compression::BROTLI(_) => "BROTLI".to_string(),
        Compression::LZ4 => "LZ4".to_string(),
        Compression::ZSTD(_) => "ZSTD".to_string(),
        Compression::LZ4_RAW => "LZ4_RAW".to_string(),
    }
}

pub(super) fn compression(context: &Context) -> Vec<Finding> {
    let codecs: BTreeSet<String> = context
        .metadata
        .row_groups()
        .iter()
        .flat_map(|row_group| row_group.columns())
        .map(|column| column.compression())
        .filter(|compression| !matches!(compression, Compression::ZSTD(_)))
        .map(codec_name)
        .collect();
    if codecs.is_empty() {
        return Vec::new();
    }
    vec![finding(
        "bp.compression",
        format!(
            "Column chunks use {} instead of ZSTD",
            codecs.into_iter().collect::<Vec<_>>().join(", ")
        ),
        REF_COMPRESSION,
    )]
}

pub(super) fn row_group_size(context: &Context) -> Vec<Finding> {
    let row_groups = context.metadata.row_groups();
    let Some((_, leading)) = row_groups.split_last() else {
        return Vec::new();
    };
    let outliers = leading
        .iter()
        .filter(|row_group| !ROW_GROUP_ROWS.contains(&row_group.num_rows()))
        .count();
    if outliers == 0 {
        return Vec::new();
    }
    vec![finding(
        "bp.row_group_size",
        format!(
            "{outliers} of {} row groups (excluding the last) are outside 50,000-150,000 rows",
            row_groups.len()
        ),
        REF_ROW_GROUP_SIZE,
    )]
}

pub(super) fn version(context: &Context) -> Vec<Finding> {
    let GeoState::Valid(geo) = context.geo() else {
        return Vec::new();
    };
    let has_covering = geo
        .columns
        .get(&geo.primary_column)
        .is_some_and(|column| column.covering.is_some());
    let message = if geo.version.starts_with("1.0") {
        format!(
            "GeoParquet {} lacks the covering support of 1.1 and later",
            geo.version
        )
    } else if geo.version.starts_with("1.1") && !has_covering {
        "GeoParquet 1.1 file has no covering on its primary geometry column".to_string()
    } else {
        return Vec::new();
    };
    vec![finding("bp.version", message, REF_SPATIAL_ACCESS)]
}

pub(super) fn file_size(context: &Context) -> Vec<Finding> {
    if context.size <= MAX_FILE_SIZE {
        return Vec::new();
    }
    vec![finding(
        "bp.file_size",
        format!(
            "File is {} bytes; consider partitioning files larger than 2 GiB",
            context.size
        ),
        REF_PARTITIONING,
    )]
}

pub(super) fn extension(context: &Context) -> Vec<Finding> {
    if !context.path.as_ref().ends_with(".geoparquet") {
        return Vec::new();
    }
    vec![finding(
        "bp.extension",
        "GeoParquet files should use the `.parquet` extension".to_string(),
        REF_EXTENSION,
    )]
}
