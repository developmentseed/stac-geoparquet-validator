//! AWS Lambda entry point for the validator HTTP API.

use lambda_http::{Error, run, service_fn};
use stac_geoparquet_validator::Options;
use stac_geoparquet_validator::api::{State, handle};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let options = Options {
        uploads_bucket: std::env::var("UPLOADS_BUCKET")
            .ok()
            .filter(|b| !b.is_empty()),
        ..Options::default()
    };
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let s3 = Some(aws_sdk_s3::Client::new(&config));
    let state = State { options, s3 };
    let state = &state;
    run(service_fn(move |req| handle(req, state))).await
}
