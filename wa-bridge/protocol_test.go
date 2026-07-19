package main

import (
	"testing"

	"go.mau.fi/whatsmeow/types"
)

func TestNewReceiptEventNormalizesWhatsAppReceiptTypes(t *testing.T) {
	delivered := NewReceiptEvent([]types.MessageID{"message-1"}, types.ReceiptTypeDelivered)
	if delivered.Status != "delivered" {
		t.Fatalf("expected delivered status, got %q", delivered.Status)
	}

	read := NewReceiptEvent([]types.MessageID{"message-1", "message-2"}, types.ReceiptTypeRead)
	if read.Status != "read" || len(read.MessageIDs) != 2 {
		t.Fatalf("expected two read message IDs, got %#v", read)
	}
}
