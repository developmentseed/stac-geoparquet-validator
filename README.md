# stac-geoparquet validator

[![CI](https://img.shields.io/github/actions/workflow/status/developmentseed/stac-geoparquet-validator/ci.yml?branch=main&style=for-the-badge&label=CI)](https://github.com/developmentseed/stac-geoparquet-validator/actions/workflows/ci.yml)
[![Deploy](https://img.shields.io/github/actions/workflow/status/developmentseed/stac-geoparquet-validator/deploy.yml?branch=main&style=for-the-badge&label=Deploy)](https://github.com/developmentseed/stac-geoparquet-validator/actions/workflows/deploy.yml)

<img src="frontend/public/logo.png" alt="stac-geoparquet validator logo" width="96">
    
    
A web tool and public API that takes a [stac-geoparquet](https://github.com/radiantearth/stac-geoparquet-spec) file, by URL or upload, and validates it against the [spec and best practices](https://validate.stac-geoparquet.org/api/checks).

<https://validate.stac-geoparquet.org/>

## Layout

- `backend/`: Rust validator, built directly as an AWS Lambda
- `frontend/`: Single-page web application
- `infra/`: AWS CDK app (TypeScript)
- `backend/checks.json`: the checks used by the validator (the public contract), served at `GET /api/checks`

## Local development

You need Rust, [cargo-lambda](https://www.cargo-lambda.info/), Node 22 (run `nvm use` to pick it up from `.nvmrc`), and yarn.

To start the backend:

```sh
cd backend
cargo lambda watch
```

In `frontend/.env.local`, point the dev server's proxy at it:

```sh
VITE_API_PROXY=http://localhost:9000/lambda-url/lambda
```

Then start the frontend:

```sh
cd frontend
yarn install
yarn dev
```

Run the tests:

```sh
cd backend && cargo test
cd frontend && yarn test
cd infra && yarn install && yarn test
```

## API

Interactive docs (Swagger UI) are at [`/api/docs`](https://validate.stac-geoparquet.org/api/docs). The API is described by an OpenAPI 3.1 document at [`GET /api/openapi.json`](https://validate.stac-geoparquet.org/api/openapi.json), generated from the Rust types with [utoipa](https://github.com/juhaku/utoipa). A copy is committed at `backend/openapi.json` (regenerate it with `cargo run --bin openapi > openapi.json` in `backend/`), and the frontend's TypeScript types in `frontend/src/openapi.ts` are generated from it with `yarn generate`.

### `POST /api/validate`

```json
{ "href": "https://example.com/items.parquet" }
```

`href` can be an `http(s)://` URL to a range-capable server or an `s3://` URL.
Responds with a report:

```json
{
  "report_version": "1",
  "href": "https://example.com/items.parquet",
  "summary": {
    "parquet_valid": true,
    "geoparquet_valid": true,
    "geoparquet_version": "1.1.0",
    "native_geospatial_types": false,
    "stac_geoparquet_valid": true,
    "counts": { "error": 0, "warning": 1, "compat": 0, "info": 3 }
  },
  "findings": [
    {
      "id": "bp.compression",
      "severity": "warning",
      "section": "geoparquet_best_practices",
      "message": "...",
      "reference": "https://..."
    }
  ]
}
```

Finding ids are stable and listed by `GET /api/checks`; messages are free text.

### `GET /api/checks`

Lists every check id the validator can emit, with its severity, section, description and links to the upstream text it implements, plus the source document for each section and any retired ids. The format is chosen from the `Accept` header:

| `Accept` | Response |
| --- | --- |
| `application/json`, `*/*` or none | JSON matching [`backend/checks.schema.json`](backend/checks.schema.json) |
| `text/markdown`, `text/plain` or `text/*` | Markdown |
| `text/html` (e.g. a browser) | HTML |

## License

[MIT](LICENSE)
