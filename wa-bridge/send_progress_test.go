package main

import (
	"context"
	"sync"
	"testing"
	"time"
)

func TestNewSendProgressEventIncludesAlbumProgress(t *testing.T) {
	event := NewSendProgressEvent(42, "job-7", "uploading", 3, 10)
	if event.Type != "send_progress" || event.RequestID != 42 || event.ProgressID != "job-7" {
		t.Fatalf("unexpected identity: %#v", event)
	}
	if event.Stage != "uploading" || event.Completed != 3 || event.Total != 10 {
		t.Fatalf("unexpected progress: %#v", event)
	}
}

func TestMapAlbumUploadsPreservesOrderAndBoundsConcurrency(t *testing.T) {
	var mu sync.Mutex
	active := 0
	maximumActive := 0
	completed := 0

	values, err := mapAlbumUploads(context.Background(), []int{0, 1, 2, 3, 4, 5}, 3, func(_ context.Context, value int) (int, error) {
		mu.Lock()
		active++
		if active > maximumActive {
			maximumActive = active
		}
		mu.Unlock()

		time.Sleep(15 * time.Millisecond)

		mu.Lock()
		active--
		completed++
		mu.Unlock()
		return value * 10, nil
	}, func(done int) {
		mu.Lock()
		defer mu.Unlock()
		if done > completed {
			t.Fatalf("progress advanced before upload completion: done=%d completed=%d", done, completed)
		}
	})
	if err != nil {
		t.Fatalf("mapAlbumUploads returned an error: %v", err)
	}
	if maximumActive != 3 {
		t.Fatalf("expected three concurrent uploads, got %d", maximumActive)
	}
	for index, value := range values {
		if value != index*10 {
			t.Fatalf("result order changed at %d: got %d", index, value)
		}
	}
}

func TestCommandBufferFitsMaximumAlbumPayload(t *testing.T) {
	const maximumDecodedAlbumBytes = 64 * 1024 * 1024
	maximumBase64Bytes := (maximumDecodedAlbumBytes + 2) / 3 * 4
	if maxCommandBytes < maximumBase64Bytes+1024*1024 {
		t.Fatalf("command buffer %d cannot hold maximum album JSON payload %d", maxCommandBytes, maximumBase64Bytes)
	}
}
