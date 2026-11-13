import { App } from "aws-cdk-lib";
import { CertificateStack } from "../lib/certificate-stack";
import { ValidatorStack } from "../lib/validator-stack";

const app = new App();
const account = process.env.CDK_DEFAULT_ACCOUNT;

const certificate = new CertificateStack(
  app,
  "stac-geoparquet-validator-certificate",
  {
    env: { account, region: "us-east-1" },
    crossRegionReferences: true,
  },
);
new ValidatorStack(app, "stac-geoparquet-validator", {
  env: { account, region: "us-west-2" },
  crossRegionReferences: true,
  certificate: certificate.certificate,
});
