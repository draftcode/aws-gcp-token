# aws-gcp-token

A tiny, fast helper that turns AWS IAM credentials (e.g. an ECS task role) into
a signed JWT for GCP Workload Identity Federation, in the exact JSON shape that
[`google-auth`'s executable-sourced external account
credentials](https://google.github.io/google-auth-library-python/reference/google.auth.external_account_authorized_user.html)
expects.

It calls `sts:GetWebIdentityToken` with the audience supplied by `google-auth`
and emits the resulting JWT on stdout. When `google-auth` provides a cache
path, the same JSON is written atomically so subsequent refreshes reuse the
JWT until just before it expires.

## Why this exists

The obvious Python implementation would use `boto3` to call STS, but importing
`boto3` costs ~1.5s of startup. The tempting workaround — hand-rolling SigV4
and XML parsing in `urllib` — works, but you then own ~250 lines of
security-sensitive code that has to track any future AWS signing or response
changes.

This project gives you both: a Rust binary that uses the official
[`aws-sigv4`](https://docs.rs/aws-sigv4) crate for signing and starts in a few
milliseconds. Distributed as a plain static-ish binary per target triple —
download, extract, drop on `$PATH`.

## Install

Each release attaches:

- **`.tar.gz`** per target triple — works anywhere `curl` + `tar` do.
  Triples: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
  `x86_64-apple-darwin`, `aarch64-apple-darwin`.
- **`.deb`** per Linux arch (`amd64`, `arm64`) — for Debian/Ubuntu base images.

In a Debian/Ubuntu-based Dockerfile, grab the `.deb`:

```dockerfile
ARG AWS_GCP_TOKEN_VERSION=v0.1.0
RUN set -eux; \
    curl -fL -o /tmp/aws-gcp-token.deb \
      "https://github.com/OWNER/REPO/releases/download/${AWS_GCP_TOKEN_VERSION}/aws-gcp-token_0.1.0-1_amd64.deb"; \
    dpkg -i /tmp/aws-gcp-token.deb; \
    rm /tmp/aws-gcp-token.deb
```

Or use the raw tarball (works anywhere `curl` + `tar` do):

```bash
curl -fL https://github.com/OWNER/REPO/releases/download/v0.1.0/aws-gcp-token-v0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  | tar -xz -C /usr/local/bin
```

From source:

```bash
cargo install --git https://github.com/OWNER/REPO --tag v0.1.0 aws-gcp-token
```

## Wire it into google-auth

Point `google-auth`'s external account credential file at the binary:

```json
{
  "type": "external_account",
  "audience": "//iam.googleapis.com/projects/…/workloadIdentityPools/…/providers/…",
  "subject_token_type": "urn:ietf:params:oauth:token-type:jwt",
  "token_url": "https://sts.googleapis.com/v1/token",
  "credential_source": {
    "executable": {
      "command": "aws-gcp-token",
      "timeout_millis": 5000,
      "output_file": "/tmp/aws-gcp-token.cache"
    }
  }
}
```

Set `GOOGLE_EXTERNAL_ACCOUNT_ALLOW_EXECUTABLES=1` in the environment so
`google-auth` is willing to exec the helper.

## Environment contract

`google-auth` sets these when invoking the executable:

| Variable | Notes |
| --- | --- |
| `GOOGLE_EXTERNAL_ACCOUNT_AUDIENCE` | Required. Passed through to STS as `Audience.member.1`. |
| `GOOGLE_EXTERNAL_ACCOUNT_OUTPUT_FILE` | Optional. When set, the helper writes its JSON response here atomically. |

AWS credentials are discovered via the ECS container-credentials endpoint:

| Variable | Notes |
| --- | --- |
| `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI` | Preferred. Fetched against `http://169.254.170.2`. |
| `AWS_CONTAINER_CREDENTIALS_FULL_URI` | Fallback full URL. |
| `AWS_CONTAINER_AUTHORIZATION_TOKEN` | Optional bearer header for `FULL_URI`. |
| `AWS_REGION` / `AWS_DEFAULT_REGION` | Required. STS region (e.g. `ap-northeast-1`, `us-east-1`). |

## Output

On success (stdout):

```json
{
  "version": 1,
  "success": true,
  "token_type": "urn:ietf:params:oauth:token-type:jwt",
  "id_token": "<jwt>",
  "expiration_time": 1700000000
}
```

`expiration_time` is the JWT's real expiry minus a 5-minute safety buffer, so
the cached JWT never expires mid STS exchange.

On failure:

```json
{
  "version": 1,
  "success": false,
  "code": "AWS_ERROR",
  "message": "..."
}
```

The process exits 1 on failure. `code` is `MISSING_AUDIENCE` when the audience
env var is unset, `AWS_ERROR` otherwise.

## Build from source

```bash
cargo build --release
```

## Development

This repo uses [`hk`](https://hk.jdx.dev) to wire up pre-commit / pre-push
hooks for `rustfmt`, `clippy`, `cargo check`, and `taplo` (TOML formatter).

```bash
# one-time: install the git hooks declared in hk.pkl
hk install

# run everything ad-hoc
hk check   # no file modifications
hk fix     # auto-fix what's fixable
```

Tool versions are pinned in `mise.toml`; if you have [`mise`](https://mise.jdx.dev)
installed, `mise install` fetches `rust`, `hk`, `taplo`, `actionlint`,
`zizmor`, `typos`, `pkl`, and `cargo-deny` at the versions the hooks expect.

## License

Apache-2.0
