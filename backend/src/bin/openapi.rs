//! Prints the OpenAPI description of the HTTP API.

fn main() {
    print!("{}", stac_geoparquet_validator::api::openapi_json());
}
