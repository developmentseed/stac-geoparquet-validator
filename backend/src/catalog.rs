//! The public catalog of check ids, loaded from `checks.json`.

use std::sync::LazyLock;

use askama::Template;
use num_format::{Locale, ToFormattedString};
use serde::{Deserialize, Serialize};

use crate::sample::{MAX_BYTES, MAX_ROWS};
use crate::{Section, Severity};

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../checks.json")).expect("checks.json is valid")
});

/// Returns the catalog of every check the validator can emit.
pub fn catalog() -> &'static Catalog {
    &CATALOG
}

/// Every check the validator can emit, the documents they come from, and the ids that
/// have been retired.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    /// The report sections and the upstream document each one encodes.
    pub sections: Vec<SectionInfo>,
    /// The checks in the order they are documented.
    pub checks: Vec<CheckInfo>,
    /// Ids that are no longer emitted and must never be reused.
    pub retired: Vec<RetiredCheck>,
}

/// A report section and its upstream document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionInfo {
    /// The section.
    pub section: Section,
    /// A human-readable title.
    pub title: String,
    /// The upstream document the section's checks come from.
    pub source: Source,
}

/// An upstream specification or best-practice document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// The document's title.
    pub title: String,
    /// A link to the document.
    pub href: String,
}

/// One check id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckInfo {
    /// The stable check id.
    pub id: String,
    /// The severity of the check's findings.
    pub severity: Severity,
    /// The section of the check's findings, or `None` when it takes the section of
    /// another check.
    pub section: Option<Section>,
    /// Whether the check reads a bounded sample of rows or row-group statistics.
    pub sampled: bool,
    /// What the check flags. May contain `code` spans.
    pub description: String,
    /// Links to the upstream sections the check implements.
    pub references: Vec<String>,
}

/// A check id that is no longer emitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetiredCheck {
    /// The retired id.
    pub id: String,
    /// The date the id was retired, as `YYYY-MM-DD`.
    pub retired: String,
    /// Why the id was retired.
    pub reason: String,
}

const TITLE: &str = "stac-geoparquet validator checks";

fn intro() -> [String; 3] {
    [
        "Every finding in a report carries a stable `id`. These ids are a public contract: \
         they will not be renamed, and new checks get new ids. Messages are free text and may \
         change."
            .to_string(),
        format!(
            "Checks marked with † read a bounded sample of rows (or per-row-group statistics) in \
             addition to the footer. The sample takes row groups in order, up to {} rows and {} \
             MiB of compressed column chunks. When the sample cannot be read, a `sample.skipped` \
             finding is emitted in place of the sampled checks.",
            MAX_ROWS.to_formatted_string(&Locale::en),
            MAX_BYTES / (1024 * 1024)
        ),
        "The severities are `error` (invalid against a specification), `warning` (deviates from \
         a best practice), `compat` (valid, but some readers are known to fail) and `info` (a \
         reported fact)."
            .to_string(),
    ]
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Compat => "compat",
        Severity::Info => "info",
    }
}

fn section_name(section: Section) -> &'static str {
    match section {
        Section::Parquet => "parquet",
        Section::Geoparquet => "geoparquet",
        Section::GeoparquetBestPractices => "geoparquet_best_practices",
        Section::StacGeoparquet => "stac_geoparquet",
        Section::StacGeoparquetBestPractices => "stac_geoparquet_best_practices",
    }
}

struct Span {
    code: bool,
    text: String,
}

struct Text<'a> {
    raw: &'a str,
    spans: Vec<Span>,
}

impl<'a> Text<'a> {
    fn new(raw: &'a str) -> Text<'a> {
        let spans = raw
            .split('`')
            .enumerate()
            .filter(|(_, text)| !text.is_empty())
            .map(|(i, text)| Span {
                code: i % 2 == 1,
                text: text.to_string(),
            })
            .collect();
        Text { raw, spans }
    }
}

struct SourceView<'a> {
    section: &'static str,
    title: &'a str,
    href: &'a str,
}

struct CheckView<'a> {
    id: &'a str,
    severity: &'static str,
    section: Option<&'static str>,
    sampled: bool,
    description: Text<'a>,
    references: &'a [String],
}

struct RetiredView<'a> {
    id: &'a str,
    retired: &'a str,
    reason: Text<'a>,
}

struct Page<'a> {
    title: &'static str,
    intro: Vec<Text<'a>>,
    sources: Vec<SourceView<'a>>,
    checks: Vec<CheckView<'a>>,
    retired: Vec<RetiredView<'a>>,
}

#[derive(Template)]
#[template(path = "checks.html")]
struct HtmlPage<'a> {
    page: Page<'a>,
}

#[derive(Template)]
#[template(path = "checks.md")]
struct MarkdownPage<'a> {
    page: Page<'a>,
}

impl Catalog {
    fn page<'a>(&'a self, intro: &'a [String]) -> Page<'a> {
        Page {
            title: TITLE,
            intro: intro.iter().map(|p| Text::new(p)).collect(),
            sources: self
                .sections
                .iter()
                .map(|info| SourceView {
                    section: section_name(info.section),
                    title: &info.source.title,
                    href: &info.source.href,
                })
                .collect(),
            checks: self
                .checks
                .iter()
                .map(|check| CheckView {
                    id: &check.id,
                    severity: severity_name(check.severity),
                    section: check.section.map(section_name),
                    sampled: check.sampled,
                    description: Text::new(&check.description),
                    references: &check.references,
                })
                .collect(),
            retired: self
                .retired
                .iter()
                .map(|retired| RetiredView {
                    id: &retired.id,
                    retired: &retired.retired,
                    reason: Text::new(&retired.reason),
                })
                .collect(),
        }
    }

    /// Renders the catalog as a Markdown document.
    pub fn to_markdown(&self) -> String {
        let intro = intro();
        let page = MarkdownPage {
            page: self.page(&intro),
        };
        page.render().expect("the checks Markdown template renders")
    }

    /// Renders the catalog as a standalone HTML page.
    pub fn to_html(&self) -> String {
        let intro = intro();
        let page = HtmlPage {
            page: self.page(&intro),
        };
        page.render().expect("the checks HTML template renders")
    }
}
