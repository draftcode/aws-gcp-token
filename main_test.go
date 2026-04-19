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
