use crate::{
    Context, Finding, Section, Severity,
    geo::GeoState,
    sample::{self, Sample, SampleError},
};
use arrow_schema::{DataType, TimeUnit};
use object_store::{ObjectStore, path::Path};
use parquet::file::{metadata::RowGroupMetaData, statistics::Statistics};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};

const ORDER_THRESHOLD: f64 = 0.5;
const MAX_PAIRWISE_EXTENTS: usize = 2000;
const MAX_DUPLICATE_EXAMPLES: usize = 5;
const BBOX_KEYS: [&str; 4] = ["xmin", "ymin", "xmax", "ymax"];

const REF_SPATIAL_ORDER: &str = "https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md#spatial-ordering";
const REF_GEOMETRY_TYPES: &str = "https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#geometry_types";
const REF_GUIDELINES: &str = "https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#guidelines";
const REF_SPATIOTEMPORAL_ORDER: &str = "https://github.com/radiantearth/stac-geoparquet-spec/blob/main/docs/best-practices.md#use-spatio-temporal-ordering";
const REF_ID_PREFIX: &str = "https://github.com/radiantearth/stac-geoparquet-spec/blob/main/docs/best-practices.md#prefix-item-ids-with-the-spatio-temporal-ordering-key";
const REF_PARTITIONING: &str = "https://github.com/radiantearth/stac-geoparquet-spec/blob/main/docs/best-practices.md#partitioning";

/// The extent of one row group, taken from its statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct Extent {
    /// `[xmin, ymin, xmax, ymax]`, always finite with min <= max.
    pub bbox: Option<[f64; 4]>,
    /// The `(min, max)` time in microseconds since the epoch.
    pub time: Option<(i64, i64)>,
    /// The `(min, max)` id, only when both statistics are exact (not truncated).
    pub id: Option<(String, String)>,
}

/// Runs the checks that need row data or per-row-group statistics.
///
/// `stacbp.hive_partitioning` and the statistics-based ordering checks always run. The
/// sampled checks (`stac.duplicate_id`, `geoparquet.geometry_types_mismatch`) are replaced
/// by `sample.skipped` when the sample cannot be read before `deadline`.
pub async fn data_checks(
    store: Arc<dyn ObjectStore>,
    path: &Path,
    ctx: &Context,
    deadline: Instant,
) -> Vec<Finding> {
    let mut findings = hive_partitioning(path);
    let extents = row_group_extents(ctx);
    findings.extend(spatial_order(&extents));
    findings.extend(spatiotemporal_order(&extents));
    findings.extend(id_prefix(&extents));

    let declared = declared_geometry_types(ctx);
    let has_id = ctx.arrow_schema.field_with_name("id").is_ok();
    if declared.is_none() && !has_id {
        return findings;
    }
    match Sample::try_read(store, path, ctx, deadline).await {
        Ok(sample) => {
            if let Some(declared) = &declared {
                findings.extend(geometry_types_mismatch(&sample, declared));
            }
            if has_id {
                findings.extend(duplicate_id(&sample));
            }
        }
        Err(err) => {
            let reason = match err {
                SampleError::Deadline => "the time budget ran out".to_string(),
                SampleError::Read(err) => format!("the rows could not be read: {err}"),
            };
            if declared.is_some() {
                findings.push(skipped(
                    "geoparquet.geometry_types_mismatch",
                    Section::Geoparquet,
                    &reason,
                    REF_GEOMETRY_TYPES,
                ));
            }
            if has_id {
                findings.push(skipped(
                    "stac.duplicate_id",
                    Section::StacGeoparquet,
                    &reason,
                    REF_GUIDELINES,
                ));
            }
        }
    }
    findings
}

fn skipped(check: &str, section: Section, reason: &str, reference: &'static str) -> Finding {
    Finding::new(
        "sample.skipped",
        Severity::Info,
        section,
        format!("Skipped `{check}` because {reason}"),
        reference,
    )
}

fn hive_partitioning(path: &Path) -> Vec<Finding> {
    let partitions: Vec<String> = path
        .parts()
        .map(|part| part.as_ref().to_string())
        .filter(|part| {
            part.split_once('=')
                .is_some_and(|(key, value)| !key.is_empty() && !value.is_empty())
        })
        .collect();
    if partitions.is_empty() {
        return Vec::new();
    }
    vec![Finding::new(
        "stacbp.hive_partitioning",
        Severity::Info,
        Section::StacGeoparquetBestPractices,
        format!(
            "The path looks hive-partitioned ({})",
            partitions.join(", ")
        ),
        REF_PARTITIONING,
    )]
}

