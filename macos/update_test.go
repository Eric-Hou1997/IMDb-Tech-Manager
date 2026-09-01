package main

import "testing"

func TestCompareVersion(t *testing.T) {
	cases := []struct {
		left, right string
		want        int
	}{
		{"4.0.0", "v4.0.0", 0},
		{"4.0.0", "v4.0.1", -1},
		{"4.1.0", "v4.0.9", 1},
	}
	for _, tc := range cases {
		got, err := compareVersion(tc.left, tc.right)
		if err != nil || got != tc.want {
			t.Fatalf("compareVersion(%q, %q) = %d, %v; want %d", tc.left, tc.right, got, err, tc.want)
		}
	}
	if _, err := compareVersion("4.0", "v4.0.0"); err == nil {
		t.Fatal("short version must be rejected")
	}
}
