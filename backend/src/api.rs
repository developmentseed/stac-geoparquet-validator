//! HTTP handlers for the Lambda API.

use std::time::Duration;

use aws_sdk_s3::presigning::PresigningConfig;
use lambda_http::{Body, Error, Request, Response, http::Method};
use serde::Deserialize;
use serde_json::json;

use crate::{Options, validate_href};

const MAX_UPLOAD_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const UPLOAD_EXPIRY: Duration = Duration::from_secs(15 * 60);

/// Shared state for the request handlers.
pub struct State {
    /// Validation options, including the uploads bucket.
    pub options: Options,
    /// The S3 client used to presign uploads, if configured.
    pub s3: Option<aws_sdk_s3::Client>,
}

#[derive(Deserialize)]
struct ValidateRequest {
    href: String,
}

#[derive(Deserialize)]
struct UploadRequest {
    filename: String,
    size: u64,
}

/// Routes a request to its handler and adds CORS headers to the response.
pub async fn handle(req: Request, state: &State) -> Result<Response<Body>, Error> {
    let path = req.uri().path().to_string();
    let response = match (req.method(), path.as_str()) {
        (&Method::OPTIONS, _) => json_response(204, None),
        (&Method::POST, "/api/validate") => validate(req, state).await,
        (&Method::POST, "/api/uploads") => uploads(req, state).await,
        _ => error(404, "not found"),
    };
    Ok(with_cors(response))
}

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
    let value = json!({
        "upload_url": presigned.uri(),
        "href": format!("s3://{bucket}/{key}"),
    });
    json_response(200, Some(value.to_string()))
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
    json_response(status, Some(json!({"error": message}).to_string()))
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
        "POST, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        "access-control-allow-headers",
        "content-type".parse().unwrap(),
    );
    response
}