/// Returns the extent of each row group, taken from statistics only.
///
/// The bbox comes from native geospatial statistics of the primary geometry column, else
/// from the min/max of the bbox covering columns, else from a root `bbox` struct column.
/// The time comes from `datetime`, else from `start_datetime` min and `end_datetime` max.
pub fn row_group_extents(ctx: &Context) -> Vec<Extent> {
    let leaves = Leaves::new(ctx);
    ctx.metadata
        .row_groups()
        .iter()
        .map(|row_group| Extent {
            bbox: leaves.bbox(row_group),
            time: leaves.time(row_group),
            id: leaves.id(row_group),
        })
        .collect()
}

struct Leaves {
    native: Option<usize>,
    covering: Option<[usize; 4]>,
    bbox_struct: Option<[usize; 4]>,
    datetime: Option<(usize, TimeUnit)>,
    start_datetime: Option<(usize, TimeUnit)>,
    end_datetime: Option<(usize, TimeUnit)>,
    id: Option<usize>,
}

impl Leaves {
    fn new(ctx: &Context) -> Leaves {
        let columns = ctx.metadata.file_metadata().schema_descr().columns();
        let leaf = |path: &[&str]| {
            columns
                .iter()
                .position(|column| column.path().parts().iter().eq(path.iter()))
        };
        let primary = sample::primary_geometry_column(ctx);
        let native = primary.as_deref().and_then(|primary| {
            ctx.native_geo_columns()
                .iter()
                .any(|(name, _)| name == primary)
                .then(|| leaf(&[primary]))
                .flatten()
        });
        let covering = match (ctx.geo(), primary.as_deref()) {
            (GeoState::Valid(metadata), Some(primary)) => metadata
                .columns
                .get(primary)
                .and_then(|column| column.covering.as_ref())
                .and_then(|covering| covering_leaves(covering, &leaf)),
            _ => None,
        };
        let bbox_struct = bbox_leaves(&|key| leaf(&["bbox", key]));
        let timestamp = |name: &str| {
            let unit = match ctx.arrow_schema.field_with_name(name).ok()?.data_type() {
                DataType::Timestamp(unit, _) => *unit,
                _ => return None,
            };
            Some((leaf(&[name])?, unit))
        };
        Leaves {
            native,
            covering,
            bbox_struct,
            datetime: timestamp("datetime"),
            start_datetime: timestamp("start_datetime"),
            end_datetime: timestamp("end_datetime"),
            id: leaf(&["id"]),
        }
    }

    fn bbox(&self, row_group: &RowGroupMetaData) -> Option<[f64; 4]> {
        let native = self.native.and_then(|index| {
            let bbox = row_group
                .column(index)
                .geo_statistics()?
                .bounding_box()?
                .clone();
            valid_bbox([
                bbox.get_xmin(),
                bbox.get_ymin(),
                bbox.get_xmax(),
                bbox.get_ymax(),
            ])
        });
        native
            .or_else(|| {
                self.covering
                    .and_then(|leaves| stats_bbox(row_group, leaves))
            })
            .or_else(|| {
                self.bbox_struct
                    .and_then(|leaves| stats_bbox(row_group, leaves))
            })
    }

    fn time(&self, row_group: &RowGroupMetaData) -> Option<(i64, i64)> {
        let time = |leaf: Option<(usize, TimeUnit)>| {
            let (index, unit) = leaf?;
            let (min, max) = int64_stats(row_group.column(index).statistics()?)?;
            Some((to_micros(min, unit)?, to_micros(max, unit)?))
        };
        let datetime = time(self.datetime).or_else(|| {
            let (start, _) = time(self.start_datetime)?;
            let (_, end) = time(self.end_datetime)?;
            Some((start, end))
        })?;
        (datetime.0 <= datetime.1).then_some(datetime)
    }

    fn id(&self, row_group: &RowGroupMetaData) -> Option<(String, String)> {
        let Statistics::ByteArray(stats) = row_group.column(self.id?).statistics()? else {
            return None;
        };
        if !stats.min_is_exact() || !stats.max_is_exact() {
            return None;
        }
        let min = std::str::from_utf8(stats.min_opt()?.data()).ok()?;
        let max = std::str::from_utf8(stats.max_opt()?.data()).ok()?;
        Some((min.to_string(), max.to_string()))
    }
}

fn covering_leaves(
    covering: &Value,
    leaf: &impl Fn(&[&str]) -> Option<usize>,
) -> Option<[usize; 4]> {
    let bbox = covering.get("bbox")?;
    bbox_leaves(&|key| {
        let path: Vec<&str> = bbox
            .get(key)?
            .as_array()?
            .iter()
            .map(Value::as_str)
            .collect::<Option<_>>()?;
        leaf(&path)
    })
}

