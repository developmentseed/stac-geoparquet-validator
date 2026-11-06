//! HTTP handlers for the Lambda API.

use std::time::Duration;

use aws_sdk_s3::presigning::PresigningConfig;
use lambda_http::{Body, Error, Request, Response, http::Method};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::catalog::Catalog;
use crate::{Options, Report, catalog, validate_href};

const MAX_UPLOAD_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const UPLOAD_EXPIRY: Duration = Duration::from_secs(15 * 60);

/// Shared state for the request handlers.
pub struct State {
    /// Validation options, including the uploads bucket.
    pub options: Options,
    /// The S3 client used to presign uploads, if configured.
    pub s3: Option<aws_sdk_s3::Client>,
}

/// The OpenAPI description of the HTTP API.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "stac-geoparquet validator",
        description = "Validates Parquet files against the GeoParquet and stac-geoparquet \
                       specifications and their best practices. Findings carry stable check \
                       ids; match on `id`, never on `message`."
    ),
    servers((url = "https://validate.stac-geoparquet.org", description = "Production")),
    security(()),
    paths(checks, validate, uploads, openapi)
)]
pub struct ApiDoc;

/// A file to validate.
#[derive(Deserialize, ToSchema)]
struct ValidateRequest {
    /// The file to validate: an `http(s)://` URL, or an `s3://` href from `/api/uploads`.
    #[schema(example = "https://example.com/items.parquet")]
    href: String,
}

/// A file to upload.
#[derive(Deserialize, ToSchema)]
struct UploadRequest {
    /// The file's name. Characters other than ASCII letters, digits, `.`, `_` and `-` are
    /// replaced.
    #[schema(example = "items.parquet")]
    filename: String,
    /// The file's size in bytes, at most 2 GB.
    #[schema(example = 123456)]
    size: u64,
}

/// Where to upload a file and how to validate it afterwards.
#[derive(Serialize, ToSchema)]
struct UploadResponse {
    /// A presigned URL to `PUT` the file to. It expires after 15 minutes.
    upload_url: String,
    /// The href to pass to `/api/validate` once the upload finishes.
    href: String,
}

/// An error.
#[derive(Serialize, ToSchema)]
struct ErrorResponse {
    /// What went wrong.
    error: String,
}

/// Routes a request to its handler and adds CORS headers to the response.
pub async fn handle(req: Request, state: &State) -> Result<Response<Body>, Error> {
    let path = req.uri().path().to_string();
    let response = match (req.method(), path.as_str()) {
        (&Method::OPTIONS, _) => json_response(204, None),
        (&Method::GET, "/api/checks") => checks(&req),
        (&Method::GET, "/api/openapi.json") => openapi(),
        (&Method::POST, "/api/validate") => validate(req, state).await,
        (&Method::POST, "/api/uploads") => uploads(req, state).await,
        _ => error(404, "not found"),
    };
    Ok(with_cors(response))
}

#[derive(Clone, Copy)]
enum Format {
    Json,
    Markdown,
    Html,
}

impl Format {
    const PREFERENCE: [Format; 3] = [Format::Json, Format::Markdown, Format::Html];

    fn media_types(self) -> &'static [&'static str] {
        match self {
            Format::Json => &["application/json"],
            Format::Markdown => &["text/markdown", "text/plain"],
            Format::Html => &["text/html", "application/xhtml+xml"],
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Format::Json => "application/json",
            Format::Markdown => "text/markdown; charset=utf-8",
            Format::Html => "text/html; charset=utf-8",
        }
    }
}

fn parse_accept(accept: &str) -> Vec<(String, f32)> {
    accept
        .split(',')
        .filter_map(|range| {
            let mut parts = range.split(';');
            let media = parts.next()?.trim().to_ascii_lowercase();
            if media.is_empty() {
                return None;
            }
            let quality = parts
                .filter_map(|p| p.trim().strip_prefix("q="))
                .find_map(|q| q.trim().parse::<f32>().ok())
                .unwrap_or(1.0);
            Some((media, quality))
        })
        .collect()
}

fn quality(ranges: &[(String, f32)], media: &str) -> Option<f32> {
    let (kind, _) = media.split_once('/')?;
    let specificity = |range: &str| {
        if range == media {
            Some(2)
        } else if range.strip_suffix("/*") == Some(kind) {
            Some(1)
        } else if range == "*/*" {
            Some(0)
        } else {
            None
        }
    };
    ranges
        .iter()
        .filter_map(|(range, q)| specificity(range).map(|s| (s, *q)))
        .max_by_key(|(s, _)| *s)
        .map(|(_, q)| q)
}

fn negotiate(accept: Option<&str>) -> Option<Format> {
    let ranges = match accept.map(parse_accept) {
        Some(ranges) if !ranges.is_empty() => ranges,
        _ => return Some(Format::Json),
    };
    let mut best: Option<(Format, f32)> = None;
    for format in Format::PREFERENCE {
        let q = format
            .media_types()
            .iter()
            .filter_map(|media| quality(&ranges, media))
            .fold(0.0, f32::max);
        if q > 0.0 && best.is_none_or(|(_, b)| q > b) {
            best = Some((format, q));
        }
    }
    best.map(|(format, _)| format)
}

