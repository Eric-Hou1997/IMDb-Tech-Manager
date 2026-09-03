package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"sync"
)

const defaultLanguage = "zh-CN"

type LanguageOption struct {
	Code           string `json:"code"`
	NativeName     string `json:"native_name"`
	EnglishName    string `json:"english_name"`
	ReviewLanguage string `json:"review_language"`
}

var languageOptions = []LanguageOption{
	{Code: "zh-CN", NativeName: "简体中文", EnglishName: "Simplified Chinese", ReviewLanguage: "zh-CN"},
	{Code: "en-US", NativeName: "English (United States)", EnglishName: "English (United States)", ReviewLanguage: "en-US"},
}

var languageByCode = func() map[string]LanguageOption {
	result := make(map[string]LanguageOption, len(languageOptions))
	for _, option := range languageOptions {
		result[option.Code] = option
	}
	return result
}()

var readPlatformOutputLanguage = platformReadOutputLanguage
var writePlatformOutputLanguage = platformSetOutputLanguage

func supportedLanguage(language string) bool {
	_, ok := languageByCode[language]
	return ok
}

func normalizedLanguage(language string) string {
	if supportedLanguage(language) {
		return language
	}
	return defaultLanguage
}

func supportedLanguages() []LanguageOption {
	result := make([]LanguageOption, len(languageOptions))
	copy(result, languageOptions)
	return result
}

type LanguageSyncStatus struct {
	State    string `json:"state"`
	Language string `json:"language"`
	Source   string `json:"source,omitempty"`
	Migrated bool   `json:"migrated,omitempty"`
	Error    string `json:"error,omitempty"`
}

var languageSyncState = struct {
	sync.RWMutex
	Status LanguageSyncStatus
}{Status: LanguageSyncStatus{State: "unverified", Language: defaultLanguage}}

func recordLanguageSync(status LanguageSyncStatus) {
	status.Language = normalizedLanguage(status.Language)
	languageSyncState.Lock()
	languageSyncState.Status = status
	languageSyncState.Unlock()
}

func currentLanguageSyncStatus() LanguageSyncStatus {
	languageSyncState.RLock()
	defer languageSyncState.RUnlock()
	return languageSyncState.Status
}

func settingsForLanguageMigration() (Settings, bool, error) {
	set := Settings{IntervalSeconds: 60, Language: defaultLanguage}
	b, err := os.ReadFile(settingsPath())
	if errors.Is(err, os.ErrNotExist) {
		return set, false, nil
	}
	if err != nil {
		return set, false, err
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(b, &raw); err != nil || raw == nil {
		if err == nil {
			err = errors.New("settings root is not an object")
		}
		return set, false, fmt.Errorf("读取应用语言设置失败：%w", err)
	}
	if err := json.Unmarshal(b, &set); err != nil {
		return set, false, fmt.Errorf("读取应用语言设置失败：%w", err)
	}
	if set.IntervalSeconds < 30 {
		set.IntervalSeconds = 60
	}
	var storedLanguage string
	if value, ok := raw["language"]; ok {
		_ = json.Unmarshal(value, &storedLanguage)
	}
	configured := supportedLanguage(storedLanguage)
	set.Language = normalizedLanguage(storedLanguage)
	return set, configured, nil
}

// migrateLanguagePreference reconciles only the two language fields used by
// the Manager and Engine. It deliberately leaves caches, task state, UI
// layout, ownership data, browser profiles, Keychain items, and NFOs alone.
func migrateLanguagePreference() error {
	set, managerConfigured, err := settingsForLanguageMigration()
	if err != nil {
		recordLanguageSync(LanguageSyncStatus{State: "failed", Language: defaultLanguage, Error: err.Error()})
		return err
	}
	engineLanguage, engineConfigured, err := readPlatformOutputLanguage()
	if err != nil {
		recordLanguageSync(LanguageSyncStatus{State: "failed", Language: set.Language, Error: err.Error()})
		return err
	}

	status := LanguageSyncStatus{State: "ready", Language: defaultLanguage, Source: "default"}
	switch {
	case managerConfigured:
		status.Language = set.Language
		status.Source = "manager-settings"
		if !engineConfigured || engineLanguage != set.Language {
			if err := writePlatformOutputLanguage(set.Language); err != nil {
				status.State, status.Error = "failed", err.Error()
				recordLanguageSync(status)
				return err
			}
			status.Migrated = true
		}
	case engineConfigured:
		set.Language = engineLanguage
		status.Language = engineLanguage
		status.Source = "engine-config"
		if err := saveSettings(set); err != nil {
			status.State, status.Error = "failed", err.Error()
			recordLanguageSync(status)
			return err
		}
		status.Migrated = true
	default:
		set.Language = defaultLanguage
		if err := saveSettings(set); err != nil {
			status.State, status.Error = "failed", err.Error()
			recordLanguageSync(status)
			return err
		}
		status.Migrated = true
	}
	recordLanguageSync(status)
	return nil
}

// saveLanguagePreference makes the Manager setting authoritative. The Engine
// is inspected before any write, and a failed Engine write rolls the Manager
// setting back so a rejected UI request cannot leave a silent split-brain.
func saveLanguagePreference(previous, next Settings, language string) error {
	if !supportedLanguage(language) {
		return fmt.Errorf("unsupported language: %s", language)
	}
	if _, _, err := readPlatformOutputLanguage(); err != nil {
		return err
	}
	next.Language = language
	if err := saveSettings(next); err != nil {
		return err
	}
	if err := writePlatformOutputLanguage(language); err != nil {
		rollbackErr := saveSettings(previous)
		if rollbackErr != nil {
			combined := fmt.Errorf("%v；应用语言回滚失败：%w", err, rollbackErr)
			recordLanguageSync(LanguageSyncStatus{State: "failed", Language: language, Error: combined.Error()})
			return combined
		}
		recordLanguageSync(LanguageSyncStatus{State: "failed", Language: previous.Language, Error: err.Error()})
		return err
	}
	recordLanguageSync(LanguageSyncStatus{State: "ready", Language: language, Source: "manager-settings"})
	return nil
}
