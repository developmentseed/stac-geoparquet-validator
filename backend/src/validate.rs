//! The validation entry points.

use crate::{Context, Finding, Report, Section, Severity, checks, resolve};
use object_store::{ObjectStore, path::Path};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const DEFAULT_BUDGET: Duration = Duration::from_secs(25);

/// Validation settings.
#[derive(Debug, Clone)]
pub struct Options {
    /// The bucket that holds user uploads, read with the process's credentials.
    ///
    /// Every other `s3://` bucket is read unsigned.
    pub uploads_bucket: Option<String>,
    /// How long a validation may take.
    pub budget: Duration,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            uploads_bucket: None,
            budget: DEFAULT_BUDGET,
        }
    }
}

/// Validates the file at `path` in `store`, reporting it as `href`.
pub async fn validate(
    store: Arc<dyn ObjectStore>,
    path: Path,
    href: String,
    options: &Options,
) -> Report {
    validate_until(store, path, href, Instant::now() + options.budget).await
}

/// Resolves `href` and validates the file it points to.
///
/// A resolution failure becomes a report with a single `parquet.unreadable` finding.
pub async fn validate_href(href: &str, options: &Options) -> Report {
    let deadline = Instant::now() + options.budget;
    let resolved = tokio::time::timeout_at(deadline.into(), resolve::resolve(href, options))
        .await
        .unwrap_or_else(|_| Err(timed_out()));
    match resolved {
        Ok((store, path)) => validate_until(store, path, href.to_string(), deadline).await,
        Err(finding) => Report::new(href.to_string(), vec![finding], None, false),
    }
}

async fn validate_until(
    store: Arc<dyn ObjectStore>,
    path: Path,
    href: String,
    deadline: Instant,
) -> Report {
    let load = Context::load(store.clone(), path.clone(), href.clone());
    let context = match tokio::time::timeout_at(deadline.into(), load)
        .await
        .unwrap_or_else(|_| Err(timed_out()))
    {
        Ok(context) => context,
        Err(finding) => return Report::new(href, vec![finding], None, false),
    };
    let mut findings = checks::run_footer_checks(&context);
    findings.extend(checks::data_checks(store, &path, &context, deadline).await);
    Report::new(
        href,
        findings,
        context.geoparquet_version(),
        !context.native_geo_columns().is_empty(),
    )
}

fn timed_out() -> Finding {
    Finding::new(
        "parquet.unreadable",
        Severity::Error,
        Section::Parquet,
        "timed out reading the file",
        "https://parquet.apache.org/docs/file-format/",
    )
}
