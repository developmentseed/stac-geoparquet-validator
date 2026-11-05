use stac_geoparquet_validator::{
    Options, Severity,
    resolve::{self, Href},
    validate_href,
};

async fn rejected(href: &str) {
    let err = resolve::resolve(href, &Options::default())
        .await
        .err()
        .unwrap();
    assert_eq!(err.id, "parquet.unreadable");
    assert_eq!(err.severity, Severity::Error);
}

#[tokio::test]
async fn rejects_loopback() {
    rejected("http://127.0.0.1:9001/x.parquet").await;
}

#[tokio::test]
async fn rejects_metadata_ip() {
    rejected("http://169.254.169.254/").await;
}

#[tokio::test]
async fn rejects_private() {
    rejected("http://10.0.0.1/").await;
}

#[tokio::test]
async fn rejects_localhost_name() {
    rejected("http://localhost/").await;
}

#[tokio::test]
async fn rejects_file_scheme() {
    rejected("file:///etc/passwd").await;
}

#[tokio::test]
async fn rejects_ipv6_loopback() {
    rejected("http://[::1]/").await;
}

#[tokio::test]
async fn rejects_ipv4_mapped_ipv6_metadata_ip() {
    rejected("http://[::ffff:169.254.169.254]/").await;
}

#[tokio::test]
async fn rejects_cgnat() {
    rejected("http://100.64.0.1/").await;
}

#[tokio::test]
async fn rejects_ipv6_unique_local() {
    rejected("http://[fd00:ec2::254]/").await;
}

#[tokio::test]
async fn rejects_unspecified() {
    rejected("http://0.0.0.0/").await;
}

#[tokio::test]
async fn rejects_not_a_url() {
    rejected("not a url").await;
}

#[test]
fn accepts_s3_scheme_shape() {
    let href = resolve::parse_href("s3://my-bucket/path/to/items.parquet").unwrap();
    match href {
        Href::S3 { bucket, key } => {
            assert_eq!(bucket, "my-bucket");
            assert_eq!(key.as_ref(), "path/to/items.parquet");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn s3_without_key_is_rejected() {
    let err = resolve::parse_href("s3://my-bucket").unwrap_err();
    assert_eq!(err.id, "parquet.unreadable");
}

#[test]
fn s3_invalid_bucket_is_rejected() {
    for href in [
        "s3://Bad_Bucket/key",
        "s3://user@bucket/key",
        "s3://bucket:9001/key",
        "s3://ab/key",
    ] {
        let err = resolve::parse_href(href).unwrap_err();
        assert_eq!(err.id, "parquet.unreadable", "{href}");
    }
}

#[test]
fn https_shape() {
    let href = resolve::parse_href("https://example.com/a/year=2016/items.parquet?x=1").unwrap();
    assert!(matches!(href, Href::Http { .. }));
}

#[tokio::test]
async fn validate_href_reports_resolve_failure() {
    let report = validate_href("http://127.0.0.1:9001/x.parquet", &Options::default()).await;
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].id, "parquet.unreadable");
    assert!(!report.summary.parquet_valid);
}

#[tokio::test]
async fn rejects_decimal_metadata_ip() {
    rejected("http://2852039166/").await;
}
