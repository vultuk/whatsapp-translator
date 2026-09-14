package main

import (
	"context"
	"encoding/json"
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"go.mau.fi/whatsmeow/util/gcmutil"
	"go.mau.fi/whatsmeow/util/hkdfutil"
	"google.golang.org/protobuf/proto"
	"testing"
	"time"
)

type editFixtureSecrets struct {
	store.MsgSecretStore
	key    []byte
	sender types.JID
}

func (s *editFixtureSecrets) GetMessageSecret(context.Context, types.JID, types.JID, types.MessageID) ([]byte, types.JID, error) {
	return s.key, s.sender, nil
}

func TestSecretEncryptedEditUsesWhatsMeowDecryptionAndOriginalTarget(t *testing.T) {
	sender := types.NewJID("447700900123", types.DefaultUserServer)
	secret := &editFixtureSecrets{key: make([]byte, 32), sender: sender}
	c := &Client{ctx: context.Background(), client: &whatsmeow.Client{Store: &store.Device{MsgSecrets: secret}}}
	key := hkdfutil.SHA256(secret.key, nil, []byte("original"+sender.String()+sender.String()+string(whatsmeow.EncSecretMessageEdit)), 32)
	plaintext, err := proto.Marshal(&waE2E.Message{Conversation: proto.String("Grr")})
	if err != nil {
		t.Fatal(err)
	}
	iv := make([]byte, 12)
	ciphertext, err := gcmutil.Encrypt(key, iv, plaintext, nil)
	if err != nil {
		t.Fatal(err)
	}
	evt := &events.Message{Info: types.MessageInfo{ID: "envelope", Timestamp: time.Unix(1700000000, 0), MessageSource: types.MessageSource{Sender: sender, Chat: types.NewJID("family", types.GroupServer)}}, Message: &waE2E.Message{SecretEncryptedMessage: &waE2E.SecretEncryptedMessage{TargetMessageKey: &waCommon.MessageKey{ID: proto.String("original"), FromMe: proto.Bool(true)}, SecretEncType: waE2E.SecretEncryptedMessage_MESSAGE_EDIT.Enum(), EncIV: iv, EncPayload: ciphertext}}}
	id, clock, content, ok := c.extractMessageEdit(evt)
	if !ok || id != "original" || clock != 1700000000000 || content.GetConversation() != "Grr" {
		t.Fatalf("encrypted edit lost: %s %d %v", id, clock, ok)
	}
	secret.key = nil
	if _, _, _, ok := c.extractMessageEdit(evt); ok {
		t.Fatal("missing decryption key must not create a false message")
	}
}

func TestEditEnvelopeKeepsTargetAndClockForLiveAndHistory(t *testing.T) {
	correction := &waE2E.Message{Conversation: proto.String("Grr")}
	protocol := &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{
		Type:        waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(),
		Key:         &waCommon.MessageKey{ID: proto.String("original-message")},
		TimestampMS: proto.Int64(1700000000123), EditedMessage: correction,
	}}
	raw := &waE2E.Message{EditedMessage: &waE2E.FutureProofMessage{Message: protocol}}
	for _, content := range []*waE2E.Message{protocol, correction} {
		evt := &events.Message{Info: types.MessageInfo{ID: "edit-envelope", Timestamp: time.Unix(1700000000, 0)}, Message: content, RawMessage: raw}
		id, clock, text, ok := messageEdit(evt)
		if !ok || id != "original-message" || clock != 1700000000123 || text.GetConversation() != "Grr" {
			t.Fatalf("edit metadata lost: %s %d %v", id, clock, ok)
		}
		msg := Message{ID: id, Content: (&Client{}).buildMessageContent(text)}
		encoded, err := json.Marshal(NewMessageEditEvent(msg, clock))
		if err != nil {
			t.Fatal(err)
		}
		var event map[string]interface{}
		if err := json.Unmarshal(encoded, &event); err != nil {
			t.Fatal(err)
		}
		if event["type"] != "message_edit" || event["id"] != "original-message" || event["edited_at_ms"] != float64(clock) {
			t.Fatalf("wrong wire event: %s", encoded)
		}
	}
}

func TestEditRejectsMissingTargetAndLeavesNormalMessagesAlone(t *testing.T) {
	for _, message := range []*waE2E.Message{nil, {Conversation: proto.String("Get")}, {ProtocolMessage: &waE2E.ProtocolMessage{Type: waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(), EditedMessage: &waE2E.Message{Conversation: proto.String("Grr")}}}} {
		if _, _, _, ok := messageEdit(&events.Message{Message: message}); ok {
			t.Fatal("ordinary or malformed message became an edit")
		}
	}
}

func TestCaptionOnlyEditRetainsRequiredWireFields(t *testing.T) {
	for _, kind := range []string{"image", "video", "document"} {
		data, err := json.Marshal(NewMessageEditEvent(Message{ID: "photo", Content: MessageContent{Type: kind, Caption: "Grr"}}, 123))
		if err != nil {
			t.Fatal(err)
		}
		var event map[string]interface{}
		if err := json.Unmarshal(data, &event); err != nil {
			t.Fatal(err)
		}
		content := event["content"].(map[string]interface{})
		if content["mime_type"] != "" || content["file_size"] != float64(0) || content["caption"] != "Grr" {
			t.Fatalf("caption contract missing fields: %s", data)
		}
	}
}
