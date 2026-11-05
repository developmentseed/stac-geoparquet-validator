# stac-geoparquet validator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A public web tool at `https://validate.stac-geoparquet.org/` that validates a stac-geoparquet file (by href or upload) against Parquet, GeoParquet 1.0/1.1/2.0, stac-geoparquet, and both best-practice docs.

**Architecture:** A Rust library does all validation over `object_store` range reads (footer first, bounded sampling for data checks). A Rust Lambda behind an API Gateway HTTP API exposes `POST /api/validate` and `POST /api/uploads`. A React SPA renders reports. CloudFront serves both under one domain. CDK (TypeScript) deploys it to `us-west-2`.

**Tech Stack:** Rust (`parquet` 60 with `async`, `object_store`, `geospatial`, `zstd`, `snap`, `lz4`, `brotli`, `flate2` features; `arrow` 60; `object_store` 0.14; `jsonschema` 0.60; `lambda_http` 1; `aws-sdk-s3`; `tokio`; `serde`). Frontend: yarn, Vite, React, TypeScript, Chakra UI v3, vitest, prettier. Infra: AWS CDK v2 (TypeScript), `cargo-lambda-cdk`.

**Spec:** `docs/superpowers/specs/2026-11-05-stac-geoparquet-validator-design.md`

## Global Constraints

- **Never run git commands.** The user commits. Each task ends with a "checkpoint" step: stop and tell the user which files changed.
- Rust: no inline "what" comments; doc comments on all public items. Tests have no doc comments.
- Rust tests never assert message text, only check ids and severities.
- Frontend: yarn, TypeScript, Vite, React, Chakra UI, prettier.
- GitHub Actions: workflow files get a `name`; `uses:` steps get no `name`; `run:` steps get a `name`.
- AWS region `us-west-2`. The CloudFront certificate goes in `us-east-1`. Domain `validate.stac-geoparquet.org`. DNS is managed by hand on gandi.net.
- The service is public and anonymous. CORS allows any origin.
- `s3://` hrefs are read **unsigned**, except in the uploads bucket, which is read with the Lambda's credentials.
- Max upload size: 2 GB (2 × 1024³ bytes). Uploads expire after 1 day.
- Validation must finish within 25 s (Lambda timeout 29 s; API Gateway limit ~30 s).
- Every report carries `report_version: "1"`. Check ids (table below) are a public contract.
- Vendored JSON Schemas: GeoParquet `v1.0.0`, `v1.1.0`, `v2.0.0` tags of `opengeospatial/geoparquet` (`format-specs/schema.json`), and `radiantearth/stac-geoparquet-spec` `json-schema/metadata.json`.

## Check ids

The spec's check list, pinned to ids, severities and sections. † = data-dependent (Task 6).

| id | sev | section |
|---|---|---|
| `parquet.unreadable` | Error | parquet |
| `parquet.info` | Info | parquet |
| `geoparquet.metadata_missing` | Error | geoparquet |
| `geoparquet.metadata_invalid_json` | Error | geoparquet |
| `geoparquet.metadata_schema` | Error | geoparquet |
| `geoparquet.unknown_version` | Error | geoparquet |
| `geoparquet.primary_column_missing` | Error | geoparquet |
| `geoparquet.column_missing` | Error | geoparquet |
| `geoparquet.column_nested` | Error | geoparquet |
| `geoparquet.column_repeated` | Error | geoparquet |
| `geoparquet.encoding` | Error | geoparquet |
| `geoparquet.crs_mismatch` | Error | geoparquet |
| `geoparquet.geometry_types_duplicate` | Error | geoparquet |
| `geoparquet.covering_invalid` | Error | geoparquet |
| `geoparquet.native_types` | Compat | geoparquet |
| `geoparquet.native_without_metadata` | Warning | geoparquet |
| `geoparquet.info` | Info | geoparquet |
| `geoparquet.geometry_types_mismatch` † | Warning | geoparquet |
| `bp.compression` | Warning | geoparquet_best_practices |
| `bp.row_group_size` | Warning | geoparquet_best_practices |
| `bp.version` | Warning | geoparquet_best_practices |
| `bp.file_size` | Warning | geoparquet_best_practices |
| `bp.extension` | Warning | geoparquet_best_practices |
| `bp.spatial_order` † | Warning | geoparquet_best_practices |
| `stac.column_missing` | Error | stac_geoparquet |
| `stac.column_type` | Error | stac_geoparquet |
| `stac.crs` | Error | stac_geoparquet |
| `stac.metadata_missing` | Error | stac_geoparquet |
| `stac.metadata_schema` | Error | stac_geoparquet |
| `stac.version` | Error | stac_geoparquet |
| `stac.collection_id_mismatch` | Error | stac_geoparquet |
| `stac.deprecated_collection` | Warning | stac_geoparquet |
| `stac.type_column` | Warning | stac_geoparquet |
| `stac.duplicate_id` † | Error | stac_geoparquet |
| `stacbp.spatiotemporal_order` † | Warning | stac_geoparquet_best_practices |
| `stacbp.id_prefix` † | Warning | stac_geoparquet_best_practices |
| `stacbp.hive_partitioning` | Info | stac_geoparquet_best_practices |
| `sample.skipped` | Info | (section of the skipped check) |

