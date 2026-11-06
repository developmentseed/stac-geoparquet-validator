use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use http_body_util::BodyExt;
use lambda_http::{Body, Request, http};
use serde_json::{Value, json};
use stac_geoparquet_validator::Options;
use stac_geoparquet_validator::api::{State, handle};

const BUCKET: &str = "my-uploads";

fn state(with_s3: bool) -> State {
    let s3 = with_s3.then(|| {
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-west-2"))
            .credentials_provider(Credentials::new("AKID", "SECRET", None, None, "test"))
            .build();
        aws_sdk_s3::Client::from_conf(config)
    });
    State {
        options: Options {
            uploads_bucket: with_s3.then(|| BUCKET.to_string()),
            ..Options::default()
        },
        s3,
    }
}

fn request(method: &str, path: &str, body: &str) -> Request {
    http::Request::builder()
        .method(method)
        .uri(path)
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn call(state: &State, req: Request) -> (u16, http::HeaderMap, Value) {
    let response = handle(req, state).await.unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, value)
}

#[tokio::test]
async fn validate_bad_body_400() {
    let (status, _, body) = call(&state(false), request("POST", "/api/validate", "nope")).await;
    assert_eq!(status, 400);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn validate_returns_report() {
    let req = request(
        "POST",
        "/api/validate",
        &json!({"href": "file:///x"}).to_string(),
    );
    let (status, headers, body) = call(&state(false), req).await;
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(body["summary"]["parquet_valid"], false);
}

#[tokio::test]
async fn uploads_too_big_400() {
    let size = 2u64 * 1024 * 1024 * 1024 + 1;
    let req = request(
        "POST",
        "/api/uploads",
        &json!({"filename": "a.parquet", "size": size}).to_string(),
    );
    let (status, _, _) = call(&state(true), req).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn uploads_zero_size_400() {
    let req = request(
        "POST",
        "/api/uploads",
        &json!({"filename": "a.parquet", "size": 0}).to_string(),
    );
    let (status, _, _) = call(&state(true), req).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn uploads_empty_filename_400() {
    let req = request(
        "POST",
        "/api/uploads",
        &json!({"filename": "..", "size": 10}).to_string(),
    );
    let (status, _, _) = call(&state(true), req).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn uploads_without_bucket_503() {
    let req = request(
        "POST",
        "/api/uploads",
        &json!({"filename": "a.parquet", "size": 10}).to_string(),
    );
    let (status, _, _) = call(&state(false), req).await;
    assert_eq!(status, 503);
}

#[tokio::test]
async fn uploads_returns_presigned_put() {
    let req = request(
        "POST",
        "/api/uploads",
        &json!({"filename": "a.parquet", "size": 1234}).to_string(),
    );
    let (status, _, body) = call(&state(true), req).await;
    assert_eq!(status, 200);
    assert!(body["upload_url"].as_str().unwrap().contains(BUCKET));
    assert!(
        body["href"]
            .as_str()
            .unwrap()
            .starts_with(&format!("s3://{BUCKET}/uploads/"))
    );
}

#[tokio::test]
async fn filename_sanitized() {
    let req = request(
        "POST",
        "/api/uploads",
        &json!({"filename": "../a b.parquet", "size": 10}).to_string(),
    );
    let (status, _, body) = call(&state(true), req).await;
    assert_eq!(status, 200);
    let href = body["href"].as_str().unwrap();
    assert!(href.ends_with("/a_b.parquet"), "{href}");
    assert!(!href.contains("/.."));
}

#[tokio::test]
async fn cors_headers_present() {
    let (_, headers, _) = call(&state(false), request("POST", "/api/validate", "nope")).await;
    assert_eq!(headers["access-control-allow-origin"], "*");
    assert_eq!(
        headers["access-control-allow-methods"],
        "GET, POST, OPTIONS"
    );
    assert_eq!(headers["access-control-allow-headers"], "content-type");
}

#[tokio::test]
async fn options_204() {
    let (status, headers, _) = call(&state(false), request("OPTIONS", "/api/validate", "")).await;
    assert_eq!(status, 204);
    assert_eq!(headers["access-control-allow-origin"], "*");
}

#[tokio::test]
async fn unknown_404() {
    let (status, headers, _) = call(&state(false), request("GET", "/api/nope", "")).await;
    assert_eq!(status, 404);
    assert_eq!(headers["access-control-allow-origin"], "*");
}

async fn get_checks(accept: Option<&str>) -> (u16, http::HeaderMap, String) {
    let mut builder = http::Request::builder().method("GET").uri("/api/checks");
    if let Some(accept) = accept {
        builder = builder.header("accept", accept);
    }
    let req = builder.body(Body::Empty).unwrap();
    let response = handle(req, &state(false)).await.unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

fn content_type(headers: &http::HeaderMap) -> &str {
    headers["content-type"].to_str().unwrap()
}

#[tokio::test]
async fn checks_default_to_json() {
    let (status, headers, body) = get_checks(None).await;
    assert_eq!(status, 200);
    assert_eq!(content_type(&headers), "application/json");
    assert_eq!(headers["vary"], "accept");
    let value: Value = serde_json::from_str(&body).unwrap();
    assert!(!value["checks"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn checks_as_json() {
    let (status, headers, body) = get_checks(Some("application/json")).await;
    assert_eq!(status, 200);
    assert_eq!(content_type(&headers), "application/json");
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["checks"][0]["id"], "parquet.unreadable");
}

#[tokio::test]
async fn checks_as_markdown_for_text() {
    for accept in ["text/plain", "text/markdown", "text/*"] {
        let (status, headers, body) = get_checks(Some(accept)).await;
        assert_eq!(status, 200);
        assert!(content_type(&headers).starts_with("text/markdown"));
        assert!(body.contains("| `parquet.unreadable` |"));
    }
}

#[tokio::test]
async fn checks_as_html_for_browsers() {
    let accept = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
    let (status, headers, body) = get_checks(Some(accept)).await;
    assert_eq!(status, 200);
    assert!(content_type(&headers).starts_with("text/html"));
    assert!(body.starts_with("<!doctype html>"));
    assert!(body.contains("<code>parquet.unreadable</code>"));
}

#[tokio::test]
async fn checks_respect_quality() {
    let accept = "text/html;q=0.5, text/markdown";
    let (_, headers, _) = get_checks(Some(accept)).await;
    assert!(content_type(&headers).starts_with("text/markdown"));
    let (_, headers, _) = get_checks(Some("application/json;q=0, */*")).await;
    assert!(content_type(&headers).starts_with("text/markdown"));
}

#[tokio::test]
async fn checks_wildcard_is_json() {
    let (_, headers, _) = get_checks(Some("*/*")).await;
    assert_eq!(content_type(&headers), "application/json");
}

#[tokio::test]
async fn checks_unacceptable_406() {
    let (status, headers, _) = get_checks(Some("image/png")).await;
    assert_eq!(status, 406);
    assert_eq!(headers["access-control-allow-origin"], "*");
}

#[tokio::test]
async fn openapi_describes_every_route() {
    let (status, headers, body) =
        call(&state(false), request("GET", "/api/openapi.json", "")).await;
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "application/json");
    assert!(body["openapi"].as_str().unwrap().starts_with("3."));
    let paths = &body["paths"];
    assert!(paths["/api/checks"]["get"].is_object());
    assert!(paths["/api/validate"]["post"].is_object());
    assert!(paths["/api/uploads"]["post"].is_object());
    assert!(paths["/api/openapi.json"]["get"].is_object());
}

#[tokio::test]
async fn openapi_documents_checks_formats() {
    let (_, _, body) = call(&state(false), request("GET", "/api/openapi.json", "")).await;
    let content = &body["paths"]["/api/checks"]["get"]["responses"]["200"]["content"];
    for media in ["application/json", "text/markdown", "text/html"] {
        assert!(content[media].is_object(), "{media}");
    }
}

#[tokio::test]
async fn openapi_includes_report_and_catalog_schemas() {
    let (_, _, body) = call(&state(false), request("GET", "/api/openapi.json", "")).await;
    let schemas = &body["components"]["schemas"];
    for name in [
        "Report",
        "Finding",
        "Severity",
        "Section",
        "Catalog",
        "CheckInfo",
    ] {
        assert!(schemas[name].is_object(), "{name}");
    }
}

#[test]
fn committed_openapi_json_is_up_to_date() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("openapi.json");
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        committed == stac_geoparquet_validator::api::openapi_json(),
        "backend/openapi.json is stale; run `cargo run --bin openapi > openapi.json` in backend/"
    );
}
