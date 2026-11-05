use crate::{Context, Finding, Section, Severity};

const REFERENCE: &str = "https://parquet.apache.org/docs/file-format/";

pub(super) fn info(context: &Context) -> Vec<Finding> {
    let file = context.metadata.file_metadata();
    vec![Finding::new(
        "parquet.info",
        Severity::Info,
        Section::Parquet,
        format!(
            "Parquet format version {}, created by {}, {} rows in {} row groups, {} bytes",
            file.version(),
            file.created_by().unwrap_or("unknown"),
            file.num_rows(),
            context.metadata.num_row_groups(),
            context.size,
        ),
        REFERENCE,
    )]
}
