package main

import (
	"encoding/json"
	"testing"
	"time"

	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"
)

func TestConfirmedReactionPreservesIdentityAndRemovalWireContract(t *testing.T) {
	for _, emoji := range []string{"❤️", "👍", ""} {
		payload := &waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{
			Key: &waCommon.MessageKey{ID: proto.String("target")}, Text: proto.String(emoji),
			SenderTimestampMS: proto.Int64(1700000000123),
		}}
		event := confirmedReactionEvent(types.NewJID("family", types.GroupServer), types.NewADJID("447700900123", 0, 17), "sent", time.Unix(1700000000, 0), payload)
		if !event.Info.IsFromMe || !event.Info.IsGroup || event.Info.Sender.String() != "447700900123@s.whatsapp.net" || event.Info.ID != "sent" {
			t.Fatalf("incorrect confirmed event identity: %+v", event.Info)
		}
		content := (&Client{}).buildMessageContent(event.Message)
		if content.Emoji != emoji || content.TargetMessageID != "target" || content.SenderTimestampMS != 1700000000123 {
			t.Fatalf("lost reaction payload: %+v", content)
		}
		wire, err := json.Marshal(content)
		if err != nil {
			t.Fatal(err)
		}
		var decoded map[string]any
		if err = json.Unmarshal(wire, &decoded); err != nil {
			t.Fatal(err)
		}
		if _, exists := decoded["emoji"]; exists != (emoji != "") {
			t.Fatalf("unexpected removal contract: %s", wire)
		}
	}
}
