# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-04-19

### Added

- Initial release. Fetches an AWS IAM JWT via `sts:GetWebIdentityToken` and
  emits it in the JSON shape expected by `google-auth`'s executable-sourced
  external account credentials.
- Implemented in Go using the official
  [`aws-sdk-go-v2`](https://pkg.go.dev/github.com/aws/aws-sdk-go-v2) — no
  hand-rolled SigV4 or XML parsing.
- AWS credential discovery via the standard SDK chain (ECS container
  credentials through `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI` /
  `AWS_CONTAINER_CREDENTIALS_FULL_URI`, and the rest).
- Atomic cache file writes when `GOOGLE_EXTERNAL_ACCOUNT_OUTPUT_FILE` is set.
- Release artifacts produced by GoReleaser: `.tar.gz` per target triple
  (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`) plus `.deb` per
  Linux arch (`amd64`, `arm64`) attached to each GitHub Release.
- GitHub Actions: `ci.yml` runs `hk check --all` + `go test ./...`;
  `release.yml` runs `goreleaser` (snapshot on branches/PRs, real release on
  `v*` tags).