Note: Parquet does not record the zstd level, so the spec's "Info: zstd level if determinable" is never emitted.

## Review Focus

1. **SSRF:** hrefs like `http://127.0.0.1:9001/...` (the Lambda runtime API), `http://169.254.169.254/`, private-range IPs, `file://`, or hostnames that resolve to those must be rejected before any fetch. → Task 7 tests.
2. **Not-a-parquet responses:** an HTML 404 page, an empty object, a file shorter than 8 bytes, or a footer length larger than the file must each give exactly `parquet.unreadable`, with no panic or giant allocation. A declared footer over 64 MiB is refused. → Task 2 tests.
3. **Degenerate-but-valid files:** zero rows, one row group, all-null geometry, zero-area bboxes (points). Ordering checks must not divide by zero or emit NaN. They emit nothing when there are fewer than 2 row groups. → Task 6 tests.
4. **Valid JSON, wrong shape:** `geo` with `columns: []` or `primary_column: 5`. Emit `geoparquet.metadata_schema` and skip the column-level checks, with no panic. → Task 3 test.
5. **Slow or huge remote files:** sampled checks past the deadline emit `sample.skipped` and the report still returns. → Task 6 test.

---

### Task 1: Backend crate and report types

**Files:**
- Create: `backend/Cargo.toml`, `backend/src/lib.rs`, `backend/src/report.rs`
- Test: `backend/tests/report.rs`

**Interfaces:**
- Produces (in `report.rs`, re-exported from `lib.rs`):
  - `enum Severity { Error, Warning, Compat, Info }` — serde `lowercase`.
  - `enum Section { Parquet, Geoparquet, GeoparquetBestPractices, StacGeoparquet, StacGeoparquetBestPractices }` — serde `snake_case`.
  - `struct Finding { id: &'static str, severity: Severity, section: Section, message: String, reference: &'static str }` with `Finding::new(id, severity, section, message: impl Into<String>, reference) -> Finding`.
  - `struct Summary { parquet_valid: bool, geoparquet_valid: bool, geoparquet_version: Option<String>, native_geospatial_types: bool, stac_geoparquet_valid: bool, counts: Counts }`, and `struct Counts { error, warning, compat, info: usize }`.
  - `struct Report { report_version: &'static str, href: String, summary: Summary, findings: Vec<Finding> }` with `Report::new(href: String, findings: Vec<Finding>, geoparquet_version: Option<String>, native_geospatial_types: bool) -> Report`. It derives the summary: `parquet_valid` = no Error in Parquet; `geoparquet_valid` = parquet_valid and no Error in Geoparquet; `stac_geoparquet_valid` = no Error in Parquet, Geoparquet or StacGeoparquet. Findings are sorted by (section, severity Error→Info).
  - All types are `Serialize + Deserialize + Debug + Clone`.

- [ ] **Step 1:** Create `backend/Cargo.toml` (package `stac-geoparquet-validator`, edition 2024) with the Tech Stack deps. Bins come later.
- [ ] **Step 2: Write failing tests** in `backend/tests/report.rs`:
  - `summary_counts_by_severity`: 1 Error (parquet), 2 Warning, 1 Info → counts `{1,2,0,1}` and `parquet_valid == false`.
  - `geoparquet_invalid_does_not_affect_parquet_valid`: an Error in Geoparquet → `parquet_valid`, `!geoparquet_valid`, `!stac_geoparquet_valid`.
  - `best_practice_warnings_keep_valid`: only Warnings/Compat → all three `*_valid` true.
  - `serializes_report_version`: `serde_json::to_value(report)["report_version"] == "1"`, and severity serializes as `"compat"`.
- [ ] **Step 3:** Run `cargo test --test report` → FAIL (types missing).
- [ ] **Step 4:** Implement `report.rs` per the Interfaces block.
- [ ] **Step 5:** Run `cargo test --test report` → PASS. Run `cargo clippy --all-targets -- -D warnings` → clean.
- [ ] **Step 6: Checkpoint** — stop for the user to commit.

### Task 2: Fixtures, file context, and Parquet checks

**Files:**
- Create: `backend/src/context.rs`, `backend/src/checks/mod.rs`, `backend/src/checks/parquet.rs`
- Create: `backend/tests/common/mod.rs` (fixture builder, reused by every later backend test)
- Test: `backend/tests/parquet.rs`

