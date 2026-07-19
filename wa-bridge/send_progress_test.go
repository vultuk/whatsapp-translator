package main

import "testing"

func TestNewSendProgressEventIncludesAlbumProgress(t *testing.T) {
	event := NewSendProgressEvent(42, "job-7", "uploading", 3, 10)
	if event.Type != "send_progress" || event.RequestID != 42 || event.ProgressID != "job-7" {
		t.Fatalf("unexpected identity: %#v", event)
	}
	if event.Stage != "uploading" || event.Completed != 3 || event.Total != 10 {
		t.Fatalf("unexpected progress: %#v", event)
	}
}
