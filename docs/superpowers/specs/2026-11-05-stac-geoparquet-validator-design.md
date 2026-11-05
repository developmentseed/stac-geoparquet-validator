# stac-geoparquet validator — design

## Intent

A web tool that takes a stac-geoparquet file (by URL or upload) and tells you whether you can trust it.

Two audiences:

- **Producers** checking a file before publishing it.
- **Consumers** who were handed a file that misbehaves (e.g. in stac-map) and want to know why.
  The motivating case is [developmentseed/stac-map#555](https://github.com/developmentseed/stac-map/issues/555): a file where `assets` is a `varchar` instead of a struct, and whose geometry uses Parquet's native `GEOMETRY` logical type, which makes older readers (gpq / arrow-go v16) panic.

The report must keep apart three things: what makes the file **invalid**, what is **valid but not best practice**, and what is **valid but some readers can't handle**.

## Goals

Given an href (or upload), report:

1. Whether the file is valid Parquet.
2. Whether the file is valid GeoParquet.
3. The GeoParquet version.
4. Deviations from best practices:
   - [Distributing GeoParquet](https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md)
   - [stac-geoparquet best practices](https://github.com/radiantearth/stac-geoparquet-spec/blob/main/docs/best-practices.md)
5. Conformance to the [stac-geoparquet spec](https://radiantearth.github.io/stac-geoparquet-spec).

## Non-goals

- Fixing or rewriting files.
- Validating every row. Data-dependent checks read a sample.
- Validating embedded STAC Collection objects against the STAC JSON Schema (the stac-geoparquet metadata schema explicitly doesn't). *(Builder may add later.)*
- Multi-file / directory (hive dataset) validation. One file per run; hive partitioning is only detected from the path.

## Anti-goals

- Downloading whole multi-GB files to validate them. Footer-first, range requests, bounded sampling.
- A JS Parquet reader deciding validity. The edge cases this tool exists for (native geo types, odd schemas) need a mature reader.

## Architecture (decided)

- **Frontend:** single-page app. React, TypeScript, Vite, Chakra UI, yarn, prettier. Thin UI; no validation logic.
- **Backend:** Rust on AWS Lambda. All validation happens here.
- **Inputs:**
  - **URL** (primary). The backend reads it directly with HTTP range requests — no CORS issue since it's server-side. Public S3 (`s3://` and https) and any range-capable HTTP server are read in place, never copied.
  - **Upload** (secondary). The SPA asks the backend for a presigned S3 PUT URL, uploads straight to an uploads bucket, then asks the backend to validate that object via the same range-read path. Bucket lifecycle deletes uploads after 1 day. Needed because Lambda's synchronous payload limit is ~6 MB.
- **Reading strategy:** footer and metadata first (covers most checks). Checks marked † below read a bounded sample of rows / row-group statistics.
- **Credentials (my call):** the backend reads `s3://` hrefs anonymously; it uses its own AWS credentials only for the uploads bucket. It can never be used to read private data the Lambda role can see.
- **Upload limits (my call):** max upload size 2 GB, enforced via the presigned request's conditions.
- **Spec versions (my call):** GeoParquet 2.0.0 was released 2026-11-05; vendor the schemas from the `v1.0.0`, `v1.1.0`, and `v2.0.0` tags.

## Severity levels (decided)

| Level   | Meaning                                       |
| ------- | --------------------------------------------- |
| Error   | Invalid against a spec                        |
| Warning | Deviates from a best practice                 |
| Compat  | Valid, but some readers are known to fail     |
| Info    | Reported fact (versions, counts, CRS, etc.)   |

Every finding carries: severity, a stable check id, a human message, and a link to the relevant spec/best-practice section.

## Checks (agreed)

† = needs row data or per-row-group statistics beyond the basic footer.

### Parquet

- Error: can't fetch, bad magic bytes, unparseable footer.
- Info: format version, `created_by`, row count, row group count, file size.

### GeoParquet (support 1.0, 1.1, 2.0, and native-types-without-`geo`)

- Error: `geo` metadata missing, not JSON, or fails the JSON Schema for its declared version.
- Error: `primary_column` not in `columns`; a geometry column missing, nested, or repeated.
- Error: `encoding` invalid for the version (2.0: WKB only, with native `GEOMETRY`/`GEOGRAPHY` logical type).
- Error: (2.0) `geo` CRS contradicts the native logical-type `crs`.
- Error: duplicate values in `geometry_types`.
- Error: malformed bbox covering (child names/types, nesting, repetition).
- Compat: native `GEOMETRY`/`GEOGRAPHY` logical type — older Arrow-based readers (e.g. gpq) fail.
  *(Decided: keep compat checks to known cases for now; no reader matrix. Add checks as real cases turn up.)*
- Warning: native geo types present but no `geo` metadata (readable, but not conformant GeoParquet 2.0).
- Info: GeoParquet version, geometry types, CRS, file bbox.
- Warning †: `geometry_types` doesn't match geometries observed in the sample.

### GeoParquet distribution best practices

- Warning: compression isn't zstd. Info: zstd level if determinable.
- Warning: row group size outside 50k–150k rows. *(My call: don't warn when the whole file has fewer rows than one row group.)*
- Warning: GeoParquet 1.0, or 1.1 without a bbox covering.
- Warning: file > ~2 GB → suggest partitioning.
- Warning †: not spatially ordered (heuristic: overlap of per-row-group geometry/bbox extents).
- Warning: `.geoparquet` extension.

### stac-geoparquet spec

- Error: missing required columns: `stac_extensions`, `id`, `geometry`, `bbox`, `links`, `assets`.
  *(Decided: `bbox` stays required per stac-geoparquet even though GeoParquet 2.0 best practices say to drop it.)*
- Error: wrong column types — `assets` not a struct; `links` not a list of structs with `href` + `rel`; `stac_extensions` not a list of strings; `datetime`, `start_datetime`, `end_datetime` (and other known datetime fields) not native timestamps.
- Error: geometry CRS not OGC:CRS84 (EPSG:4326 accepted as equivalent per GeoParquet).
- Error: `stac-geoparquet` metadata key missing, fails its [JSON Schema](https://github.com/radiantearth/stac-geoparquet-spec/blob/main/json-schema/metadata.json), `version` not `1.0.0`/`1.1.0`, or a `collections` map key ≠ its collection's `id`.
- Warning: deprecated `collection` key used; `type` column present.
- Error †: duplicate `id`s within a collection in the sample.

### stac-geoparquet best practices

- Warning †: not spatio-temporally ordered.
- Warning †: ids not prefixed with an ordering key (heuristic: `id` min/max statistics monotonic across row groups).
- Info: hive partitioning detected from path (e.g. `year=2016`).

## Interfaces (my calls)

- `POST /validate` `{ "href": "..." }` → JSON report `{ summary, findings[] }`. `summary` includes `parquet_valid`, `geoparquet_valid`, `geoparquet_version`, `stac_geoparquet_valid`, counts by severity.
- `POST /uploads` → `{ upload_url, href }` (presigned PUT + the `s3://` href to pass to `/validate`).
- **Public API (decided):** the service is public and anonymous, and other tools (e.g. stac-map, scripts) may call `/validate` directly. So: CORS open to any origin; the report carries a `report_version`; check ids are stable and documented; validation must finish inside API Gateway's ~30 s limit.
- SPA puts the href in the query string (`?href=...`) so reports are shareable links and stac-map could deep-link to it.
- SPA shows the four headline answers at the top, then findings grouped by section, filterable by severity.

## Deployment (my calls)

- Region: `us-west-2` (decided). The CloudFront certificate must be issued in `us-east-1`.
- Backend built with `cargo-lambda`, behind an API Gateway HTTP API with throttling (my call, because it's public and anonymous).
- SPA as static files on S3 + CloudFront; uploads bucket separate, with 1-day lifecycle and CORS allowing PUT from the SPA origin.
- Infrastructure as code with AWS CDK (TypeScript).
- Domain: `https://validate.stac-geoparquet.org/` (decided). The API lives under the same origin (e.g. `/api/*` routed by CloudFront) (my call). DNS for `stac-geoparquet.org` is on gandi.net and managed by hand (decided). Deployment outputs the ACM validation CNAME and the CloudFront domain, and the README documents adding both records in Gandi.

## Left to the builder

- Choice of Rust crates. Likely `parquet`/`arrow` (verify the version in use parses the native `GEOMETRY`/`GEOGRAPHY` logical types — this is the case that broke gpq), `object_store` for range reads over HTTP/S3, `jsonschema` for metadata schemas. Consider reusing rustac's stac-geoparquet pieces where they fit.
- Sample size and sampling strategy for † checks, and the exact ordering heuristics' thresholds.
- Request timeouts / max bytes read per validation, to keep Lambda bounded.
- Vendoring the GeoParquet 1.0/1.1/2.0 and stac-geoparquet JSON Schemas.
- Test fixtures: generate small files covering each check, plus the #555 file as a real-world regression case.
