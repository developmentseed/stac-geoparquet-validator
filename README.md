# stac-geoparquet validator

A web tool and public API that takes a [stac-geoparquet](https://github.com/radiantearth/stac-geoparquet-spec) file, by URL or upload, and tells you whether you can trust it.

The report keeps apart what makes a file **invalid**, what is valid but **not best practice**, and what is valid but **some readers cannot handle**. It answers four questions up front: is it valid Parquet, valid GeoParquet, which GeoParquet version, and valid stac-geoparquet.

Live at <https://validate.stac-geoparquet.org/>.

## Layout

- `backend/`: Rust validator, deployed as an AWS Lambda. All validation happens here, using HTTP range requests, footer first, with bounded sampling.
- `frontend/`: React, TypeScript, Vite, Chakra UI single-page app.
- `infra/`: AWS CDK app (TypeScript).
- `docs/checks.md`: the check ids, a public contract.

## Local development

You need Rust, [cargo-lambda](https://www.cargo-lambda.info/), Node 22 and yarn.

Start the backend:

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

The service is public and anonymous. CORS allows any origin, and requests are throttled (10 requests/s, burst 20). Validation must finish within 25 s.

### `POST /api/validate`

```json
{ "href": "https://example.com/items.parquet" }
```

`href` can be an `http(s)://` URL to a range-capable server, or an `s3://` URL (read unsigned, except in the uploads bucket). Responds with a report:

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

Finding ids are stable and listed in [docs/checks.md](docs/checks.md); messages are free text.

### `POST /api/uploads`

```json
{ "filename": "items.parquet", "size": 123456 }
```

Responds with `{ "upload_url": "...", "href": "s3://..." }`. `PUT` the file (at most 2 GB) to `upload_url`, then pass `href` to `/api/validate`. Uploads are deleted after one day.

## Continuous integration and deployment

- `.github/workflows/ci.yml` runs formatting, lints, type checks and tests for `backend/`, `frontend/` and `infra/` on every pull request.
- `.github/workflows/deploy.yml` runs on every push to `main` (and on manual dispatch). It runs the same CI jobs, then runs `cdk deploy --all` in the `production` environment, but only if they pass.

The deploy job authenticates to AWS with GitHub OIDC. One-time setup, after the first manual deploy below:

1. In the AWS account, add the GitHub OIDC identity provider (`token.actions.githubusercontent.com`, audience `sts.amazonaws.com`) if it isn't there already.
2. Create an IAM role:
   - Trust policy: allow `sts:AssumeRoleWithWebIdentity` from that provider, with the conditions `token.actions.githubusercontent.com:aud` = `sts.amazonaws.com` and `token.actions.githubusercontent.com:sub` = `repo:<owner>/stac-geoparquet-validator:environment:production`.
   - Permissions: `sts:AssumeRole` on `arn:aws:iam::<account>:role/cdk-*`. CDK deploys through its bootstrap roles, so the role needs nothing else.
3. In the GitHub repository settings, set the `production` environment variable `AWS_DEPLOY_ROLE_ARN` to that role's ARN. Add protection rules to the environment if you want deploys to need approval.

## Deploying

The first deploy is done by hand, because it creates the certificate that needs DNS records in Gandi. After that, pushes to `main` redeploy automatically (see above). You need AWS credentials for the target account, cargo-lambda, and the `aarch64-unknown-linux-gnu` Rust target.

1. Build the frontend: `yarn install && yarn build` in `frontend/`.
2. Bootstrap CDK once per region: `yarn cdk bootstrap aws://ACCOUNT/us-east-1 aws://ACCOUNT/us-west-2` in `infra/`.
3. Deploy: `yarn install && yarn cdk deploy --all` in `infra/`. The `ValidatorCertificate` stack lives in `us-east-1` and the `Validator` stack in `us-west-2`.
4. While the certificate stack waits for validation, copy the ACM validation CNAME from the ACM console (or the CloudFormation events) and add it in Gandi.
5. Add a `validate` CNAME in Gandi pointing at the `DistributionDomainName` stack output.

DNS for `stac-geoparquet.org` is managed by hand on gandi.net; no Route 53 hosted zone is involved.
