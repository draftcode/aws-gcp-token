// SPDX-License-Identifier: Apache-2.0

package main

import (
	"os"
	"path/filepath"
	"testing"
)

func TestWriteCacheAtomic(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "cache.json")

	if err := writeCache(path, []byte(`{"version":1}`)); err != nil {
		t.Fatalf("writeCache: %v", err)
	}

	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(got) != `{"version":1}` {
		t.Errorf("unexpected content: %q", got)
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatalf("ReadDir: %v", err)
	}
	for _, e := range entries {
		if e.Name() != "cache.json" {
			t.Errorf("unexpected leftover file: %s", e.Name())
		}
	}
}

func TestDurationFromEnv(t *testing.T) {
	tests := []struct {
		name    string
		value   string
		set     bool
		want    int32
		wantErr bool
	}{
		{name: "unset", want: 3600},
		{name: "empty", value: "", set: true, want: 3600},
		{name: "custom", value: "900", set: true, want: 900},
		{name: "min", value: "60", set: true, want: 60},
		{name: "max", value: "3600", set: true, want: 3600},
		{name: "below min", value: "59", set: true, wantErr: true},
		{name: "above max", value: "3601", set: true, wantErr: true},
		{name: "not a number", value: "abc", set: true, wantErr: true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.set {
				t.Setenv(durationEnv, tt.value)
			} else {
				t.Setenv(durationEnv, "")
			}
			got, err := durationFromEnv()
			if (err != nil) != tt.wantErr {
				t.Fatalf("err = %v, wantErr %v", err, tt.wantErr)
			}
			if got != tt.want {
				t.Errorf("got %d, want %d", got, tt.want)
			}
		})
	}
}
