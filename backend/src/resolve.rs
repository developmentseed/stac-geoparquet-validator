//! Turning user-supplied hrefs into object stores, with a guard against server-side request
//! forgery.
//!
//! Every `http(s)://` host is resolved and rejected if any of its addresses is not publicly
//! routable. The HTTP client re-applies the same check on every connection through its DNS
//! resolver, follows at most five redirects (only to `http(s)://` URLs whose IP-literal hosts
//! are public), and ignores proxy environment variables.

use crate::{Finding, Options, Section, Severity};
use object_store::{
    ClientOptions, ObjectStore, RetryConfig,
    aws::{AmazonS3Builder, resolve_bucket_region},
    client::{DnsError, DnsFuture, DnsResolver, HttpClient, HttpConnector},
    http::HttpBuilder,
    path::Path,
};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Arc,
    time::Duration,
};
use url::{Host, Url};

const REFERENCE: &str = "https://parquet.apache.org/docs/file-format/";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RETRIES: usize = 2;
const MAX_REDIRECTS: usize = 5;
const USER_AGENT: &str = concat!("stac-geoparquet-validator/", env!("CARGO_PKG_VERSION"));

/// A parsed, supported href.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Href {
    /// An `http://` or `https://` URL.
    Http(Url),
    /// An `s3://bucket/key` location.
    S3 {
        /// The bucket name.
        bucket: String,
        /// The object key.
        key: Path,
    },
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

/// Parses `href` and checks its shape, without touching the network.
///
/// Returns a `parquet.unreadable` finding for unsupported schemes and malformed hrefs.
pub fn parse_href(href: &str) -> Result<Href, Finding> {
    let url = Url::parse(href).map_err(|err| unreadable(format!("invalid href: {err}")))?;
    match url.scheme() {
        "http" | "https" => {
            if url.host().is_none() {
                return Err(unreadable("href has no host"));
            }
            Ok(Href::Http(url))
        }
        "s3" => {
            if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
                return Err(unreadable(
                    "`s3://` hrefs cannot have credentials or a port",
                ));
            }
            let bucket = url.host_str().unwrap_or_default();
            if !is_valid_bucket_name(bucket) {
                return Err(unreadable(format!("invalid S3 bucket name: {bucket}")));
            }
            let key = Path::from_url_path(url.path())
                .map_err(|err| unreadable(format!("invalid S3 key: {err}")))?;
            if key.as_ref().is_empty() {
                return Err(unreadable("`s3://` href has no key"));
            }
            Ok(Href::S3 {
                bucket: bucket.to_string(),
                key,
            })
        }
        scheme => Err(unreadable(format!("unsupported scheme: {scheme}"))),
    }
}

/// Resolves `href` to an object store and a path within it.
///
/// Returns a `parquet.unreadable` finding if the href is unsupported, points at a
/// non-public address, or its S3 bucket cannot be found.
pub async fn resolve(
    href: &str,
    options: &Options,
) -> Result<(Arc<dyn ObjectStore>, Path), Finding> {
    match parse_href(href)? {
        Href::Http(url) => http(url, options).await,
        Href::S3 { bucket, key } => s3(bucket, key, options).await,
    }
}

async fn http(url: Url, options: &Options) -> Result<(Arc<dyn ObjectStore>, Path), Finding> {
    match url.host() {
        Some(Host::Ipv4(ip)) => check_ip(IpAddr::V4(ip))?,
        Some(Host::Ipv6(ip)) => check_ip(IpAddr::V6(ip))?,
        Some(Host::Domain(domain)) => {
            lookup(domain)
                .await
                .map_err(|err| unreadable(err.to_string()))?;
        }
        None => return Err(unreadable("href has no host")),
    }
    http_store(url, options)
}

fn http_store(url: Url, options: &Options) -> Result<(Arc<dyn ObjectStore>, Path), Finding> {
    let path = Path::from_url_path(url.path())
        .map_err(|err| unreadable(format!("invalid path: {err}")))?;
    let mut base = url;
    base.set_path("/");
    base.set_fragment(None);
    let store = HttpBuilder::new()
        .with_url(base.as_str())
        .with_client_options(ClientOptions::new().with_allow_http(true))
        .with_retry(retry_config(options))
        .with_http_connector(GuardedConnector {
            timeout: options.budget,
        })
        .build()
        .map_err(|err| unreadable(format!("could not create HTTP client: {err}")))?;
    Ok((Arc::new(store), path))
}

