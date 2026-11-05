import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { App } from "aws-cdk-lib";
import { Match, Template } from "aws-cdk-lib/assertions";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as s3deploy from "aws-cdk-lib/aws-s3-deployment";
import { beforeAll, describe, expect, test } from "vitest";
import { CertificateStack } from "../lib/certificate-stack";
import { ValidatorStack } from "../lib/validator-stack";

const DOMAIN = "validate.stac-geoparquet.org";

describe("ValidatorStack", () => {
  let template: Template;

  beforeAll(() => {
    const siteDir = fs.mkdtempSync(path.join(os.tmpdir(), "site-"));
    fs.writeFileSync(path.join(siteDir, "index.html"), "<html></html>");
    const app = new App();
    const env = { account: "123456789012" };
    const certificate = new CertificateStack(app, "Certificate", {
      env: { ...env, region: "us-east-1" },
      crossRegionReferences: true,
    });
    const stack = new ValidatorStack(app, "Validator", {
      env: { ...env, region: "us-west-2" },
      crossRegionReferences: true,
      certificate: certificate.certificate,
      lambdaCode: lambda.Code.fromAsset(siteDir),
      siteSource: s3deploy.Source.asset(siteDir),
    });
    template = Template.fromStack(stack);
  });

  test("uploads bucket expires after one day", () => {
    template.hasResourceProperties("AWS::S3::Bucket", {
      LifecycleConfiguration: {
        Rules: [Match.objectLike({ ExpirationInDays: 1, Status: "Enabled" })],
      },
      CorsConfiguration: {
        CorsRules: [
          Match.objectLike({
            AllowedMethods: ["PUT"],
            AllowedOrigins: [`https://${DOMAIN}`, "http://localhost:5173"],
          }),
        ],
      },
    });
  });

  test("lambda has reserved concurrency", () => {
    template.hasResourceProperties("AWS::Lambda::Function", {
      ReservedConcurrentExecutions: 25,
    });
  });

  test("lambda has 29s timeout and UPLOADS_BUCKET env", () => {
    template.hasResourceProperties("AWS::Lambda::Function", {
      Timeout: 29,
      MemorySize: 1024,
      Architectures: ["arm64"],
      Environment: { Variables: { UPLOADS_BUCKET: Match.anyValue() } },
    });
  });

  test("http api has throttling", () => {
    template.hasResourceProperties("AWS::ApiGatewayV2::Stage", {
      StageName: "$default",
      AutoDeploy: true,
      DefaultRouteSettings: {
        ThrottlingRateLimit: 10,
        ThrottlingBurstLimit: 20,
      },
    });
    const routeKeys = Object.values(
      template.findResources("AWS::ApiGatewayV2::Route"),
    ).map((route) => route.Properties.RouteKey);
    expect(routeKeys.sort()).toEqual([
      "OPTIONS /api/{proxy+}",
      "POST /api/uploads",
      "POST /api/validate",
    ]);
  });

  test("distribution has api behavior and alias", () => {
    template.hasResourceProperties("AWS::CloudFront::Distribution", {
      DistributionConfig: Match.objectLike({
        Aliases: [DOMAIN],
        CacheBehaviors: [
          Match.objectLike({
            PathPattern: "/api/*",
            AllowedMethods: [
              "GET",
              "HEAD",
              "OPTIONS",
              "PUT",
              "PATCH",
              "POST",
              "DELETE",
            ],
          }),
        ],
      }),
    });
  });

  test("spa fallback is a default-behavior function, not error responses", () => {
    const [distribution] = Object.values(
      template.findResources("AWS::CloudFront::Distribution"),
    );
    const config = distribution.Properties.DistributionConfig;
    expect(config.CustomErrorResponses).toBeUndefined();
    expect(config.DefaultCacheBehavior.FunctionAssociations).toMatchObject([
      { EventType: "viewer-request" },
    ]);
    expect(config.CacheBehaviors[0].FunctionAssociations).toBeUndefined();
  });
});