**Interfaces:**
- Produces:
  - `context.rs`: `struct Context { href: String, path: object_store::path::Path, size: u64, metadata: Arc<ParquetMetaData>, arrow_schema: arrow_schema::SchemaRef, key_value: HashMap<String, String> }` and `async fn Context::load(store: Arc<dyn ObjectStore>, path: Path, href: String) -> Result<Context, Finding>`. On failure it returns a `parquet.unreadable` Finding. It reads via `ParquetObjectReader` + `ParquetMetaDataReader` with the page index **not** loaded. Before reading the footer, it reads the last 8 bytes itself to validate the `PAR1` magic and refuse a footer length over 64 MiB or larger than `size - 8`.
  - `checks/mod.rs`: `type Check = fn(&Context) -> Vec<Finding>;` and `pub fn footer_checks() -> Vec<Check>` (later tasks append theirs).
  - `checks/parquet.rs`: `fn info(&Context) -> Vec<Finding>`. It emits one `parquet.info` with format version, `created_by`, rows, row groups and size in the message.
  - `tests/common/mod.rs`:
    - `fn stac_batch(n: usize) -> RecordBatch`: a valid stac-geoparquet batch. Columns: `type`-less; `stac_extensions: List<Utf8>`; `id: Utf8` (`"item-{i:06}"`); `geometry: Binary` WKB points marching west→east; `bbox: Struct{xmin,ymin,xmax,ymax: Float64}`; `links: List<Struct{href,rel,type,title: Utf8}>`; `assets: Struct{data: Struct{href,type: Utf8, roles: List<Utf8>}}`; `collection: Utf8`; `datetime: Timestamp(µs, "UTC")`, increasing.
    - `fn geo_metadata(version: &str) -> serde_json::Value`: valid `geo` for 1.1.0 with a bbox covering.
    - `fn stac_metadata() -> serde_json::Value`: `{"version":"1.1.0","collections":{"test":{...minimal valid collection with id "test"}}}`.
    - `struct Fixture { batch, kv: Vec<(String,String)>, props: WriterProperties, native: bool }` with `Fixture::valid()` (zstd, row group 100_000) and `fn bytes(&self) -> Bytes`. When `native`, the geometry field gets the `geoarrow.wkb` extension type, so parquet's `geospatial` feature writes the native `GEOMETRY` logical type.
    - `async fn validate_bytes(bytes: Bytes, name: &str) -> Report`: puts the bytes in `object_store::memory::InMemory` at `name` and calls `validate` (Task 7). Until Task 7 lands, tests call `Context::load` + checks directly through a temporary `pub fn run_footer_checks(&Context) -> Vec<Finding>` in `checks/mod.rs`.

- [ ] **Step 1: Write failing tests** in `backend/tests/parquet.rs`:
  - `valid_file_emits_parquet_info` → exactly one `parquet.info`, no `parquet.unreadable`.
  - `html_body_is_unreadable` (bytes `b"<html>404</html>"`) → `Err` whose id is `parquet.unreadable`.
  - `empty_file_is_unreadable`, `seven_bytes_is_unreadable`.
  - `footer_length_larger_than_file_is_unreadable` (valid magic, footer len `u32::MAX`).
  - `zero_row_file_loads` → `Ok`, `parquet.info` present.
- [ ] **Step 2:** Run `cargo test --test parquet` → FAIL.
- [ ] **Step 3:** Implement `Context::load`, `checks/parquet.rs`, `checks/mod.rs`, and the fixture builder.
- [ ] **Step 4:** Run `cargo test --test parquet` → PASS.
- [ ] **Step 5: Checkpoint.**

### Task 3: GeoParquet checks

**Files:**
- Create: `backend/schemas/geoparquet-1.0.0.json`, `geoparquet-1.1.0.json`, `geoparquet-2.0.0.json` (downloaded from the tags in Global Constraints)
- Create: `backend/src/geo.rs`, `backend/src/checks/geoparquet.rs`
- Test: `backend/tests/geoparquet.rs`

**Interfaces:**
- Produces:
  - `geo.rs`: `struct GeoMetadata { version: String, primary_column: String, columns: BTreeMap<String, GeoColumn> }` and `struct GeoColumn { encoding: String, geometry_types: Vec<String>, crs: Option<Option<serde_json::Value>>, covering: Option<serde_json::Value>, bbox: Option<Vec<f64>> }`. In `crs`, outer `None` = key absent and `Some(None)` = explicit null. Plus `enum GeoState { Missing, InvalidJson, Invalid(serde_json::Value), Valid(GeoMetadata) }` and `fn Context::geo(&self) -> GeoState` (memoize with `OnceLock` on `Context`). Also `fn Context::native_geo_columns(&self) -> Vec<(String, NativeGeo)>`, where `NativeGeo { crs: Option<String>, geography: bool }` comes from `LogicalType::Geometry` / `Geography` on root columns.
  - `fn is_crs84(geo_crs: Option<&Option<Value>>, native_crs: Option<&str>) -> bool`: true when absent, or PROJJSON `id` is OGC/CRS84 or EPSG/4326, or the string is `OGC:CRS84`/`EPSG:4326`. Task 5 uses this.
  - `fn Context::geoparquet_version(&self) -> Option<String>`: `geo.version` when valid.
  - Check fns appended to `footer_checks()`.

