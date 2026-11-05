import { Stack, StackProps } from "aws-cdk-lib";
import * as acm from "aws-cdk-lib/aws-certificatemanager";
import { Construct } from "constructs";

export const DOMAIN_NAME = "validate.stac-geoparquet.org";

/**
 * The CloudFront certificate, which must live in `us-east-1`.
 *
 * DNS validation is used without a hosted zone: add the validation CNAME shown in the
 * ACM console by hand in Gandi.
 */
export class CertificateStack extends Stack {
  public readonly certificate: acm.Certificate;

  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);
    this.certificate = new acm.Certificate(this, "Certificate", {
      domainName: DOMAIN_NAME,
      validation: acm.CertificateValidation.fromDns(),
    });
  }
}
