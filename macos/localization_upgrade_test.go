//go:build darwin

package main

import (
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeTestFile(t *testing.T, path string, data []byte, mode os.FileMode) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, data, mode); err != nil {
		t.Fatal(err)
	}
}

func readJSONObjectForTest(t *testing.T, path string) map[string]interface{} {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var value map[string]interface{}
	if err := json.Unmarshal(b, &value); err != nil {
		t.Fatal(err)
	}
	return value
}

func TestLanguageRegistryAndFallbackAreStable(t *testing.T) {
	if normalizedLanguage("en-US") != "en-US" || normalizedLanguage("zh-CN") != "zh-CN" {
		t.Fatal("registered languages were not preserved")
	}
	if normalizedLanguage("fr-FR") != defaultLanguage || normalizedLanguage("") != defaultLanguage {
		t.Fatal("unknown languages must fall back to Simplified Chinese")
	}
	options := supportedLanguages()
	if len(options) != 2 || options[0].Code != "zh-CN" || options[1].Code != "en-US" {
		t.Fatalf("unexpected language registry: %#v", options)
	}
	options[0].Code = "changed"
	if supportedLanguages()[0].Code != "zh-CN" {
		t.Fatal("callers must not be able to mutate the language registry")
	}
}

func TestUpgradeImportsLegacyEngineLanguageWithoutTouchingCaches(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	writeTestFile(t, settingsPath(), []byte(`{"interval_seconds":120,"future_setting":{"keep":true}}`), 0644)
	writeTestFile(t, macConfigPath(), []byte(`{"output_language":"en-US","library_roots":{"movies":["/Volumes/Movies"],"tv":[]},"future_engine":{"keep":true}}`), 0600)

	sentinels := map[string][]byte{
		filepath.Join(platformDataPath(), "cache", "raw-tt0000001.html.gz"): []byte("raw-imdb-cache"),
		filepath.Join(platformDataPath(), "ai-cache", "result.json"):        []byte(`{"cache_schema":2,"result":{"tags":[]}}`),
		filepath.Join(platformDataPath(), "index-cache.json"):               []byte(`{"schema":3,"items":{"keep":true}}`),
		filepath.Join(platformDataPath(), "ownership", "record.json"):       []byte(`{"entries":[{"value":"Keep"}]}`),
		filepath.Join(baseDir(), "task-history.json"):                       []byte(`[{"message":"旧任务"}]`),
		filepath.Join(baseDir(), "ui-layout.json"):                          []byte(`{"schema_version":1,"revision":7,"split_ratio":41}`),
	}
	for path, data := range sentinels {
		writeTestFile(t, path, data, 0600)
	}

	if err := migrateLanguagePreference(); err != nil {
		t.Fatal(err)
	}
	settings := readJSONObjectForTest(t, settingsPath())
	if settings["language"] != "en-US" || settings["future_setting"] == nil {
		t.Fatalf("legacy Manager settings were not migrated safely: %#v", settings)
	}
	engine := readJSONObjectForTest(t, macConfigPath())
	if engine["output_language"] != "en-US" || engine["library_roots"] == nil || engine["future_engine"] == nil {
		t.Fatalf("Engine config changed unexpectedly: %#v", engine)
	}
	for path, want := range sentinels {
		got, err := os.ReadFile(path)
		if err != nil || string(got) != string(want) {
			t.Fatalf("upgrade touched unrelated data %s: %q, %v", path, got, err)
		}
	}
	status := currentLanguageSyncStatus()
	if status.State != "ready" || status.Language != "en-US" || status.Source != "engine-config" || !status.Migrated {
		t.Fatalf("unexpected migration status: %#v", status)
	}
	settingsAfterFirst, _ := os.ReadFile(settingsPath())
	engineAfterFirst, _ := os.ReadFile(macConfigPath())
	if err := migrateLanguagePreference(); err != nil {
		t.Fatal(err)
	}
	settingsAfterSecond, _ := os.ReadFile(settingsPath())
	engineAfterSecond, _ := os.ReadFile(macConfigPath())
	if string(settingsAfterSecond) != string(settingsAfterFirst) || string(engineAfterSecond) != string(engineAfterFirst) {
		t.Fatal("repeated migration was not idempotent")
	}
	if currentLanguageSyncStatus().Migrated {
		t.Fatal("idempotent migration incorrectly reported another write")
	}
}

