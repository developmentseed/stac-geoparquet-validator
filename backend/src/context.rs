//! File-level context shared by checks.

use crate::{Finding, Section, Severity, geo::GeoState};
use arrow_schema::SchemaRef;
use bytes::Bytes;
use object_store::{GetOptions, GetRange, ObjectStore, ObjectStoreExt, path::Path};
#[allow(deprecated)]
use parquet::arrow::async_reader::ParquetObjectReader;
use parquet::{
    arrow::parquet_to_arrow_schema,
    file::metadata::{ParquetMetaData, ParquetMetaDataReader},
};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

const MAX_FOOTER_LEN: u64 = 64 * 1024 * 1024;
const REFERENCE: &str = "https://parquet.apache.org/docs/file-format/";
const PARQUET_EXTENSIONS: [&str; 4] = ["parquet", "geoparquet", "pq", "parq"];

/// Everything known about a file after reading its footer.
#[derive(Debug)]
pub struct Context {
    /// The user-facing location of the file.
    pub href: String,
    /// The path of the file within its object store.
    pub path: Path,
    /// The size of the file in bytes.
    pub size: u64,
    /// The parsed Parquet footer.
    pub metadata: Arc<ParquetMetaData>,
    /// The Arrow schema derived from the footer.
    pub arrow_schema: SchemaRef,
    /// The file's key-value metadata.
    pub key_value: HashMap<String, String>,
    pub(crate) geo: OnceLock<GeoState>,
}

fn unreadable(message: impl Into<String>) -> Finding {
    Finding::new(
        "parquet.unreadable",
        Severity::Error,
        Section::Parquet,
        message,
        REFERENCE,
    )
}

fn too_small(size: u64) -> Result<(), Finding> {
    if size < 12 {
        Err(unreadable(format!(
            "file is {size} bytes, too small to be Parquet"
        )))
    } else {
        Ok(())
    }
}

fn missing_magic(path: &Path) -> Finding {
    let has_parquet_extension = path
        .extension()
        .is_some_and(|extension| PARQUET_EXTENSIONS.contains(&extension.to_lowercase().as_str()));
    if has_parquet_extension {
        unreadable(
            "file has a Parquet extension but does not end with the PAR1 magic bytes, so it may be truncated or corrupt",
        )
    } else {
        unreadable("not a Parquet file")
    }
}

fn read_error(err: object_store::Error) -> Finding {
    unreadable(format!("could not read file: {err}"))
}

/// Returns the file size and its last 8 bytes.
///
/// Uses one suffix-range GET, which works for presigned GET URLs and servers that reject
/// HEAD. Servers that reject suffix ranges get a HEAD followed by a bounded range GET.
async fn read_tail(store: &dyn ObjectStore, path: &Path) -> Result<(u64, Bytes), Finding> {
    let options = GetOptions {
        range: Some(GetRange::Suffix(8)),
        ..GetOptions::default()
    };
    let (size, tail) = match store.get_opts(path, options).await {
        Ok(result) => {
            let size = result.meta.size;
            too_small(size)?;
            (size, result.bytes().await.map_err(read_error)?)
        }
        Err(err @ object_store::Error::NotFound { .. }) => return Err(read_error(err)),
        Err(_) => {
            let size = store.head(path).await.map_err(read_error)?.size;
            too_small(size)?;
            let tail = store
                .get_range(path, size - 8..size)
                .await
                .map_err(read_error)?;
            (size, tail)
        }
    };
    if tail.len() != 8 {
        return Err(unreadable(format!(
            "expected the last 8 bytes of the file, got {}",
            tail.len()
        )));
    }
    Ok((size, tail))
}

impl Context {
    /// Reads and validates the footer of the file at `path`.
    ///
    /// Returns a `parquet.unreadable` finding if the file is not a readable Parquet file.
    pub async fn load(
        store: Arc<dyn ObjectStore>,
        path: Path,
        href: String,
    ) -> Result<Context, Finding> {
        let (size, tail) = read_tail(store.as_ref(), &path).await?;
        if &tail[4..8] != b"PAR1" {
            return Err(missing_magic(&path));
        }
        let footer_len = u64::from(u32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]));
        if footer_len > MAX_FOOTER_LEN || footer_len > size - 8 {
            return Err(unreadable(format!(
                "footer length {footer_len} is invalid for a {size} byte file"
            )));
        }
        #[allow(deprecated)]
        let mut reader = ParquetObjectReader::new(store, path.clone()).with_file_size(size);
        let metadata = ParquetMetaDataReader::new()
            .with_prefetch_hint(Some(footer_len as usize + 8))
            .load_and_finish(&mut reader, size)
            .await
            .map_err(|err| unreadable(format!("could not parse Parquet footer: {err}")))?;
        let file = metadata.file_metadata();
        let key_value: HashMap<String, String> = file
            .key_value_metadata()
            .into_iter()
            .flatten()
            .filter_map(|kv| kv.value.clone().map(|value| (kv.key.clone(), value)))
            .collect();
        let arrow_schema = Arc::new(
            parquet_to_arrow_schema(file.schema_descr(), file.key_value_metadata())
                .map_err(|err| unreadable(format!("could not derive Arrow schema: {err}")))?,
        );
        Ok(Context {
            href,
            path,
            size,
            metadata: Arc::new(metadata),
            arrow_schema,
            key_value,
            geo: OnceLock::new(),
        })
    }
}
