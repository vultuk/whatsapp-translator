package main

import (
	"testing"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"
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

func TestBuildAlbumMessagesAssociatesChildrenWithParent(t *testing.T) {
	parent, parentKey, err := buildAlbumMessage("447700900123@s.whatsapp.net", 3, "reply-1", "447700900456", "Quoted")
	if err != nil {
		t.Fatalf("build album message: %v", err)
	}
	if got := parent.GetAlbumMessage().GetExpectedImageCount(); got != 3 {
		t.Fatalf("expected 3 images, got %d", got)
	}
	if got := parent.GetAlbumMessage().GetContextInfo().GetStanzaID(); got != "reply-1" {
		t.Fatalf("expected reply context on album parent, got %q", got)
	}

	child := buildAlbumChildMessage(&waE2E.ImageMessage{}, parentKey, 2)
	association := child.GetMessageContextInfo().GetMessageAssociation()
	if association.GetAssociationType() != waE2E.MessageAssociation_MEDIA_ALBUM {
		t.Fatalf("expected media album association, got %v", association.GetAssociationType())
	}
	if got := association.GetParentMessageKey().GetID(); got != parentKey.GetID() {
		t.Fatalf("expected parent ID %q, got %q", parentKey.GetID(), got)
	}
	if got := association.GetMessageIndex(); got != 2 {
		t.Fatalf("expected child index 2, got %d", got)
	}
	if len(proto.Clone(child).(*waE2E.Message).GetMessageContextInfo().GetMessageSecret()) != 32 {
		t.Fatal("expected a 32-byte child message secret")
	}
}