func TestManagerLanguageWinsAndPreservesEngineConfig(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	writeTestFile(t, settingsPath(), []byte(`{"interval_seconds":60,"language":"en-US","future_setting":9}`), 0644)
	writeTestFile(t, macConfigPath(), []byte(`{"output_language":"zh-CN","api_protocol":"openai","future_engine":9}`), 0600)
	if err := migrateLanguagePreference(); err != nil {
		t.Fatal(err)
	}
	engine := readJSONObjectForTest(t, macConfigPath())
	if engine["output_language"] != "en-US" || engine["api_protocol"] != "openai" || engine["future_engine"] != float64(9) {
		t.Fatalf("Manager language did not win safely: %#v", engine)
	}
	settings := readJSONObjectForTest(t, settingsPath())
	if settings["future_setting"] != float64(9) {
		t.Fatalf("unknown Manager setting was discarded: %#v", settings)
	}
}

func TestFreshInstallDefaultsWithoutCreatingEngineConfig(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	if err := migrateLanguagePreference(); err != nil {
		t.Fatal(err)
	}
	if loadSettings().Language != defaultLanguage {
		t.Fatal("fresh install did not default to Simplified Chinese")
	}
	if _, err := os.Stat(macConfigPath()); !os.IsNotExist(err) {
		t.Fatalf("language migration unexpectedly created Engine config: %v", err)
	}
}

func TestCorruptUpgradeInputsFailClosed(t *testing.T) {
	t.Run("manager settings", func(t *testing.T) {
		t.Setenv("HOME", t.TempDir())
		settings := []byte("{broken")
		engine := []byte(`{"output_language":"en-US","keep":true}`)
		writeTestFile(t, settingsPath(), settings, 0644)
		writeTestFile(t, macConfigPath(), engine, 0600)
		if err := migrateLanguagePreference(); err == nil {
			t.Fatal("corrupt Manager settings were silently overwritten")
		}
		if got, _ := os.ReadFile(settingsPath()); string(got) != string(settings) {
			t.Fatalf("corrupt Manager settings changed: %q", got)
		}
		if got, _ := os.ReadFile(macConfigPath()); string(got) != string(engine) {
			t.Fatalf("Engine config changed after failed migration: %q", got)
		}
	})

	t.Run("engine config", func(t *testing.T) {
		t.Setenv("HOME", t.TempDir())
		settings := []byte(`{"interval_seconds":60,"language":"en-US"}`)
		engine := []byte("{broken")
		writeTestFile(t, settingsPath(), settings, 0644)
		writeTestFile(t, macConfigPath(), engine, 0600)
		if err := migrateLanguagePreference(); err == nil {
			t.Fatal("corrupt Engine config was silently overwritten")
		}
		if got, _ := os.ReadFile(settingsPath()); string(got) != string(settings) {
			t.Fatalf("Manager settings changed after failed migration: %q", got)
		}
		if got, _ := os.ReadFile(macConfigPath()); string(got) != string(engine) {
			t.Fatalf("corrupt Engine config changed: %q", got)
		}
	})
}

func TestRejectedLanguageChangeLeavesBothFilesUnchanged(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	settings := []byte(`{"interval_seconds":60,"language":"zh-CN","future_setting":true}`)
	engine := []byte("{broken")
	writeTestFile(t, settingsPath(), settings, 0644)
	writeTestFile(t, macConfigPath(), engine, 0600)
	previous := loadSettings()
	next := previous
	next.Language = "en-US"
	if err := saveLanguagePreference(previous, next, "en-US"); err == nil {
		t.Fatal("language change unexpectedly accepted a corrupt Engine config")
	}
	if got, _ := os.ReadFile(settingsPath()); string(got) != string(settings) {
		t.Fatalf("rejected language change touched Manager settings: %q", got)
	}
	if got, _ := os.ReadFile(macConfigPath()); string(got) != string(engine) {
		t.Fatalf("rejected language change touched Engine config: %q", got)
	}
}

