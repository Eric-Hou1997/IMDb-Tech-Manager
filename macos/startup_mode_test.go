package main

import "testing"

func TestResolvedAutoModeOnAppStartSeparatesLegacyAndApplicationPreferences(t *testing.T) {
	tests := []struct {
		name   string
		set    Settings
		legacy bool
		want   bool
	}{
		{"new preference on", Settings{AutoModeOnAppStart: true, AutoModeOnAppStartConfigured: true}, false, true},
		{"new preference off overrides legacy on", Settings{AutoModeOnAppStartConfigured: true, AutoStart: true, AutoStartConfigured: true}, true, false},
		{"legacy configured on migrates to on", Settings{AutoStart: true, AutoStartConfigured: true}, false, true},
		{"legacy configured off migrates to off", Settings{AutoStartConfigured: true}, true, false},
		{"unconfigured legacy launch item is preserved", Settings{}, true, true},
		{"fresh install defaults off", Settings{}, false, false},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if got := resolvedAutoModeOnAppStart(test.set, test.legacy); got != test.want {
				t.Fatalf("resolvedAutoModeOnAppStart() = %v, want %v", got, test.want)
			}
		})
	}
}