fn bbox_leaves(leaf: &impl Fn(&str) -> Option<usize>) -> Option<[usize; 4]> {
    Some([
        leaf(BBOX_KEYS[0])?,
        leaf(BBOX_KEYS[1])?,
        leaf(BBOX_KEYS[2])?,
        leaf(BBOX_KEYS[3])?,
    ])
}

fn stats_bbox(row_group: &RowGroupMetaData, leaves: [usize; 4]) -> Option<[f64; 4]> {
    let stats = |index: usize| float_stats(row_group.column(index).statistics()?);
    valid_bbox([
        stats(leaves[0])?.0,
        stats(leaves[1])?.0,
        stats(leaves[2])?.1,
        stats(leaves[3])?.1,
    ])
}

fn valid_bbox(bbox: [f64; 4]) -> Option<[f64; 4]> {
    (bbox.iter().all(|value| value.is_finite()) && bbox[0] <= bbox[2] && bbox[1] <= bbox[3])
        .then_some(bbox)
}

fn float_stats(stats: &Statistics) -> Option<(f64, f64)> {
    match stats {
        Statistics::Double(stats) => Some((*stats.min_opt()?, *stats.max_opt()?)),
        Statistics::Float(stats) => {
            Some((f64::from(*stats.min_opt()?), f64::from(*stats.max_opt()?)))
        }
        _ => None,
    }
}

fn int64_stats(stats: &Statistics) -> Option<(i64, i64)> {
    match stats {
        Statistics::Int64(stats) => Some((*stats.min_opt()?, *stats.max_opt()?)),
        _ => None,
    }
}

fn to_micros(value: i64, unit: TimeUnit) -> Option<i64> {
    match unit {
        TimeUnit::Second => value.checked_mul(1_000_000),
        TimeUnit::Millisecond => value.checked_mul(1_000),
        TimeUnit::Microsecond => Some(value),
        TimeUnit::Nanosecond => Some(value.div_euclid(1_000)),
    }
}

fn area(bbox: &[f64; 4]) -> f64 {
    (bbox[2] - bbox[0]) * (bbox[3] - bbox[1])
}

fn union(bboxes: &[[f64; 4]]) -> [f64; 4] {
    bboxes.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |acc, bbox| {
            [
                acc[0].min(bbox[0]),
                acc[1].min(bbox[1]),
                acc[2].max(bbox[2]),
                acc[3].max(bbox[3]),
            ]
        },
    )
}

fn span(time: (i64, i64)) -> f64 {
    let span = i128::from(time.1) - i128::from(time.0);
    span.max(1) as f64
}

fn overlap_fraction(a: (f64, f64), b: (f64, f64)) -> f64 {
    let union = a.1.max(b.1) - a.0.min(b.0);
    let intersection = (a.1.min(b.1) - a.0.max(b.0)).max(0.0);
    if union > 0.0 {
        intersection / union
    } else {
        0.0
    }
}

fn time_range(time: (i64, i64)) -> (f64, f64) {
    let start = time.0 as f64;
    (start, start + span(time))
}

fn mean_pairwise<T>(items: &[T], fraction: impl Fn(&T, &T) -> f64) -> f64 {
    let items = &items[..items.len().min(MAX_PAIRWISE_EXTENTS)];
    let mut total = 0.0;
    let mut pairs = 0usize;
    for (i, a) in items.iter().enumerate() {
        for b in &items[i + 1..] {
            total += fraction(a, b);
            pairs += 1;
        }
    }
    if pairs == 0 {
        0.0
    } else {
        total / pairs as f64
    }
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (total, count) = values.fold((0.0, 0usize), |(total, count), value| {
        (total + value, count + 1)
    });
    if count == 0 {
        0.0
    } else {
        total / count as f64
    }
}

fn order_ratio(ratio: f64) -> Option<f64> {
    (ratio.is_finite() && ratio > ORDER_THRESHOLD).then_some(ratio)
}

fn spatial_order(extents: &[Extent]) -> Vec<Finding> {
    let bboxes: Vec<[f64; 4]> = extents.iter().filter_map(|extent| extent.bbox).collect();
    if bboxes.len() < 2 {
        return Vec::new();
    }
    let file_area = area(&union(&bboxes));
    let (ratio, description) = if file_area > 0.0 {
        (
            mean(bboxes.iter().map(area)) / file_area,
            "average row group bbox covers",
        )
    } else {
        (
            mean_pairwise(&bboxes, |a, b| overlap_fraction((a[0], a[2]), (b[0], b[2]))),
            "row group x ranges overlap by",
        )
    };
    let Some(ratio) = order_ratio(ratio) else {
        return Vec::new();
    };
    vec![Finding::new(
        "bp.spatial_order",
        Severity::Warning,
        Section::GeoparquetBestPractices,
        format!(
            "The file may not be spatially ordered: the {description} {:.0}% of the file's extent",
            ratio * 100.0
        ),
        REF_SPATIAL_ORDER,
    )]
}