func TestEngineWriteFailureRollsManagerLanguageBack(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	if err := saveSettings(Settings{IntervalSeconds: 60, Language: "zh-CN"}); err != nil {
		t.Fatal(err)
	}
	writeTestFile(t, macConfigPath(), []byte(`{"output_language":"zh-CN","keep":true}`), 0600)
	beforeSettings, _ := os.ReadFile(settingsPath())
	beforeEngine, _ := os.ReadFile(macConfigPath())
	oldWriter := writePlatformOutputLanguage
	writePlatformOutputLanguage = func(string) error { return errors.New("injected Engine write failure") }
	t.Cleanup(func() { writePlatformOutputLanguage = oldWriter })
	previous := loadSettings()
	next := previous
	next.Language = "en-US"
	if err := saveLanguagePreference(previous, next, "en-US"); err == nil {
		t.Fatal("injected Engine write failure was ignored")
	}
	if got, _ := os.ReadFile(settingsPath()); string(got) != string(beforeSettings) {
		t.Fatalf("Manager language was not rolled back: %q", got)
	}
	if got, _ := os.ReadFile(macConfigPath()); string(got) != string(beforeEngine) {
		t.Fatalf("Engine config changed after injected failure: %q", got)
	}
}

func TestLanguageSettingsEndpointUpdatesBothStores(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	if err := saveSettings(Settings{IntervalSeconds: 60, Language: "zh-CN"}); err != nil {
		t.Fatal(err)
	}
	writeTestFile(t, macConfigPath(), []byte(`{"output_language":"zh-CN","future_engine":true}`), 0600)
	recorder := httptest.NewRecorder()
	request := httptest.NewRequest(http.MethodPost, "/api/settings", strings.NewReader(`{"language":"en-US"}`))
	handleSettings(recorder, request)
	if recorder.Code != http.StatusOK {
		t.Fatalf("language update status = %d, body=%s", recorder.Code, recorder.Body.String())
	}
	if loadSettings().Language != "en-US" {
		t.Fatal("Manager language did not update")
	}
	engine := readJSONObjectForTest(t, macConfigPath())
	if engine["output_language"] != "en-US" || engine["future_engine"] != true {
		t.Fatalf("Engine language update discarded existing data: %#v", engine)
	}
	status := currentLanguageSyncStatus()
	if status.State != "ready" || status.Language != "en-US" {
		t.Fatalf("language sync status not ready: %#v", status)
	}

	beforeSettings, _ := os.ReadFile(settingsPath())
	beforeEngine, _ := os.ReadFile(macConfigPath())
	recorder = httptest.NewRecorder()
	request = httptest.NewRequest(http.MethodPost, "/api/settings", strings.NewReader(`{"language":"fr-FR"}`))
	handleSettings(recorder, request)
	if recorder.Code != http.StatusBadRequest {
		t.Fatalf("unsupported language status = %d, body=%s", recorder.Code, recorder.Body.String())
	}
	if got, _ := os.ReadFile(settingsPath()); string(got) != string(beforeSettings) {
		t.Fatal("unsupported language touched Manager settings")
	}
	if got, _ := os.ReadFile(macConfigPath()); string(got) != string(beforeEngine) {
		t.Fatal("unsupported language touched Engine config")
	}
}

func TestJobCapturesLanguageAtStart(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	if err := saveSettings(Settings{IntervalSeconds: 60, Language: "en-US"}); err != nil {
		t.Fatal(err)
	}
	var manager jobManager
	job, err := manager.begin("ai-generate-selected")
	if err != nil {
		t.Fatal(err)
	}
	if job.Language != "en-US" || job.MessageCode != "job.running" {
		t.Fatalf("job did not capture its presentation language: %#v", job)
	}
	job = manager.complete(job.ID, job.Action, 0, "完成", "log")
	if job.Language != "en-US" || job.MessageCode != "job.completed" {
		t.Fatalf("completed job lost its presentation metadata: %#v", job)
	}
}

func TestLibraryStatusAddsStableSpaceWithoutRemovingLegacyKind(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	writeTestFile(t, macConfigPath(), []byte(`{
  "library_roots":{"movies":["/Volumes/Movies"],"tv":["/Volumes/TV"]},
  "roots":["/Volumes/Movies","/Volumes/Legacy"]
}`), 0600)
	status, err := collectStatus()
	if err != nil {
		t.Fatal(err)
	}
	got := map[string]LibraryInfo{}
	for _, library := range status.Libraries {
		got[library.Path] = library
	}
	wants := map[string]struct{ space, kind string }{
		"/Volumes/Movies": {"movies", "电影"},
		"/Volumes/TV":     {"tv", "电视剧"},
		"/Volumes/Legacy": {"unassigned", "待分类"},
	}
	for path, want := range wants {
		library, ok := got[path]
		if !ok || library.Space != want.space || library.Kind != want.kind {
			t.Fatalf("library %s = %#v, want space=%s kind=%s", path, library, want.space, want.kind)
		}
	}
}
