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

func TestReplaceMentionTokensUsesResolvedContactNames(t *testing.T) {
	mentions := []Mention{
		{JID: "33419157352505@lid", Phone: "447700900123", Name: "Simon"},
		{JID: "447700900456@s.whatsapp.net", Phone: "447700900456", Name: "David"},
	}

	got := replaceMentionTokens(
		"One for @33419157352505 and @447700900456. Not @334191573525050.",
		mentions,
	)
	want := "One for @Simon and @David. Not @334191573525050."
	if got != want {
		t.Fatalf("expected %q, got %q", want, got)
	}
}

func TestReplaceMentionTokensFallsBackToPhoneThenJIDUser(t *testing.T) {
	mentions := []Mention{
		{JID: "33419157352505@lid", Phone: "447700900123"},
		{JID: "998877@lid"},
	}

	got := replaceMentionTokens("Hi @33419157352505 and @998877", mentions)
	want := "Hi @447700900123 and @998877"
	if got != want {
		t.Fatalf("expected %q, got %q", want, got)
	}
}