fn spatiotemporal_order(extents: &[Extent]) -> Vec<Finding> {
    let items: Vec<([f64; 4], (i64, i64))> = extents
        .iter()
        .filter_map(|extent| Some((extent.bbox?, extent.time?)))
        .collect();
    if items.len() < 2 {
        return Vec::new();
    }
    let bboxes: Vec<[f64; 4]> = items.iter().map(|(bbox, _)| *bbox).collect();
    let file_area = area(&union(&bboxes));
    let file_time = items.iter().fold((i64::MAX, i64::MIN), |acc, (_, time)| {
        (acc.0.min(time.0), acc.1.max(time.1))
    });
    let (ratio, description) = if file_area > 0.0 {
        (
            mean(items.iter().map(|(bbox, time)| area(bbox) * span(*time)))
                / (file_area * span(file_time)),
            "average row group covers",
        )
    } else {
        (
            mean_pairwise(&items, |(a, at), (b, bt)| {
                overlap_fraction((a[0], a[2]), (b[0], b[2]))
                    * overlap_fraction(time_range(*at), time_range(*bt))
            }),
            "row group x and time ranges overlap by",
        )
    };
    let Some(ratio) = order_ratio(ratio) else {
        return Vec::new();
    };
    vec![Finding::new(
        "stacbp.spatiotemporal_order",
        Severity::Warning,
        Section::StacGeoparquetBestPractices,
        format!(
            "The file may not be spatio-temporally ordered: the {description} {:.0}% of the file's space-time extent",
            ratio * 100.0
        ),
        REF_SPATIOTEMPORAL_ORDER,
    )]
}

fn id_prefix(extents: &[Extent]) -> Vec<Finding> {
    let ids: Vec<&(String, String)> = extents.iter().filter_map(|e| e.id.as_ref()).collect();
    if ids.len() < 2 {
        return Vec::new();
    }
    let unordered = ids.windows(2).filter(|pair| pair[0].1 > pair[1].0).count();
    if unordered == 0 {
        return Vec::new();
    }
    vec![Finding::new(
        "stacbp.id_prefix",
        Severity::Warning,
        Section::StacGeoparquetBestPractices,
        format!(
            "Item ids may not be prefixed with an ordering key: id ranges overlap at {unordered} of {} row group boundaries",
            ids.len() - 1
        ),
        REF_ID_PREFIX,
    )]
}

fn declared_geometry_types(ctx: &Context) -> Option<BTreeSet<String>> {
    let GeoState::Valid(metadata) = ctx.geo() else {
        return None;
    };
    let declared: BTreeSet<String> = metadata
        .columns
        .get(&metadata.primary_column)?
        .geometry_types
        .iter()
        .cloned()
        .collect();
    (!declared.is_empty()).then_some(declared)
}

fn geometry_types_mismatch(sample: &Sample, declared: &BTreeSet<String>) -> Vec<Finding> {
    let undeclared: Vec<&str> = sample
        .geometry_types
        .difference(declared)
        .map(String::as_str)
        .collect();
    if undeclared.is_empty() {
        return Vec::new();
    }
    vec![Finding::new(
        "geoparquet.geometry_types_mismatch",
        Severity::Warning,
        Section::Geoparquet,
        format!(
            "The first {} rows contain geometry types not listed in `geometry_types`: {}",
            sample.rows,
            undeclared.join(", ")
        ),
        REF_GEOMETRY_TYPES,
    )]
}

fn duplicate_id(sample: &Sample) -> Vec<Finding> {
    let mut counts: BTreeMap<(Option<&str>, &str), usize> = BTreeMap::new();
    for (collection, id) in &sample.ids {
        *counts
            .entry((collection.as_deref(), id.as_str()))
            .or_default() += 1;
    }
    let duplicates: Vec<(Option<&str>, &str)> = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(key, _)| key)
        .collect();
    if duplicates.is_empty() {
        return Vec::new();
    }
    let examples: Vec<String> = duplicates
        .iter()
        .take(MAX_DUPLICATE_EXAMPLES)
        .map(|(collection, id)| match collection {
            Some(collection) => format!("{collection}/{id}"),
            None => id.to_string(),
        })
        .collect();
    vec![Finding::new(
        "stac.duplicate_id",
        Severity::Error,
        Section::StacGeoparquet,
        format!(
            "{} ids appear more than once within a collection in the first {} rows, e.g. {}",
            duplicates.len(),
            sample.rows,
            examples.join(", ")
        ),
        REF_GUIDELINES,
    )]
}
