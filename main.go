// SPDX-License-Identifier: Apache-2.0

// Fetch an AWS IAM JWT via STS GetWebIdentityToken and emit it in the JSON
// shape expected by google-auth's executable-sourced external account
// credentials.
//
// The audience is supplied by google-auth through the
// GOOGLE_EXTERNAL_ACCOUNT_AUDIENCE env var; when
// GOOGLE_EXTERNAL_ACCOUNT_OUTPUT_FILE is set, the same JSON is written there
// atomically so subsequent refreshes can reuse the JWT until expiry.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/service/sts"
)

// Seconds subtracted from the JWT's real expiry when reporting
// expiration_time back to google-auth, so the cached JWT never expires
// mid-STS-exchange.
const expirationBufferSec = 300

type successResponse struct {
	Version        int    `json:"version"`
	Success        bool   `json:"success"`
	TokenType      string `json:"token_type"`
	IDToken        string `json:"id_token"`
	ExpirationTime int64  `json:"expiration_time"`
}

type failureResponse struct {
	Version int    `json:"version"`
	Success bool   `json:"success"`
	Code    string `json:"code"`
	Message string `json:"message"`
}

var errMissingAudience = errors.New("GOOGLE_EXTERNAL_ACCOUNT_AUDIENCE is not set")

func main() {
	if err := run(context.Background()); err != nil {
		code := "AWS_ERROR"
		if errors.Is(err, errMissingAudience) {
			code = "MISSING_AUDIENCE"
		}
		out, _ := json.Marshal(failureResponse{
			Version: 1,
			Success: false,
			Code:    code,
			Message: err.Error(),
		})
		fmt.Println(string(out))
		os.Exit(1)
	}
}

func run(ctx context.Context) error {
	audience := os.Getenv("GOOGLE_EXTERNAL_ACCOUNT_AUDIENCE")
	if audience == "" {
		return errMissingAudience
	}

	region := os.Getenv("AWS_REGION")
	if region == "" {
		region = os.Getenv("AWS_DEFAULT_REGION")
	}
	if region == "" {
		return errors.New("neither AWS_REGION nor AWS_DEFAULT_REGION is set")
	}

	cfg, err := config.LoadDefaultConfig(ctx, config.WithRegion(region))
	if err != nil {
		return fmt.Errorf("loading AWS config: %w", err)
	}

	client := sts.NewFromConfig(cfg)
	out, err := client.GetWebIdentityToken(ctx, &sts.GetWebIdentityTokenInput{
		Audience:         []string{audience},
		SigningAlgorithm: aws.String("ES384"),
		DurationSeconds:  aws.Int32(3600),
	})
	if err != nil {
		return fmt.Errorf("STS GetWebIdentityToken: %w", err)
	}
	if out.WebIdentityToken == nil || out.Expiration == nil {
		return errors.New("STS response missing WebIdentityToken or Expiration")
	}

	resp := successResponse{
		Version:        1,
		Success:        true,
		TokenType:      "urn:ietf:params:oauth:token-type:jwt",
		IDToken:        *out.WebIdentityToken,
		ExpirationTime: out.Expiration.Unix() - expirationBufferSec,
	}
	rendered, err := json.Marshal(resp)
	if err != nil {
		return fmt.Errorf("marshalling success response: %w", err)
	}
	fmt.Println(string(rendered))

	if cachePath := os.Getenv("GOOGLE_EXTERNAL_ACCOUNT_OUTPUT_FILE"); cachePath != "" {
		if werr := writeCache(cachePath, rendered); werr != nil {
			fmt.Fprintf(os.Stderr, "Warning: failed to write cache file: %v\n", werr)
		}
	}
	return nil
}

func writeCache(path string, content []byte) error {
	tmp := fmt.Sprintf("%s.tmp.%d", path, os.Getpid())
	if err := os.WriteFile(tmp, content, 0o644); err != nil {
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return nil
}
