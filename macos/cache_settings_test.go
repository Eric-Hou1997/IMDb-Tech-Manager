//go:build darwin

package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestIMDbCacheSettingsPreserveEngineConfig(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	if got := platformIMDbCacheMaxMB(); got != defaultIMDbCacheMaxMB {
		t.Fatalf("missing config cache limit = %d, want %d", got, defaultIMDbCacheMaxMB)
	}

	configPath := macConfigPath()
	if err := os.MkdirAll(filepath.Dir(configPath), 0700); err != nil {
		t.Fatal(err)
	}
	original := []byte(`{"roots":["/Volumes/Media"],"api_key":"keep-me","future_field":{"enabled":true}}`)
	if err := os.WriteFile(configPath, original, 0600); err != nil {
		t.Fatal(err)
	}
	if err := platformSetIMDbCacheMaxMB(4096); err != nil {
		t.Fatal(err)
	}
	if got := platformIMDbCacheMaxMB(); got != 4096 {
		t.Fatalf("saved cache limit = %d, want 4096", got)
	}

	var stored map[string]interface{}
	b, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(b, &stored); err != nil {
		t.Fatal(err)
	}
	if stored["api_key"] != "keep-me" || stored["future_field"] == nil || stored["roots"] == nil {
		t.Fatalf("cache setting update discarded unrelated engine config: %#v", stored)
	}
	if err := platformSetIMDbCacheMaxMB(minIMDbCacheMaxMB - 1); err == nil {
		t.Fatal("out-of-range cache limit unexpectedly accepted")
	}
	if got := platformIMDbCacheMaxMB(); got != 4096 {
		t.Fatalf("invalid update changed cache limit to %d", got)
	}
}

func TestIMDbCacheStatusMarksChangedLimitRequested(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	if err := platformSetIMDbCacheMaxMB(4096); err != nil {
		t.Fatal(err)
	}
	statusPath := macCacheStatusPath()
	status := IMDbCacheStatus{
		State: "ready", LimitMB: 2048, LimitBytes: 2048 * 1024 * 1024,
		UsedBytes: 12345, ParsedCount: 2, RawCount: 1, EntryCount: 3,
	}
	b, err := json.Marshal(status)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(statusPath, b, 0600); err != nil {
		t.Fatal(err)
	}

	got := platformIMDbCacheStatus()
	if got.State != "requested" {
		t.Fatalf("state = %q, want requested", got.State)
	}
	if got.LimitMB != 4096 || got.LimitBytes != 4096*1024*1024 {
		t.Fatalf("status did not expose current requested limit: %#v", got)
	}
	if got.UsedBytes != status.UsedBytes || got.EntryCount != status.EntryCount {
		t.Fatalf("status discarded last verified usage while maintenance is pending: %#v", got)
	}
}

func TestIMDbCacheStatusDoesNotHideCorruptEvidence(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	if err := os.MkdirAll(filepath.Dir(macCacheStatusPath()), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(macCacheStatusPath(), []byte("{broken"), 0600); err != nil {
		t.Fatal(err)
	}
	got := platformIMDbCacheStatus()
	if got.State != "failed" || got.Error == "" {
		t.Fatalf("corrupt status was not surfaced honestly: %#v", got)
	}
}

func TestIMDbCacheSettingFailsClosedOnCorruptEngineConfig(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	configPath := macConfigPath()
	if err := os.MkdirAll(filepath.Dir(configPath), 0700); err != nil {
		t.Fatal(err)
	}
	original := []byte("{broken")
	if err := os.WriteFile(configPath, original, 0600); err != nil {
		t.Fatal(err)
	}
	if err := platformSetIMDbCacheMaxMB(4096); err == nil {
		t.Fatal("corrupt engine config was silently overwritten")
	}
	got, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != string(original) {
		t.Fatalf("corrupt engine config changed despite failed update: %q", got)
	}
}
