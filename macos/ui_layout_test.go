package main

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func useTemporaryUILayoutPath(t *testing.T) string {
	t.Helper()
	old := uiLayoutPathOverride
	path := filepath.Join(t.TempDir(), "ui-layout.json")
	uiLayoutPathOverride = path
	t.Cleanup(func() { uiLayoutPathOverride = old })
	return path
}

func TestUILayoutDefaultsAndAtomicRevisionWrites(t *testing.T) {
	path := useTemporaryUILayoutPath(t)

	raw, envelope, err := readUILayoutLocked()
	if err != nil {
		t.Fatalf("read default layout: %v", err)
	}
	if envelope.SchemaVersion != uiLayoutSchema || envelope.Revision != 0 {
		t.Fatalf("unexpected default envelope: %+v (%s)", envelope, raw)
	}

	first := []byte(`{"schema_version":1,"revision":1,"split_ratio":37.5}`)
	if err := saveUILayoutBytes(first); err != nil {
		t.Fatalf("save first layout: %v", err)
	}
	stored, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read stored layout: %v", err)
	}
	if string(stored) != string(first) {
		t.Fatalf("stored layout changed: %s", stored)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("stat layout: %v", err)
	}
	if info.Mode().Perm() != 0600 {
		t.Fatalf("layout permissions = %v", info.Mode().Perm())
	}
	if err := saveUILayoutBytes(first); err != nil {
		t.Fatalf("identical retry should be idempotent: %v", err)
	}
	if err := saveUILayoutBytes([]byte(`{"schema_version":1,"revision":1,"split_ratio":55}`)); !errors.Is(err, errStaleUILayout) {
		t.Fatalf("same revision with different payload should conflict, got %v", err)
	}
	if err := saveUILayoutBytes([]byte(`{"schema_version":1,"revision":2,"split_ratio":55}`)); err != nil {
		t.Fatalf("save newer layout: %v", err)
	}
	if err := saveUILayoutBytes(first); !errors.Is(err, errStaleUILayout) {
		t.Fatalf("older revision should conflict, got %v", err)
	}
}

func TestUILayoutValidationRejectsUnsafePayloads(t *testing.T) {
	useTemporaryUILayoutPath(t)
	cases := [][]byte{
		nil,
		[]byte(`not-json`),
		[]byte(`{"schema_version":2,"revision":1}`),
		[]byte(`{"schema_version":1,"revision":1}{}`),
		[]byte(strings.Repeat("x", uiLayoutMaxBytes+1)),
	}
	for _, payload := range cases {
		if _, err := parseUILayout(payload); err == nil {
			t.Fatalf("expected validation failure for payload length %d", len(payload))
		}
	}
	if err := saveUILayoutBytes([]byte(`{"schema_version":1,"revision":0}`)); err == nil {
		t.Fatal("revision zero must not be accepted as a client write")
	}
}

func TestUILayoutCorruptionFallsBackWithoutMutatingFile(t *testing.T) {
	path := useTemporaryUILayoutPath(t)
	if err := os.WriteFile(path, []byte(`{"schema_version":1`), 0600); err != nil {
		t.Fatal(err)
	}
	if _, _, err := readUILayoutLocked(); err == nil {
		t.Fatal("corrupt layout should be reported")
	}
	stored, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(stored) != `{"schema_version":1` {
		t.Fatalf("read path unexpectedly mutated corrupt state: %q", stored)
	}
}

func TestUILayoutEndpointRequiresSessionAndReturnsConflicts(t *testing.T) {
	useTemporaryUILayoutPath(t)
	oldToken := uiToken
	uiToken = "layout-test-token"
	t.Cleanup(func() { uiToken = oldToken })
	handler := requireToken(handleUILayout)

	unauthorized := httptest.NewRecorder()
	handler(unauthorized, httptest.NewRequest(http.MethodGet, "/api/ui-layout", nil))
	if unauthorized.Code != http.StatusForbidden {
		t.Fatalf("unauthorized status = %d", unauthorized.Code)
	}

	post := func(payload string) *httptest.ResponseRecorder {
		recorder := httptest.NewRecorder()
		request := httptest.NewRequest(http.MethodPost, "/api/ui-layout", strings.NewReader(payload))
		request.Header.Set("X-Manager-Token", uiToken)
		handler(recorder, request)
		return recorder
	}
	if recorder := post(`{"schema_version":1,"revision":1,"split_ratio":41}`); recorder.Code != http.StatusOK {
		t.Fatalf("first POST status = %d, body=%s", recorder.Code, recorder.Body.String())
	}
	if recorder := post(`{"schema_version":1,"revision":1,"split_ratio":42}`); recorder.Code != http.StatusConflict {
		t.Fatalf("conflicting POST status = %d, body=%s", recorder.Code, recorder.Body.String())
	}

	get := httptest.NewRecorder()
	request := httptest.NewRequest(http.MethodGet, "/api/ui-layout", nil)
	request.Header.Set("X-Manager-Token", uiToken)
	handler(get, request)
	if get.Code != http.StatusOK || !strings.Contains(get.Body.String(), `"split_ratio":41`) {
		t.Fatalf("GET status = %d, body=%s", get.Code, get.Body.String())
	}
}