Rules beyond the check table:
- Pick the schema by `major.minor` of `version`. If there's no match, emit `geoparquet.unknown_version` and skip the schema step. A schema failure emits one `geoparquet.metadata_schema` per error (cap 20) and skips the column-level checks.
- `geoparquet.metadata_missing` only when there's no `geo` key **and** no native geo columns. Native columns without `geo` → `geoparquet.native_without_metadata` instead.
- Encoding: 1.0 → `WKB` only. 1.1 → `WKB`, `point`, `linestring`, `polygon`, `multipoint`, `multilinestring`, `multipolygon`. 2.0 → `WKB` **and** a native logical type on that column.
- `geoparquet.crs_mismatch` (any version) when both the `geo` crs and native crs are present and `is_crs84` disagrees between them. A native `srid:0` must pair with a `geo` crs of `null`.
- Covering: the column exists at root; child names exactly `xmin,ymin,[zmin,]xmax,ymax[,zmax]`; all FLOAT or all DOUBLE; repetition equals the geometry column's.
- `geoparquet.native_types` (Compat), once per native column. The message names gpq / arrow-go < v18 as known failures.
- `geoparquet.info`: version, primary column, geometry types, CRS (or "OGC:CRS84 (default)"), bbox.

- [ ] **Step 1:** Download the three schema files into `backend/schemas/` with `gh api ... -H "Accept: application/vnd.github.raw"` at each tag.
- [ ] **Step 2: Write failing tests** (each asserts the presence or absence of ids):
  - `valid_1_1_has_no_errors` → no Error findings; `geoparquet.info` present.
  - `no_geo_no_native_is_missing` → `geoparquet.metadata_missing`.
  - `native_without_geo` (`native: true`, no `geo` kv) → `geoparquet.native_without_metadata` + `geoparquet.native_types`, **no** `geoparquet.metadata_missing`.
  - `valid_2_0_native` → no Error; `geoparquet.native_types` present.
  - `version_2_0_without_native_is_encoding_error`.
  - `geo_not_json` → `geoparquet.metadata_invalid_json`.
  - `geo_wrong_shape` (`{"version":"1.1.0","primary_column":5,"columns":[]}`) → `geoparquet.metadata_schema`, no panic.
  - `unknown_version` (`"9.0.0"`) → `geoparquet.unknown_version`.
  - `primary_column_not_in_columns`, `geo_column_not_in_schema` → their ids.
  - `duplicate_geometry_types` (`["Point","Point"]`) → `geoparquet.geometry_types_duplicate`.
  - `covering_wrong_child_name` (child `minx`) → `geoparquet.covering_invalid`.
  - `crs_mismatch` (geo crs EPSG:4326 PROJJSON id, native crs `EPSG:32633`) → `geoparquet.crs_mismatch`.
- [ ] **Step 3:** Run `cargo test --test geoparquet` → FAIL.
- [ ] **Step 4:** Implement `geo.rs` and `checks/geoparquet.rs`. Embed the schemas with `include_str!` and compile them once (`LazyLock`).
- [ ] **Step 5:** Run `cargo test --test geoparquet` → PASS.
- [ ] **Step 6: Checkpoint.**

### Task 4: GeoParquet distribution best-practice checks (footer-only)

**Files:**
- Create: `backend/src/checks/best_practices.rs`
- Test: `backend/tests/best_practices.rs`

**Interfaces:**
- Consumes: `Context`, `Context::geo()`, `Context::geoparquet_version()`.
- Produces: check fns `compression`, `row_group_size`, `version`, `file_size`, `extension`, appended to `footer_checks()`.

Rules:
- `bp.compression`: any column chunk whose codec is not ZSTD. One finding listing the codecs seen.
- `bp.row_group_size`: warn if any row group **except the last** has rows outside `50_000..=150_000`. Don't warn when the file has only one row group.
- `bp.version`: GeoParquet `1.0.*`, or `1.1.*` whose primary column has no `covering`.
- `bp.file_size`: `size > 2 * 1024^3`.
- `bp.extension`: path ends with `.geoparquet`.

- [ ] **Step 1: Write failing tests:**
  - `snappy_warns` → `bp.compression`.
  - `zstd_does_not_warn`.
  - `small_row_groups_warn` (3 groups of 1_000 rows, then 500) → `bp.row_group_size`.
  - `single_small_row_group_ok` (10 rows) → none.
  - `version_1_0_warns`, `version_1_1_without_covering_warns`, `version_2_0_ok`.
  - `geoparquet_extension_warns` (name `x.geoparquet`).
