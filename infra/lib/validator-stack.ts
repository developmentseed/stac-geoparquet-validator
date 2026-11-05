import {
  CfnOutput,
  Duration,
  RemovalPolicy,
  Stack,
  StackProps,
} from "aws-cdk-lib";
import * as apigw from "aws-cdk-lib/aws-apigatewayv2";
import * as integrations from "aws-cdk-lib/aws-apigatewayv2-integrations";
import * as acm from "aws-cdk-lib/aws-certificatemanager";
import * as cloudfront from "aws-cdk-lib/aws-cloudfront";
import * as origins from "aws-cdk-lib/aws-cloudfront-origins";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as s3deploy from "aws-cdk-lib/aws-s3-deployment";
import { RustFunction } from "cargo-lambda-cdk";
import { Construct } from "constructs";
import * as path from "node:path";
import { DOMAIN_NAME } from "./certificate-stack";

/** Properties for the {@link ValidatorStack}. */
export interface ValidatorStackProps extends StackProps {
  /** The `us-east-1` certificate for the distribution's alias. */
  certificate: acm.ICertificate;
  /** Prebuilt Lambda code. When omitted, the Rust backend is built with cargo-lambda. */
  lambdaCode?: lambda.Code;
  /** The site contents. Defaults to `frontend/dist`. */
  siteSource?: s3deploy.ISource;
}

/** The validator's Lambda API, uploads bucket, and CloudFront-fronted site. */
export class ValidatorStack extends Stack {
  constructor(scope: Construct, id: string, props: ValidatorStackProps) {
    super(scope, id, props);

    const uploads = new s3.Bucket(this, "Uploads", {
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      enforceSSL: true,
      removalPolicy: RemovalPolicy.DESTROY,
      autoDeleteObjects: true,
      lifecycleRules: [{ expiration: Duration.days(1) }],
      cors: [
        {
          allowedMethods: [s3.HttpMethods.PUT],
          allowedOrigins: [`https://${DOMAIN_NAME}`, "http://localhost:5173"],
          allowedHeaders: ["*"],
        },
      ],
    });

    const functionProps = {
      architecture: lambda.Architecture.ARM_64,
      memorySize: 1024,
      timeout: Duration.seconds(29),
      environment: { UPLOADS_BUCKET: uploads.bucketName },
    };
    const handler = props.lambdaCode
      ? new lambda.Function(this, "Handler", {
          ...functionProps,
          runtime: lambda.Runtime.PROVIDED_AL2023,
          handler: "bootstrap",
          code: props.lambdaCode,
        })
      : new RustFunction(this, "Handler", {
          ...functionProps,
          manifestPath: path.join(__dirname, "../../backend/Cargo.toml"),
          binaryName: "lambda",
        });
    uploads.grantRead(handler);
    uploads.grantPut(handler);

    const api = new apigw.HttpApi(this, "Api", { createDefaultStage: false });
    new apigw.HttpStage(this, "DefaultStage", {
      httpApi: api,
      autoDeploy: true,
      throttle: { rateLimit: 10, burstLimit: 20 },
    });
    const integration = new integrations.HttpLambdaIntegration(
      "Integration",
      handler,
    );
    api.addRoutes({
      path: "/api/validate",
      methods: [apigw.HttpMethod.POST],
      integration,
    });
    api.addRoutes({
      path: "/api/uploads",
      methods: [apigw.HttpMethod.POST],
      integration,
    });
    api.addRoutes({
      path: "/api/{proxy+}",
      methods: [apigw.HttpMethod.OPTIONS],
      integration,
    });

    const site = new s3.Bucket(this, "Site", {
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      enforceSSL: true,
      removalPolicy: RemovalPolicy.DESTROY,
      autoDeleteObjects: true,
    });
    const apiOrigin = new origins.HttpOrigin(
      `${api.apiId}.execute-api.${this.region}.amazonaws.com`,
      { protocolPolicy: cloudfront.OriginProtocolPolicy.HTTPS_ONLY },
    );
    const distribution = new cloudfront.Distribution(this, "Distribution", {
      defaultRootObject: "index.html",
      domainNames: [DOMAIN_NAME],
      certificate: props.certificate,
      defaultBehavior: {
        origin: origins.S3BucketOrigin.withOriginAccessControl(site),
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
      },
      additionalBehaviors: {
        "/api/*": {
          origin: apiOrigin,
          allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
          cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
          originRequestPolicy:
            cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
          viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.HTTPS_ONLY,
        },
      },
      errorResponses: [403, 404].map((httpStatus) => ({
        httpStatus,
        responseHttpStatus: 200,
        responsePagePath: "/index.html",
        ttl: Duration.seconds(0),
      })),
    });

    new s3deploy.BucketDeployment(this, "SiteDeployment", {
      destinationBucket: site,
      sources: [
        props.siteSource ??
          s3deploy.Source.asset(path.join(__dirname, "../../frontend/dist")),
      ],
      distribution,
      distributionPaths: ["/*"],
    });

    new CfnOutput(this, "DistributionDomainName", {
      value: distribution.distributionDomainName,
    });
  }
}