async fn s3(
    bucket: String,
    key: Path,
    options: &Options,
) -> Result<(Arc<dyn ObjectStore>, Path), Finding> {
    let client_options = ClientOptions::new()
        .with_timeout(options.budget)
        .with_connect_timeout(CONNECT_TIMEOUT)
        .with_dns_resolver(Arc::new(GuardedDns));
    let region = resolve_bucket_region(&bucket, &client_options)
        .await
        .map_err(|err| unreadable(format!("could not find S3 bucket {bucket}: {err}")))?;
    let builder = if options.uploads_bucket.as_deref() == Some(bucket.as_str()) {
        AmazonS3Builder::from_env()
    } else {
        AmazonS3Builder::new().with_skip_signature(true)
    };
    let store = builder
        .with_bucket_name(&bucket)
        .with_region(region)
        .with_client_options(client_options)
        .with_retry(retry_config(options))
        .build()
        .map_err(|err| unreadable(format!("could not create S3 client: {err}")))?;
    Ok((Arc::new(store), key))
}

fn retry_config(options: &Options) -> RetryConfig {
    RetryConfig {
        max_retries: MAX_RETRIES,
        retry_timeout: options.budget,
        ..RetryConfig::default()
    }
}

fn is_valid_bucket_name(bucket: &str) -> bool {
    let bytes = bucket.as_bytes();
    (3..=63).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'.' || *b == b'-')
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
}

fn check_ip(ip: IpAddr) -> Result<(), Finding> {
    if is_public(ip) {
        Ok(())
    } else {
        Err(unreadable(format!(
            "refusing to read from non-public address {ip}"
        )))
    }
}

async fn lookup(host: &str) -> Result<Vec<IpAddr>, DnsError> {
    let ips: Vec<IpAddr> = tokio::net::lookup_host((host, 0))
        .await?
        .map(|addr| addr.ip())
        .collect();
    if ips.is_empty() {
        return Err(format!("{host} did not resolve to any address").into());
    }
    if let Some(ip) = ips.iter().find(|ip| !is_public(**ip)) {
        return Err(format!(
            "refusing to read from {host}, which resolves to non-public address {ip}"
        )
        .into());
    }
    Ok(ips)
}

/// Returns true if `ip` is a publicly routable unicast address.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => is_public_v6(ip),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a >= 240
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (18..20).contains(&b)))
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    let embedded_v4 = |high: usize| {
        let [a, b] = segments[high].to_be_bytes();
        let [c, d] = segments[high + 1].to_be_bytes();
        Ipv4Addr::new(a, b, c, d)
    };
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    if segments[..6] == [0; 6] {
        return !ip.is_unspecified() && !ip.is_loopback() && is_public_v4(embedded_v4(6));
    }
    if segments[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        return is_public_v4(embedded_v4(6));
    }
    if segments[0] == 0x2002 {
        return is_public_v4(embedded_v4(1));
    }
    !(ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] & 0xffc0) == 0xfec0
        || (segments[0] == 0x2001 && segments[1] == 0)
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
        || segments[..4] == [0x100, 0, 0, 0])
}

#[derive(Debug)]
struct GuardedDns;

impl DnsResolver for GuardedDns {
    fn resolve(&self, host: &str) -> DnsFuture {
        let host = host.to_string();
        Box::pin(async move { lookup(&host).await })
    }
}

struct GuardedReqwestDns;

impl reqwest::dns::Resolve for GuardedReqwestDns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let ips = lookup(&host).await?;
            let addrs: reqwest::dns::Addrs =
                Box::new(ips.into_iter().map(|ip| std::net::SocketAddr::new(ip, 0)));
            Ok(addrs)
        })
    }
}

fn redirect_error(url: &Url, previous: usize) -> Option<String> {
    if previous > MAX_REDIRECTS {
        return Some(format!("too many redirects (more than {MAX_REDIRECTS})"));
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Some(format!(
            "refusing to follow a redirect to scheme {}",
            url.scheme()
        ));
    }
    let ip = match url.host()? {
        Host::Ipv4(ip) => IpAddr::V4(ip),
        Host::Ipv6(ip) => IpAddr::V6(ip),
        Host::Domain(_) => return None,
    };
    (!is_public(ip)).then(|| format!("refusing to follow a redirect to non-public address {ip}"))
}

fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        match redirect_error(attempt.url(), attempt.previous().len()) {
            Some(err) => attempt.error(err),
            None => attempt.follow(),
        }
    })
}

#[derive(Debug)]
struct GuardedConnector {
    timeout: Duration,
}

impl HttpConnector for GuardedConnector {
    fn connect(&self, _: &ClientOptions) -> object_store::Result<HttpClient> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(redirect_policy())
            .no_proxy()
            .dns_resolver(Arc::new(GuardedReqwestDns))
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(self.timeout)
            .no_gzip()
            .no_brotli()
            .no_zstd()
            .no_deflate()
            .build()
            .map_err(|err| object_store::Error::Generic {
                store: "HTTP",
                source: Box::new(err),
            })?;
        Ok(HttpClient::new(client))
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_REDIRECTS, http_store, is_public, redirect_error};
    use crate::{Context, Options};
    use arrow_array::{ArrayRef, RecordBatch, StringArray};
    use object_store::ObjectStoreExt;
    use parquet::arrow::ArrowWriter;
    use std::{
        net::IpAddr,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use url::Url;

    async fn serve(response: String) -> (u16, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                counter.fetch_add(1, Ordering::SeqCst);
                let mut buffer = [0; 4096];
                let _ = socket.read(&mut buffer).await;
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        (port, hits)
    }

    async fn serve_without_head(body: Vec<u8>) -> (u16, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let heads = Arc::new(AtomicUsize::new(0));
        let counter = heads.clone();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = vec![0; 8192];
                let n = socket.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..n]).to_lowercase();
                if request.starts_with("head ") {
                    counter.fetch_add(1, Ordering::SeqCst);
                    let _ = socket
                        .write_all(b"HTTP/1.1 405 Method Not Allowed\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                        .await;
                    continue;
                }
                let len = body.len();
                let range = request
                    .lines()
                    .find_map(|line| line.strip_prefix("range: bytes="))
                    .map(|range| {
                        let (start, end) = range.trim().split_once('-').unwrap();
                        match (start.parse::<usize>().ok(), end.parse::<usize>().ok()) {
                            (None, Some(suffix)) => len.saturating_sub(suffix)..len,
                            (Some(start), Some(end)) => start..(end + 1).min(len),
                            (Some(start), None) => start..len,
                            (None, None) => 0..len,
                        }
                    });
                let (status, range) = match range {
                    Some(range) => ("206 Partial Content", range),
                    None => ("200 OK", 0..len),
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\ncontent-range: bytes {}-{}/{len}\r\nconnection: close\r\n\r\n",
                    range.len(),
                    range.start,
                    range.end - 1
                );
                let _ = socket.write_all(header.as_bytes()).await;
                let _ = socket.write_all(&body[range]).await;
            }
        });
        (port, heads)
    }

    fn parquet_bytes() -> Vec<u8> {
        let ids: ArrayRef = Arc::new(StringArray::from(vec!["a", "b"]));
        let batch = RecordBatch::try_from_iter([("id", ids)]).unwrap();
        let mut buffer = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut buffer, batch.schema(), None).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        buffer
    }

    #[tokio::test]
    async fn loads_footer_without_head_requests() {
        let body = parquet_bytes();
        let size = body.len() as u64;
        let (port, heads) = serve_without_head(body).await;
        let url = Url::parse(&format!("http://127.0.0.1:{port}/x.parquet")).unwrap();
        let (store, path) = http_store(url, &Options::default()).unwrap();
        let context = Context::load(store, path, "x.parquet".to_string())
            .await
            .unwrap();
        assert_eq!(context.size, size);
        assert_eq!(context.metadata.file_metadata().num_rows(), 2);
        assert_eq!(heads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn classifies_addresses() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "169.254.170.2",
            "100.64.0.1",
            "100.127.255.255",
            "0.0.0.0",
            "0.1.2.3",
            "255.255.255.255",
            "224.0.0.1",
            "::",
            "::1",
            "::ffff:127.0.0.1",
            "::127.0.0.1",
            "64:ff9b::a9fe:a9fe",
            "2002:a9fe:a9fe::",
            "fd00:ec2::254",
            "fe80::1",
            "ff02::1",
        ] {
            assert!(!is_public(ip.parse::<IpAddr>().unwrap()), "{ip}");
        }
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "100.128.0.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public(ip.parse::<IpAddr>().unwrap()), "{ip}");
        }
    }

    #[tokio::test]
    async fn does_not_follow_redirects_to_private_ip_literals() {
        let (target, target_hits) = serve(
            "HTTP/1.1 200 OK\r\ncontent-length: 4\r\nconnection: close\r\n\r\nPAR1".to_string(),
        )
        .await;
        let (port, hits) = serve(format!(
            "HTTP/1.1 302 Found\r\nlocation: http://127.0.0.1:{target}/x.parquet\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        ))
        .await;
        let url = Url::parse(&format!("http://127.0.0.1:{port}/x.parquet")).unwrap();
        let (store, path) = http_store(url, &Options::default()).unwrap();
        assert!(store.head(&path).await.is_err());
        assert!(hits.load(Ordering::SeqCst) >= 1);
        assert_eq!(target_hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn does_not_follow_redirects_to_private_hosts() {
        let (target, target_hits) = serve(
            "HTTP/1.1 200 OK\r\ncontent-length: 4\r\nconnection: close\r\n\r\nPAR1".to_string(),
        )
        .await;
        let (port, _) = serve(format!(
            "HTTP/1.1 302 Found\r\nlocation: http://localhost:{target}/x.parquet\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        ))
        .await;
        let url = Url::parse(&format!("http://127.0.0.1:{port}/x.parquet")).unwrap();
        let (store, path) = http_store(url, &Options::default()).unwrap();
        assert!(store.head(&path).await.is_err());
        assert_eq!(target_hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn redirect_policy_follows_public_targets() {
        for url in [
            "https://example.com/x.parquet",
            "http://example.com/x.parquet",
            "https://8.8.8.8/x.parquet",
            "https://[2606:4700:4700::1111]/x.parquet",
        ] {
            assert!(
                redirect_error(&Url::parse(url).unwrap(), 1).is_none(),
                "{url}"
            );
        }
        let url = Url::parse("https://example.com/x.parquet").unwrap();
        assert!(redirect_error(&url, MAX_REDIRECTS).is_none());
    }

    #[test]
    fn redirect_policy_rejects_unsafe_targets() {
        for url in [
            "http://127.0.0.1/x.parquet",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/x.parquet",
            "http://[::1]/x.parquet",
            "http://[::ffff:127.0.0.1]/x.parquet",
            "file:///etc/passwd",
            "ftp://example.com/x.parquet",
        ] {
            assert!(
                redirect_error(&Url::parse(url).unwrap(), 1).is_some(),
                "{url}"
            );
        }
    }

    #[test]
    fn redirect_policy_limits_hops() {
        let url = Url::parse("https://example.com/x.parquet").unwrap();
        assert!(redirect_error(&url, MAX_REDIRECTS + 1).is_some());
    }

    #[tokio::test]
    async fn connection_time_lookup_rejects_private_hosts() {
        let (port, hits) = serve(
            "HTTP/1.1 200 OK\r\ncontent-length: 4\r\nconnection: close\r\n\r\nPAR1".to_string(),
        )
        .await;
        let url = Url::parse(&format!("http://localhost:{port}/x.parquet")).unwrap();
        let (store, path) = http_store(url, &Options::default()).unwrap();
        assert!(store.head(&path).await.is_err());
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn keeps_query_and_full_path() {
        let (port, _) = serve(
            "HTTP/1.1 200 OK\r\ncontent-length: 4\r\nconnection: close\r\n\r\nPAR1".to_string(),
        )
        .await;
        let url = Url::parse(&format!(
            "http://127.0.0.1:{port}/a/year=2016/x.parquet?sig=1"
        ))
        .unwrap();
        let (store, path) = http_store(url, &Options::default()).unwrap();
        assert_eq!(path.as_ref(), "a/year=2016/x.parquet");
        assert_eq!(store.head(&path).await.unwrap().size, 4);
    }
}