- [ ] **Step 2:** Run `cargo test --test best_practices` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run → PASS.
- [ ] **Step 5: Checkpoint.**

### Task 5: stac-geoparquet spec checks (footer-only)

**Files:**
- Create: `backend/schemas/stac-geoparquet-metadata.json`
- Create: `backend/src/checks/stac.rs`
- Test: `backend/tests/stac.rs`

**Interfaces:**
- Consumes: `Context.arrow_schema`, `Context.key_value`, `geo::is_crs84`, `Context::native_geo_columns()`.
- Produces: check fns appended to `footer_checks()`.

Rules:
- Required root columns: `stac_extensions`, `id`, `geometry`, `bbox`, `links`, `assets` → one `stac.column_missing` each.
- `stac.column_type`, one per offending column:
  - `stac_extensions` must be a List/LargeList of Utf8/LargeUtf8/Utf8View.
  - `id` must be a string type.
  - `bbox` must be a Struct of only Float32/Float64 fields named per the GeoParquet bbox column rule (4 or 6).
  - `links` must be a List of Struct containing `href` and `rel` string fields.
  - `assets` must be a Struct whose every child is a Struct containing `href`.
  - Each of `datetime, start_datetime, end_datetime, created, updated, expires, published, unpublished`, if present, must be a Timestamp.
- `stac.crs`: the `geometry` column's CRS is not OGC:CRS84 (per `is_crs84`, using both the geo and native crs).
- The `stac-geoparquet` kv:
  - absent → `stac.metadata_missing`
  - fails schema → `stac.metadata_schema`
  - `version` missing or not in `{"1.0.0","1.1.0"}` → `stac.version`
  - a `collections` key ≠ `collections[key].id` → `stac.collection_id_mismatch`
  - `collection` key present → `stac.deprecated_collection`
- A `type` column present → `stac.type_column`.

