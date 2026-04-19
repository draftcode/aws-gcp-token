// SPDX-License-Identifier: Apache-2.0

//! Fetch an AWS IAM JWT via STS GetWebIdentityToken and emit it in the JSON
//! shape expected by google-auth's executable-sourced external account
//! credentials.
//!
//! The audience is supplied by google-auth through the
//! `GOOGLE_EXTERNAL_ACCOUNT_AUDIENCE` env var; when
//! `GOOGLE_EXTERNAL_ACCOUNT_OUTPUT_FILE` is set, the same JSON is written
//! there atomically so subsequent refreshes can reuse the JWT until expiry.

use std::env;
use std::fs;
use std::process;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use aws_credential_types::Credentials;
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
use aws_sigv4::sign::v4;
use aws_smithy_runtime_api::client::identity::Identity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Seconds subtracted from the JWT's real expiry when reporting
/// `expiration_time` back to google-auth, so the cached JWT never expires
/// mid-STS-exchange.
const EXPIRATION_BUFFER_SEC: i64 = 300;
const ECS_METADATA_HOST: &str = "http://169.254.170.2";

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EcsCredentials {
    access_key_id: String,
    secret_access_key: String,
    token: String,
    #[allow(dead_code)]
    expiration: String,
}

#[derive(Serialize)]
struct SuccessResponse<'a> {
    version: u32,
    success: bool,
    token_type: &'a str,
    id_token: String,
    expiration_time: i64,
}

#[derive(Serialize)]
struct FailureResponse<'a> {
    version: u32,
    success: bool,
    code: &'a str,
    message: String,
}

fn main() {
    if let Err(err) = run() {
        let code = err
            .downcast_ref::<ExitCode>()
            .map(ExitCode::as_str)
            .unwrap_or("AWS_ERROR");
        let failure = FailureResponse {
            version: 1,
            success: false,
            code,
            message: format!("{err:#}"),
        };
        println!("{}", serde_json::to_string(&failure).unwrap());
        process::exit(1);
    }
}

#[derive(Debug)]
enum ExitCode {
    MissingAudience,
}

impl ExitCode {
    fn as_str(&self) -> &'static str {
        match self {
            ExitCode::MissingAudience => "MISSING_AUDIENCE",
        }
    }
}

impl std::fmt::Display for ExitCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ExitCode::MissingAudience => "GOOGLE_EXTERNAL_ACCOUNT_AUDIENCE is not set",
        })
    }
}

impl std::error::Error for ExitCode {}

fn run() -> Result<()> {
    let audience =
        env::var("GOOGLE_EXTERNAL_ACCOUNT_AUDIENCE").map_err(|_| ExitCode::MissingAudience)?;

    let creds = fetch_ecs_credentials()?;
    let region = env::var("AWS_REGION")
        .or_else(|_| env::var("AWS_DEFAULT_REGION"))
        .map_err(|_| anyhow!("Neither AWS_REGION nor AWS_DEFAULT_REGION is set"))?;

    let (id_token, expiration) = call_get_web_identity_token(
        &region,
        &creds.access_key_id,
        &creds.secret_access_key,
        Some(&creds.token),
        &audience,
    )?;

    let expiration_time = expiration.timestamp() - EXPIRATION_BUFFER_SEC;
    let resp = SuccessResponse {
        version: 1,
        success: true,
        token_type: "urn:ietf:params:oauth:token-type:jwt",
        id_token,
        expiration_time,
    };
    let rendered = serde_json::to_string(&resp)?;
    println!("{rendered}");

    if let Ok(output_file) = env::var("GOOGLE_EXTERNAL_ACCOUNT_OUTPUT_FILE") {
        write_cache(&output_file, &rendered);
    }
    Ok(())
}

fn fetch_ecs_credentials() -> Result<EcsCredentials> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();

    let body = if let Ok(relative) = env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI") {
        let url = format!("{ECS_METADATA_HOST}{relative}");
        agent
            .get(&url)
            .call()
            .context("fetching ECS credential metadata")?
            .into_string()?
    } else if let Ok(full) = env::var("AWS_CONTAINER_CREDENTIALS_FULL_URI") {
        let mut req = agent.get(&full);
        if let Ok(token) = env::var("AWS_CONTAINER_AUTHORIZATION_TOKEN") {
            req = req.set("Authorization", &token);
        }
        req.call()
            .context("fetching ECS credential metadata")?
            .into_string()?
    } else {
        bail!(
            "Neither AWS_CONTAINER_CREDENTIALS_RELATIVE_URI nor \
             AWS_CONTAINER_CREDENTIALS_FULL_URI is set"
        );
    };

    serde_json::from_str(&body).context("parsing ECS credential response")
}