/// Lists every check the validator can emit.
///
/// The format follows the `Accept` header: JSON by default, Markdown for `text/markdown`,
/// `text/plain` or `text/*`, and HTML for `text/html`.
#[utoipa::path(
    get,
    path = "/api/checks",
    tag = "checks",
    params(
        ("accept" = Option<String>, Header, description = "The preferred format, e.g. `text/markdown`.")
    ),
    responses(
        (status = 200, description = "The checks catalog.", content(
            (Catalog = "application/json"),
            (String = "text/markdown"),
            (String = "text/html"),
        )),
        (status = 406, description = "None of the formats is acceptable.", body = ErrorResponse),
    )
)]
fn checks(req: &Request) -> Response<Body> {
    let accept = req
        .headers()
        .get("accept")
        .and_then(|value| value.to_str().ok());
    let Some(format) = negotiate(accept) else {
        return error(
            406,
            "acceptable types are application/json, text/markdown, text/plain and text/html",
        );
    };
    let catalog = catalog::catalog();
    let body = match format {
        Format::Json => match serde_json::to_string(catalog) {
            Ok(text) => text,
            Err(_) => return error(500, "failed to serialize checks"),
        },
        Format::Markdown => catalog.to_markdown(),
        Format::Html => catalog.to_html(),
    };
    Response::builder()
        .status(200)
        .header("content-type", format.content_type())
        .header("vary", "accept")
        .body(Body::Text(body))
        .expect("static response parts are valid")
}

/// Validates a file and returns a report.
#[utoipa::path(
    post,
    path = "/api/validate",
    tag = "validate",
    request_body = ValidateRequest,
    responses(
        (status = 200, description = "The validation report. An unreadable file still returns 200, with a `parquet.unreadable` finding.", body = Report),
        (status = 400, description = "The body is not a valid request.", body = ErrorResponse),
    )
)]
async fn validate(req: Request, state: &State) -> Response<Body> {
    let Ok(body) = serde_json::from_slice::<ValidateRequest>(req.body().as_ref()) else {
        return error(400, "body must be JSON like {\"href\": string}");
    };
    let report = validate_href(&body.href, &state.options).await;
    match serde_json::to_string(&report) {
        Ok(text) => json_response(200, Some(text)),
        Err(_) => error(500, "failed to serialize report"),
    }
}

/// Starts an upload.
///
/// `PUT` the file to `upload_url`, then pass `href` to `/api/validate`. Uploads are deleted
/// after one day.
#[utoipa::path(
    post,
    path = "/api/uploads",
    tag = "validate",
    request_body = UploadRequest,
    responses(
        (status = 200, description = "A presigned upload.", body = UploadResponse),
        (status = 400, description = "The body is not a valid request, or the size or filename is invalid.", body = ErrorResponse),
        (status = 503, description = "Uploads are not enabled.", body = ErrorResponse),
    )
)]
async fn uploads(req: Request, state: &State) -> Response<Body> {
    let (Some(s3), Some(bucket)) = (&state.s3, &state.options.uploads_bucket) else {
        return error(503, "uploads are not enabled");
    };
    let Ok(body) = serde_json::from_slice::<UploadRequest>(req.body().as_ref()) else {
        return error(
            400,
            "body must be JSON like {\"filename\": string, \"size\": number}",
        );
    };
    if body.size == 0 || body.size > MAX_UPLOAD_SIZE {
        return error(400, "size must be between 1 byte and 2 GB");
    }
    let filename = sanitize(&body.filename);
    if filename.is_empty() {
        return error(400, "filename is empty after sanitizing");
    }
    let key = format!("uploads/{}/{}", uuid::Uuid::new_v4(), filename);
    let Ok(presigning) = PresigningConfig::expires_in(UPLOAD_EXPIRY) else {
        return error(500, "failed to configure presigning");
    };
    let Ok(presigned) = s3
        .put_object()
        .bucket(bucket)
        .key(&key)
        .content_length(body.size as i64)
        .presigned(presigning)
        .await
    else {
        return error(500, "failed to presign upload");
    };
    json_body(
        200,
        &UploadResponse {
            upload_url: presigned.uri().to_string(),
            href: format!("s3://{bucket}/{key}"),
        },
    )
}

fn sanitize(filename: &str) -> String {
    let replaced: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    replaced.trim_start_matches(['.', '_']).to_string()
}

fn error(status: u16, message: &str) -> Response<Body> {
    json_body(
        status,
        &ErrorResponse {
            error: message.to_string(),
        },
    )
}

/// Returns this OpenAPI description.
#[utoipa::path(
    get,
    path = "/api/openapi.json",
    tag = "meta",
    responses((status = 200, description = "The OpenAPI description.", content_type = "application/json"))
)]
fn openapi() -> Response<Body> {
    json_response(200, Some(openapi_json()))
}

/// Returns the OpenAPI description as pretty-printed JSON with a trailing newline.
pub fn openapi_json() -> String {
    let mut text = ApiDoc::openapi()
        .to_pretty_json()
        .expect("the OpenAPI description serializes");
    text.push('\n');
    text
}

fn json_body(status: u16, value: &impl Serialize) -> Response<Body> {
    match serde_json::to_string(value) {
        Ok(text) => json_response(status, Some(text)),
        Err(_) => json_response(
            500,
            Some(r#"{"error":"failed to serialize response"}"#.to_string()),
        ),
    }
}

fn json_response(status: u16, body: Option<String>) -> Response<Body> {
    let mut builder = Response::builder().status(status);
    let body = match body {
        Some(text) => {
            builder = builder.header("content-type", "application/json");
            Body::Text(text)
        }
        None => Body::Empty,
    };
    builder.body(body).expect("static response parts are valid")
}

fn with_cors(mut response: Response<Body>) -> Response<Body> {
    let headers = response.headers_mut();
    headers.insert("access-control-allow-origin", "*".parse().unwrap());
    headers.insert(
        "access-control-allow-methods",
        "GET, POST, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        "access-control-allow-headers",
        "content-type".parse().unwrap(),
    );
    response
}
