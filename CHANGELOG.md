# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-04-19

### Added

- Rust port of `get_gcp_token.py`: fetches an AWS IAM JWT via
  `sts:GetWebIdentityToken` and emits it in the JSON shape expected by
  `google-auth`'s executable-sourced external account credentials.
- ECS container credential discovery via `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI`
  and `AWS_CONTAINER_CREDENTIALS_FULL_URI`.
- SigV4 signing delegated to the official `aws-sigv4` crate; no hand-rolled
  crypto.
- Atomic cache file writes when `GOOGLE_EXTERNAL_ACCOUNT_OUTPUT_FILE` is set.
- Standalone-binary distribution: `.tar.gz` per target triple plus `.deb` per
  Linux arch attached to each GitHub Release.
- GitHub Actions: `ci.yml` runs `hk check --all` + `cargo test`; `release.yml`
  builds release binaries for `x86_64-unknown-linux-gnu` and
  `aarch64-unknown-linux-gnu` on native runners and attaches them to the
  GitHub Release on `v*` tags.
