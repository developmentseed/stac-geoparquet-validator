# Check ids

Every finding in a report carries a stable `id`. These ids are a public contract: they will not be renamed, and new checks get new ids. Messages are free text and may change.

Checks marked with † read a bounded sample of rows (or per-row-group statistics) in addition to the footer. The sample takes row groups in order, up to 200,000 rows and 128 MiB of compressed column chunks. When the sample cannot be read (the time budget runs out, the first row group alone exceeds the byte budget, or the rows cannot be read), a `sample.skipped` finding is emitted in place of the sampled checks.

The severities are `Error` (invalid against a spec), `Warning` (deviates from a best practice), `Compat` (valid, but some readers are known to fail) and `Info` (a reported fact).

| id | severity | section | what it flags | reference |
| --- | --- | --- | --- | --- |
| `parquet.unreadable` | Error | `parquet` | The file cannot be fetched, has bad magic bytes, an unparseable footer, or the read timed out. | <https://parquet.apache.org/docs/file-format/> |
| `parquet.info` | Info | `parquet` | Parquet format version, creator, row and row group counts, file size. | <https://parquet.apache.org/docs/file-format/> |
| `geoparquet.metadata_missing` | Error | `geoparquet` | The `geo` metadata key is missing. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#file-metadata> |
| `geoparquet.metadata_invalid_json` | Error | `geoparquet` | The `geo` metadata is not valid JSON. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#file-metadata> |
| `geoparquet.metadata_schema` | Error | `geoparquet` | The `geo` metadata fails the JSON Schema for its declared version. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#file-metadata> |
| `geoparquet.unknown_version` | Error | `geoparquet` | The declared GeoParquet version is not 1.0, 1.1 or 2.0. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#version-and-schema> |
| `geoparquet.primary_column_missing` | Error | `geoparquet` | `primary_column` is not in `columns`. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#file-metadata> |
| `geoparquet.column_missing` | Error | `geoparquet` | A geometry column listed in the metadata is missing from the schema. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#column-metadata> |
| `geoparquet.column_nested` | Error | `geoparquet` | A geometry column is nested. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#nesting> |
| `geoparquet.column_repeated` | Error | `geoparquet` | A geometry column is repeated. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#repetition> |
| `geoparquet.encoding` | Error | `geoparquet` | The encoding is invalid for the declared version. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#encoding> |
| `geoparquet.crs_mismatch` | Error | `geoparquet` | The `geo` CRS contradicts the native logical-type CRS (any version). | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#crs-parquet-property> |
| `geoparquet.geometry_types_duplicate` | Error | `geoparquet` | Duplicate values in `geometry_types`. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#geometry_types> |
| `geoparquet.covering_invalid` | Error | `geoparquet` | The bbox covering is malformed. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#covering> |
| `geoparquet.native_types` | Compat | `geoparquet` | Native `GEOMETRY`/`GEOGRAPHY` logical types, which some readers cannot handle. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#geometry-columns> |
| `geoparquet.native_without_metadata` | Warning | `geoparquet` | Native geospatial types without `geo` metadata. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#metadata> |
| `geoparquet.info` | Info | `geoparquet` | GeoParquet version, geometry types, CRS and bbox. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#column-metadata> |
| `geoparquet.geometry_types_mismatch` † | Warning | `geoparquet` | `geometry_types` does not match the geometries seen in the sample. | <https://github.com/opengeospatial/geoparquet/blob/v2.0.0/format-specs/geoparquet.md#geometry_types> |
| `bp.compression` | Warning | `geoparquet_best_practices` | Compression is not zstd. | <https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md#compression> |
| `bp.row_group_size` | Warning | `geoparquet_best_practices` | Row group size is outside 50k to 150k rows. | <https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md#row-group-size> |
| `bp.version` | Warning | `geoparquet_best_practices` | GeoParquet 1.0, or 1.1 without a bbox covering. | <https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md#efficient-spatial-access> |
| `bp.file_size` | Warning | `geoparquet_best_practices` | The file is larger than about 2 GB. | <https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md#spatial-partitioning> |
| `bp.extension` | Warning | `geoparquet_best_practices` | The file uses a `.geoparquet` extension. | <https://github.com/opengeospatial/geoparquet/blob/main/format-specs/geoparquet.md#file-extension> |
| `bp.spatial_order` † | Warning | `geoparquet_best_practices` | The file does not look spatially ordered. | <https://github.com/opengeospatial/geoparquet/blob/main/format-specs/distributing-geoparquet.md#spatial-ordering> |
| `stac.column_missing` | Error | `stac_geoparquet` | A required column is missing. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#guidelines> |
| `stac.column_type` | Error | `stac_geoparquet` | A column has the wrong type. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#guidelines> (also <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#link-struct>, <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#asset-struct>) |
| `stac.geoparquet_metadata_missing` | Error | `stac_geoparquet` | The `geo` metadata key is missing; stac-geoparquet files must be valid GeoParquet with proper metadata, even when they use native geospatial types. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#guidelines> |
| `stac.crs` | Error | `stac_geoparquet` | The geometry CRS is not OGC:CRS84. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#guidelines> |
| `stac.metadata_missing` | Error | `stac_geoparquet` | The `stac-geoparquet` metadata key is missing. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#parquet-metadata> |
| `stac.metadata_schema` | Error | `stac_geoparquet` | The `stac-geoparquet` metadata is not JSON or fails its schema. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#parquet-metadata> |
| `stac.version` | Error | `stac_geoparquet` | The `stac-geoparquet` version is missing or unsupported. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#parquet-metadata> |
| `stac.collection_id_mismatch` | Error | `stac_geoparquet` | A `collections` key differs from its collection's `id`. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#stac-collection-objects> |
| `stac.deprecated_collection` | Warning | `stac_geoparquet` | The deprecated `collection` key is used. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#stac-collection-objects> |
| `stac.type_column` | Warning | `stac_geoparquet` | A `type` column is present. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#guidelines> |
| `stac.duplicate_id` † | Error | `stac_geoparquet` | Duplicate `id`s within a collection in the sample. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/stac-geoparquet-spec.md#guidelines> |
| `stacbp.spatiotemporal_order` † | Warning | `stac_geoparquet_best_practices` | The file is not spatio-temporally ordered. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/docs/best-practices.md#use-spatio-temporal-ordering> |
| `stacbp.id_prefix` † | Warning | `stac_geoparquet_best_practices` | Ids are not prefixed with an ordering key. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/docs/best-practices.md#prefix-item-ids-with-the-spatio-temporal-ordering-key> |
| `stacbp.hive_partitioning` | Info | `stac_geoparquet_best_practices` | Hive partitioning detected in the path. | <https://github.com/radiantearth/stac-geoparquet-spec/blob/main/docs/best-practices.md#partitioning> |
| `sample.skipped` | Info | (section of the skipped check) | A sampled check was skipped because the sample could not be read: the time budget ran out, even the first row group's sampled columns exceed the 128 MiB read budget, or the rows could not be read. Its reference is that of the skipped check. | (same as the skipped check) |