// TODO(aws-sdk-rust): replace the raw SigV4 request below with
// `aws_sdk_sts::Client::get_web_identity_token` once that operation is
// generated in the Rust SDK. As of the initial 0.1.0 release, aws-sdk-sts
// only exposes `assume_role_with_web_identity` — tracking at
// https://github.com/awslabs/aws-sdk-rust.
fn call_get_web_identity_token(
    region: &str,
    access_key: &str,
    secret_key: &str,
    session_token: Option<&str>,
    audience: &str,
) -> Result<(String, DateTime<Utc>)> {
    let host = format!("sts.{region}.amazonaws.com");
    let endpoint = format!("https://{host}/");

    let body = form_urlencoded::Serializer::new(String::new())
        .append_pair("Action", "GetWebIdentityToken")
        .append_pair("Version", "2011-06-15")
        .append_pair("Audience.member.1", audience)
        .append_pair("SigningAlgorithm", "ES384")
        .append_pair("DurationSeconds", "3600")
        .finish();

    let identity: Identity = Credentials::new(
        access_key,
        secret_key,
        session_token.map(str::to_string),
        None,
        "aws-gcp-token",
    )
    .into();

    let signing_settings = SigningSettings::default();
    let signing_params = v4::SigningParams::builder()
        .identity(&identity)
        .region(region)
        .name("sts")
        .time(SystemTime::now())
        .settings(signing_settings)
        .build()
        .context("building SigV4 signing params")?
        .into();

    let content_type = "application/x-www-form-urlencoded; charset=utf-8";
    let signable = SignableRequest::new(
        "POST",
        &endpoint,
        std::iter::once(("content-type", content_type)),
        SignableBody::Bytes(body.as_bytes()),
    )
    .context("building signable request")?;

    let (signing_instructions, _signature) = sign(signable, &signing_params)
        .context("signing STS request")?
        .into_parts();

    let mut req = ureq::post(&endpoint).set("content-type", content_type);
    let (headers, _params) = signing_instructions.into_parts();
    for header in headers {
        req = req.set(header.name(), header.value());
    }

    let response = req
        .send_bytes(body.as_bytes())
        .context("POST to STS endpoint")?;
    let text = response.into_string()?;
    parse_get_web_identity_token_response(&text)
}

#[derive(Deserialize)]
struct StsResponse {
    #[serde(rename = "GetWebIdentityTokenResult")]
    result: GetWebIdentityTokenResult,
}

#[derive(Deserialize)]
struct GetWebIdentityTokenResult {
    #[serde(rename = "WebIdentityToken")]
    web_identity_token: String,
    #[serde(rename = "Expiration")]
    expiration: String,
}

fn parse_get_web_identity_token_response(xml: &str) -> Result<(String, DateTime<Utc>)> {
    let parsed: StsResponse =
        quick_xml::de::from_str(xml).with_context(|| format!("parsing STS response XML: {xml}"))?;
    let expiration = DateTime::parse_from_rfc3339(&parsed.result.expiration)
        .with_context(|| format!("parsing Expiration {:?}", parsed.result.expiration))?
        .with_timezone(&Utc);
    Ok((parsed.result.web_identity_token, expiration))
}

fn write_cache(path: &str, content: &str) {
    let tmp = format!("{path}.tmp.{}", process::id());
    let result: Result<()> = (|| {
        fs::write(&tmp, content)?;
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("Warning: failed to write cache file: {e:#}");
        let _ = fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_STS_RESPONSE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<GetWebIdentityTokenResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/">
  <GetWebIdentityTokenResult>
    <WebIdentityToken>eyJhbGciOiJFUzM4NCIsImtpZCI6IkVDMzg0XzAifQ.eyJhdWQiOiJ0ZXN0In0.sig</WebIdentityToken>
    <Expiration>2026-04-19T12:00:00Z</Expiration>
  </GetWebIdentityTokenResult>
  <ResponseMetadata>
    <RequestId>11111111-2222-3333-4444-555555555555</RequestId>
  </ResponseMetadata>
</GetWebIdentityTokenResponse>"#;

    #[test]
    fn parses_sts_response() {
        let (token, expiration) = parse_get_web_identity_token_response(SAMPLE_STS_RESPONSE)
            .expect("valid STS response should parse");
        assert_eq!(
            token,
            "eyJhbGciOiJFUzM4NCIsImtpZCI6IkVDMzg0XzAifQ.eyJhdWQiOiJ0ZXN0In0.sig"
        );
        let expected = DateTime::parse_from_rfc3339("2026-04-19T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(expiration, expected);
    }

    #[test]
    fn parses_sts_response_with_xml_entities() {
        // Tokens shouldn't contain entities, but the parser must still decode
        // them correctly per XML spec rather than treating them literally.
        let xml =
            SAMPLE_STS_RESPONSE.replace("sig</WebIdentityToken>", "a&amp;b</WebIdentityToken>");
        let (token, _) = parse_get_web_identity_token_response(&xml)
            .expect("response with entities should parse");
        assert!(token.ends_with("a&b"), "entity was not decoded: {token}");
    }

    #[test]
    fn rejects_malformed_xml() {
        let err = parse_get_web_identity_token_response("<not-sts-response/>")
            .expect_err("malformed response must error");
        // Error message should include the offending XML for operator debugging.
        assert!(format!("{err:#}").contains("parsing STS response XML"));
    }

    #[test]
    fn rejects_non_rfc3339_expiration() {
        let xml = SAMPLE_STS_RESPONSE.replace(
            "<Expiration>2026-04-19T12:00:00Z</Expiration>",
            "<Expiration>not-a-date</Expiration>",
        );
        assert!(parse_get_web_identity_token_response(&xml).is_err());
    }

    #[test]
    fn write_cache_writes_atomically() {
        let dir = std::env::temp_dir().join(format!("aws-gcp-token-test-{}", process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache.json");

        write_cache(path.to_str().unwrap(), r#"{"version":1}"#);
        let got = fs::read_to_string(&path).unwrap();
        assert_eq!(got, r#"{"version":1}"#);

        // No leftover temp files in the directory.
        for entry in fs::read_dir(&dir).unwrap() {
            let name = entry.unwrap().file_name();
            assert_eq!(name, "cache.json", "unexpected leftover file: {name:?}");
        }

        fs::remove_dir_all(&dir).ok();
    }
}
