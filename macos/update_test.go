package main

import "testing"

func TestCompareVersion(t *testing.T) {
	cases := []struct {
		left, right string
		want        int
	}{
		{"4.0.2", "v4.0.2", 0},
		{"4.0.2", "v4.0.3", -1},
		{"4.0.2", "v4.0.0", 1},
		{"4.1.0", "v4.0.9", 1},
	}
	for _, tc := range cases {
		got, err := compareVersion(tc.left, tc.right)
		if err != nil || got != tc.want {
			t.Fatalf("compareVersion(%q, %q) = %d, %v; want %d", tc.left, tc.right, got, err, tc.want)
		}
	}
	if _, err := compareVersion("4.0", "v4.0.2"); err == nil {
		t.Fatal("short version must be rejected")
	}
}

func TestSelectTechUpdateAssetsRequiresCanonicalName(t *testing.T) {
	release := githubRelease{
		TagName: "v4.0.4",
		Assets: []githubReleaseAsset{
			{Name: "ITM-v4.0.4-MacOS-AArch64-APP.zip", BrowserDownloadURL: "archive"},
			{Name: "ITM-v4.0.4-MacOS-AArch64-APP.zip.sig", BrowserDownloadURL: "signature"},
			{Name: "ITM-v4.0.4-Windows-x64-EXE.zip", BrowserDownloadURL: "wrong-platform"},
		},
	}
	archive, signature, err := selectTechUpdateAssets(release)
	if err != nil {
		t.Fatal(err)
	}
	if archive.BrowserDownloadURL != "archive" || signature.BrowserDownloadURL != "signature" {
		t.Fatalf("selected wrong assets: %#v %#v", archive, signature)
	}

	release.Assets = []githubReleaseAsset{
		{Name: "IMDb-Tech-Manager-macOS-v4.0.4-AppleSilicon-App.zip"},
		{Name: "IMDb-Tech-Manager-macOS-v4.0.4-AppleSilicon-App.zip.sig"},
	}
	if _, _, err := selectTechUpdateAssets(release); err == nil {
		t.Fatal("legacy or ambiguous archive names must not satisfy the OTA selector")
	}
}
