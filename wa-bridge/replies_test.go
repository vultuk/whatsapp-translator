package main

import (
	"encoding/json"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"google.golang.org/protobuf/proto"
)

func TestIncomingReplyCarriesEmbeddedQuoteThroughLiveHistoryAndEditEvents(t *testing.T) {
	context := &waE2E.ContextInfo{StanzaID: proto.String("outside-history"), Participant: proto.String("447700900123@s.whatsapp.net"), QuotedMessage: &waE2E.Message{Conversation: proto.String("Yep I can")}}
	payloads := []*waE2E.Message{
		{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: proto.String("Thanks!"), ContextInfo: context}},
		{ImageMessage: &waE2E.ImageMessage{ContextInfo: context}},
		{VideoMessage: &waE2E.VideoMessage{ContextInfo: context}},
		{AudioMessage: &waE2E.AudioMessage{ContextInfo: context}},
		{DocumentMessage: &waE2E.DocumentMessage{ContextInfo: context}},
		{StickerMessage: &waE2E.StickerMessage{ContextInfo: context}},
		{LocationMessage: &waE2E.LocationMessage{ContextInfo: context}},
		{ContactMessage: &waE2E.ContactMessage{ContextInfo: context}},
		{PollCreationMessage: &waE2E.PollCreationMessage{ContextInfo: context}},
	}
	var client *Client
	for _, payload := range payloads {
		quote := client.incomingReplyContext(payload)
		if quote == nil || quote.MessageID != "outside-history" || quote.Text != "Yep I can" || quote.SenderName != "+447700900123" {
			t.Fatalf("lost quote: %#v", quote)
		}
		for _, history := range []bool{false, true} {
			message := Message{ID: "reply", IsHistory: history, ReplyContext: quote}
			for _, event := range []map[string]interface{}{NewMessageEvent(message), NewMessageEditEvent(message, 200)} {
				raw, err := json.Marshal(event)
				if err != nil {
					t.Fatal(err)
				}
				var decoded map[string]interface{}
				if err = json.Unmarshal(raw, &decoded); err != nil {
					t.Fatal(err)
				}
				context := decoded["reply_context"].(map[string]interface{})
				if context["messageId"] != "outside-history" || context["text"] != "Yep I can" {
					t.Fatalf("bad wire quote: %s", raw)
				}
			}
		}
	}
}

func TestIncomingQuoteHandlesMediaMissingOriginalAndBoundedWrappers(t *testing.T) {
	cases := []struct {
		message *waE2E.Message
		want    string
	}{
		{nil, "Original message unavailable"},
		{&waE2E.Message{ImageMessage: &waE2E.ImageMessage{Caption: proto.String("Lunch")}}, "Photo: Lunch"},
		{&waE2E.Message{AudioMessage: &waE2E.AudioMessage{PTT: proto.Bool(true)}}, "Voice message"},
		{&waE2E.Message{DocumentMessage: &waE2E.DocumentMessage{FileName: proto.String("Menu.pdf")}}, "Document: Menu.pdf"},
		{&waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: proto.String("Earlier reply"), ContextInfo: &waE2E.ContextInfo{QuotedMessage: &waE2E.Message{Conversation: proto.String("Nested secret")}}}}, "Earlier reply"},
	}
	for _, test := range cases {
		if got := quotedPreview(test.message); got != test.want {
			t.Errorf("got %q, want %q", got, test.want)
		}
	}
	wrapped := &waE2E.Message{Conversation: proto.String("Hello")}
	wrapped = &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: wrapped}}
	if quotedPreview(wrapped) != "Hello" {
		t.Fatal("lost wrapped text")
	}
	for i := 0; i < 10; i++ {
		wrapped = &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: wrapped}}
	}
	if quotedPreview(wrapped) != "Original message unavailable" {
		t.Fatal("unbounded quote wrappers")
	}
	if len([]rune(quotedPreview(&waE2E.Message{Conversation: proto.String(strings.Repeat("é", 4000))}))) != 2001 {
		t.Fatal("quote not bounded on rune boundary")
	}
	if (*Client)(nil).incomingReplyContext(&waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{ContextInfo: &waE2E.ContextInfo{IsForwarded: proto.Bool(true)}}}) != nil {
		t.Fatal("forwarding alone is not a reply")
	}
}