- [ ] **Step 1:** Download `json-schema/metadata.json` from `radiantearth/stac-geoparquet-spec` main into `backend/schemas/stac-geoparquet-metadata.json`.
- [ ] **Step 2: Write failing tests:**
  - `valid_stac_has_no_stac_errors`
  - `assets_as_string_is_type_error` (the #555 shape) → `stac.column_type`
  - `datetime_as_string_is_type_error`
  - `missing_links` → `stac.column_missing`
  - `links_without_rel` → `stac.column_type`
  - `no_stac_metadata` → `stac.metadata_missing`
  - `bad_version` (`"0.9.0"`) → `stac.version`
  - `collection_key_mismatch`
  - `deprecated_collection_warns`
  - `type_column_warns`
  - `utm_crs` (native crs `EPSG:32633`) → `stac.crs`
- [ ] **Step 3:** Run `cargo test --test stac` → FAIL.
- [ ] **Step 4:** Implement.
- [ ] **Step 5:** Run → PASS.
- [ ] **Step 6: Checkpoint.**

### Task 6: Data-dependent checks with sampling and deadline

**Files:**
- Create: `backend/src/sample.rs`, `backend/src/wkb.rs`, `backend/src/checks/data.rs`
- Test: `backend/tests/data.rs`

**Interfaces:**
- Consumes: `Context`, the store and path, `Context::geo()`.
- Produces:
  - `wkb.rs`: `fn geometry_type(wkb: &[u8]) -> Option<String>`. It returns the GeoParquet name (e.g. `"Point Z"`) and handles both byte orders, ISO codes (+1000 Z, +2000 M, +3000 ZM) and EWKB flags (0x80000000 Z, 0x40000000 M).
  - `sample.rs`: `struct Sample { ids: Vec<(Option<String>, String)> /* (collection, id) */, geometry_types: BTreeSet<String> }` and `async fn Sample::read(store, path, ctx: &Context, deadline: Instant) -> Option<Sample>`. It reads only `id`, `collection` (if present) and the primary geometry column from up to **200_000 rows**. Row groups are taken in order until the cap. It returns `None` when the deadline passes.
  - `fn row_group_extents(ctx: &Context) -> Vec<Extent>`, with `struct Extent { bbox: Option<[f64;4]>, time: Option<(i64,i64)>, id: Option<(String,String)> }`, taken from statistics only. bbox comes from native geo statistics, else from the covering columns' min/max. time comes from `datetime` stats (fallback `start_datetime` min / `end_datetime` max). id comes from `id` min/max.
  - `async fn data_checks(store, path, ctx: &Context, deadline: Instant) -> Vec<Finding>`.

Heuristics (pinned):
- `bp.spatial_order`: with ≥ 2 row groups that have bboxes, let `file` be the union bbox. Warn when `mean(area(rg)) / area(file) > 0.5`. If `area(file) == 0`, use the mean pairwise overlap fraction of the extents' x-ranges, and skip if that's also 0. Never emit NaN.
- `stacbp.spatiotemporal_order`: same, with volume = area × time span. Skip when time stats are absent. Treat a zero span as 1 µs.
- `stacbp.id_prefix`: with ≥ 2 row groups that have id stats, warn unless `rg[i].max <= rg[i+1].min` for all i.
- `geoparquet.geometry_types_mismatch`: the sample's types ⊄ declared `geometry_types`, ignoring an empty declared list.
- `stac.duplicate_id`: duplicate `(collection, id)` within the sample. Report up to 5 examples.
- `stacbp.hive_partitioning`: any path segment matching `^[^=/]+=[^/]+$`.
- When the deadline passes before a sampled check runs, emit `sample.skipped` (Info) naming that check, in that check's section.

- [ ] **Step 1: Write failing tests:**
  - `wkb_point_le`, `wkb_polygon_z_iso`, `wkb_point_m_ewkb_be`, `wkb_garbage_is_none` (unit tests in `wkb.rs`).
  - `shuffled_points_warn_spatial_order`: 4 row groups, points randomly placed over the world → `bp.spatial_order`.
  - `sorted_points_ok`: points marching west→east across row groups → none.
  - `single_row_group_no_order_checks`, `zero_rows_no_panic`, `all_points_same_location_no_nan`.
  - `id_prefix_unsorted_warns`, `id_prefix_sorted_ok`.
  - `duplicate_ids_error`, `same_id_different_collection_ok`.
  - `declared_point_but_polygon_present` → `geoparquet.geometry_types_mismatch`.
  - `hive_path_info` (name `year=2016/items.parquet`).
  - `past_deadline_skips` (deadline = `Instant::now()`) → `sample.skipped` present, no sampled ids.
- [ ] **Step 2:** Run `cargo test --test data` and `cargo test --lib wkb` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run → PASS.
- [ ] **Step 5: Checkpoint.**

### Task 7: Href resolution, SSRF guard, `validate` entry point, #555 regression

**Files:**
- Create: `backend/src/resolve.rs`, `backend/src/validate.rs`
- Modify: `backend/src/lib.rs` (export `validate`, `resolve`, `Options`); remove the temporary `run_footer_checks` and switch the earlier tests to `common::validate_bytes`
- Test: `backend/tests/resolve.rs`, `backend/tests/regression.rs`

**Interfaces:**
- Produces:
  - `struct Options { uploads_bucket: Option<String>, budget: Duration }`. `Default` is 25 s with no bucket.
  - `async fn resolve(href: &str, options: &Options) -> Result<(Arc<dyn ObjectStore>, Path), Finding>`. The Err is `parquet.unreadable`.
    - `http(s)://`: parse the URL, resolve the host with `tokio::net::lookup_host`, and reject loopback, private, link-local, unspecified, CGNAT (100.64/10), and IPv6 ULA/link-local addresses. Then use `HttpBuilder` with redirects disabled. A redirect would bypass the check, so disable it via `ClientOptions`.
    - `s3://bucket/key`: use `AmazonS3Builder::new().with_bucket_name(bucket)`, with the region from `object_store::aws::resolve_bucket_region`. Use `.with_skip_signature(true)` unless `bucket == options.uploads_bucket`, in which case use `from_env()`.
    - Any other scheme → Err.
  - `async fn validate(store: Arc<dyn ObjectStore>, path: Path, href: String, options: &Options) -> Report`: `Context::load` → footer checks → `data_checks` with deadline `now + budget` → `Report::new(...)`.
  - `async fn validate_href(href: &str, options: &Options) -> Report`: resolve + validate. A resolve failure becomes a Report with that single finding.

- [ ] **Step 1: Write failing tests** in `resolve.rs`:
  - `rejects_loopback` (`http://127.0.0.1:9001/x.parquet`)
  - `rejects_metadata_ip` (`http://169.254.169.254/`)
  - `rejects_private` (`http://10.0.0.1/`)
  - `rejects_localhost_name` (`http://localhost/`)
  - `rejects_file_scheme`
  - `rejects_ipv6_loopback` (`http://[::1]/`)
  - `accepts_s3_scheme_shape`: parse only, without network — factor the scheme/bucket parsing into `fn parse_href(&str) -> Result<Href, Finding>` and test that.
- [ ] **Step 2: Write** `regression.rs::stac_map_555`, `#[ignore]` because it needs the network. Run `validate_href` on `https://data.source.coop/tge-labs/s2-stac-geoparquet/sentinel-2-l2a/year=2016/items.parquet` and assert the presence of `stac.column_type`, `geoparquet.native_types`, `stacbp.hive_partitioning`, and that `parquet_valid` is true.
- [ ] **Step 3:** Run `cargo test --test resolve` → FAIL.
- [ ] **Step 4:** Implement, then migrate the earlier tests to `validate_bytes`.
- [ ] **Step 5:** Run `cargo test` → all PASS. Run `cargo test --test regression -- --ignored` → PASS. If the file has moved, report that to the user rather than changing the assertion.
- [ ] **Step 6: Checkpoint.**

### Task 8: Lambda HTTP API

**Files:**
- Create: `backend/src/bin/lambda.rs`, `backend/src/api.rs`
- Modify: `backend/Cargo.toml` (`[[bin]] name = "lambda"`)
- Test: `backend/tests/api.rs`

**Interfaces:**
- Produces:
  - `api.rs`: `async fn handle(req: lambda_http::Request, state: &State) -> Result<Response<Body>, Error>` with `struct State { options: Options, s3: Option<aws_sdk_s3::Client> }`.
  - `POST /api/validate`, body `{"href": string}` → 200 with the `Report` JSON. Malformed body → 400 `{"error": string}`.
  - `POST /api/uploads`, body `{"filename": string, "size": number}` → 200 `{"upload_url": string, "href": "s3://{bucket}/uploads/{uuid}/{sanitized filename}"}`. `size > 2*1024^3` or `size == 0` → 400. The presigned PUT comes from `aws_sdk_s3` with `content_length(size)`, so S3 enforces the size, and expires in 15 min. Sanitize the filename to `[A-Za-z0-9._-]`; it can't be empty.
  - `OPTIONS *` → 204. Every response has `Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods: POST, OPTIONS`, `Access-Control-Allow-Headers: content-type`.
  - Other path/method → 404.
  - The env var `UPLOADS_BUCKET` sets `options.uploads_bucket` and enables `/api/uploads`; without it, uploads return 503.
- [ ] **Step 1: Write failing tests** (build `Request`s directly; `s3: None` except the presign tests, which use a client with static fake credentials and region `us-west-2` and need no network):
  - `validate_bad_body_400`
  - `validate_returns_report` (href `file:///x` → 200, `summary.parquet_valid == false`)
  - `uploads_too_big_400`
  - `uploads_returns_presigned_put` (`upload_url` host contains the bucket; `href` starts with `s3://{bucket}/uploads/`)
  - `filename_sanitized` (`"../a b.parquet"` → key ends with `a_b.parquet` or similar, with no `/..`)
  - `cors_headers_present`
  - `options_204`
  - `unknown_404`
- [ ] **Step 2:** Run `cargo test --test api` → FAIL.
- [ ] **Step 3:** Implement `api.rs` and `bin/lambda.rs` (`lambda_http::run(service_fn(...))`).
- [ ] **Step 4:** Run → PASS. Install cargo-lambda if missing (`brew install cargo-lambda/tap/cargo-lambda`), then run `cargo lambda build --release --arm64 --bin lambda` → succeeds.
- [ ] **Step 5: Checkpoint.**

### Task 9: Frontend SPA

**Files:**
- Create: `frontend/` via `yarn create vite frontend --template react-ts`, then add `@chakra-ui/react` (v3), `@emotion/react`, `vitest`, `@testing-library/react`, `jsdom`, `prettier`.
- Create: `frontend/src/api.ts`, `frontend/src/types.ts`, `frontend/src/components/{HrefForm,UploadButton,Report,Headline,Findings}.tsx`, `frontend/src/App.tsx`
- Test: `frontend/src/api.test.ts`, `frontend/src/components/Report.test.tsx`

**Interfaces:**
- Consumes: the Task 8 HTTP API. `types.ts` mirrors `Report`, `Summary`, `Finding`, `Severity` and `Section` exactly (snake_case field names).
- Produces:
  - `api.ts`: `validate(href: string): Promise<Report>` and `upload(file: File): Promise<string>`. `upload` calls `/api/uploads`, then PUTs the file to `upload_url` with a `Content-Length` matching its size, and returns the `href`. The base URL comes from `import.meta.env.VITE_API_BASE ?? ""`, and Vite dev proxies `/api` to `VITE_API_PROXY`.
  - `App`: reads `?href=` on load and validates when present. Submitting the form sets `?href=` via `history.replaceState`. Uploads validate after upload but do **not** put the `s3://` uploads href in the URL, since it expires in a day.
  - `Headline`: four answers — Parquet valid, GeoParquet valid, GeoParquet version (or "native types, no geo metadata", or "—"), stac-geoparquet valid — plus counts by severity.
  - `Findings`: grouped by section in the `Section` order, with severity filter toggles (all on by default). Each finding shows its severity badge, id, message and a reference link.
- [ ] **Step 1: Write failing tests:**
  - `api.test.ts::validate posts href` (stub `fetch` and check the body is `{"href":...}`)
  - `api.test.ts::upload puts file then returns href`
  - `Report.test.tsx::renders four headline answers`
  - `Report.test.tsx::severity filter hides warnings`
- [ ] **Step 2:** Run `yarn vitest run` → FAIL.
- [ ] **Step 3:** Implement. Use Chakra's provider at the root, and keep it to one screen.
- [ ] **Step 4:** Run `yarn vitest run` → PASS, `yarn tsc -b` → clean, `yarn prettier --check .` → clean, `yarn build` → `dist/` produced.
- [ ] **Step 5:** Manual check: run `cargo lambda watch` in `backend/`, set `VITE_API_PROXY=http://localhost:9000/lambda-url/lambda` in `frontend/.env.local`, then `yarn dev`. Validate the #555 URL and see the `stac.column_type` error and the native-types Compat.
- [ ] **Step 6: Checkpoint.**

### Task 10: Infrastructure, README, CI

**Files:**
- Create: `infra/` (CDK app: `bin/app.ts`, `lib/certificate-stack.ts`, `lib/validator-stack.ts`, `test/validator-stack.test.ts`)
- Create: `README.md`, `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `backend/` (built by `RustFunction` from `cargo-lambda-cdk` with `binaryName: "lambda"`, ARM64), and `frontend/dist`.
- Produces:
  - `CertificateStack` (`us-east-1`): an ACM certificate for `validate.stac-geoparquet.org` with `CertificateValidation.fromDns()`, so no hosted zone is involved. The Gandi validation CNAME shows up in the ACM console / CloudFormation events.
  - `ValidatorStack` (`us-west-2`, `crossRegionReferences: true`):
    - **Uploads bucket:** private, lifecycle expiration 1 day, CORS allowing `PUT` from `https://validate.stac-geoparquet.org` and `http://localhost:5173`.
    - **Lambda:** `RustFunction`, ARM64, 1024 MB, 29 s timeout, env `UPLOADS_BUCKET`, with read/put on the uploads bucket.
    - **HTTP API:** routes `POST /api/validate`, `POST /api/uploads`, `OPTIONS /api/{proxy+}`. Default stage throttling is rate 10 req/s, burst 20.
    - **Site:** a private bucket with `BucketDeployment` from `../frontend/dist`. A CloudFront distribution serves the default behavior from the site bucket via OAC, with SPA fallback (403/404 → `/index.html`). The `/api/*` behavior goes to the HTTP API origin with caching disabled, the `AllViewerExceptHostHeader` origin request policy, and allowed methods ALL. The distribution uses the alias `validate.stac-geoparquet.org` and the cross-region cert.
    - **Outputs:** `DistributionDomainName`.
- [ ] **Step 1:** `npx cdk init app --language typescript` in `infra/`, switch to yarn, and add `cargo-lambda-cdk`.
- [ ] **Step 2: Write failing tests** (`aws-cdk-lib/assertions`):
  - `uploads bucket expires after one day`
  - `lambda has 29s timeout and UPLOADS_BUCKET env`
  - `http api has throttling`
  - `distribution has api behavior and alias`
- [ ] **Step 3:** Run `yarn test` → FAIL.
- [ ] **Step 4:** Implement the stacks. `bin/app.ts` builds both stacks with `env.account` from `CDK_DEFAULT_ACCOUNT`.
- [ ] **Step 5:** Run `yarn test` → PASS, then `yarn cdk synth` → succeeds. Requires `frontend/dist` and cargo-lambda.
- [ ] **Step 6:** Write `README.md`: what it is, the local dev commands (Task 9 Step 5), the API (endpoints, request/response, link to the check-id table, now moved to `docs/checks.md`), and deploy steps:
  1. `yarn build` in `frontend/`.
  2. `yarn cdk deploy --all` in `infra/`.
  3. While the cert stack waits, add the ACM validation CNAME in Gandi.
  4. Add `validate` CNAME → `DistributionDomainName` in Gandi.
- [ ] **Step 7:** Move the Check ids table from this plan into `docs/checks.md`, with a reference URL per id matching what the code emits.
- [ ] **Step 8:** `.github/workflows/ci.yml` (`name: CI`), on push/PR, with three jobs:
  - **backend:** `actions/checkout`, `dtolnay/rust-toolchain@stable`, then `run` steps for `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` in `backend/`.
  - **frontend:** checkout, `actions/setup-node` (yarn cache), then `run` for `yarn install --immutable`, `yarn prettier --check .`, `yarn tsc -b`, `yarn vitest run`, `yarn build`.
  - **infra:** `yarn install` and `yarn test`.

  `run` steps get `name`s; `uses` steps don't.
- [ ] **Step 9: Checkpoint.** Tell the user that deploying (`cdk deploy`) and the Gandi records are theirs to run.
