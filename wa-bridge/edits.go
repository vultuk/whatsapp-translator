package main

import (
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types/events"
)

func (c *Client) extractMessageEdit(evt *events.Message) (string, int64, *waE2E.Message, bool) {
	secret := evt.Message.GetSecretEncryptedMessage()
	if secret.GetSecretEncType() != waE2E.SecretEncryptedMessage_MESSAGE_EDIT {
		return messageEdit(evt)
	}
	if secret.GetTargetMessageKey().GetID() == "" {
		return "", 0, nil, false
	}
	decoded, err := c.client.DecryptSecretEncryptedMessage(c.ctx, evt)
	if err != nil {
		SendEvent(NewLogEvent("warn", "Could not decrypt an incoming message edit"))
		return "", 0, nil, false
	}
	unwrapped := &events.Message{Info: evt.Info, RawMessage: decoded}
	unwrapped.UnwrapRaw()
	if id, clock, content, ok := messageEdit(unwrapped); ok {
		if id != secret.GetTargetMessageKey().GetID() {
			return "", 0, nil, false
		}
		return id, clock, content, true
	}
	return secret.GetTargetMessageKey().GetID(), evt.Info.Timestamp.UnixMilli(), unwrapped.Message, true
}

// Live events retain the edit protocol; ParseWebMessage unwraps its content.
// Read RawMessage too so both paths retain the original ID and edit clock.
func messageEdit(evt *events.Message) (string, int64, *waE2E.Message, bool) {
	message := evt.Message
	if evt.RawMessage != nil {
		copy := *evt
		copy.UnwrapRaw()
		message = copy.Message
	}
	protocol := message.GetProtocolMessage()
	if protocol.GetType() == waE2E.ProtocolMessage_MESSAGE_EDIT && protocol.GetEditedMessage() != nil {
		id := protocol.GetKey().GetID()
		if id == "" {
			return "", 0, nil, false
		}
		timestamp := protocol.GetTimestampMS()
		if timestamp <= 0 {
			timestamp = evt.Info.Timestamp.UnixMilli()
		}
		return id, timestamp, protocol.GetEditedMessage(), true
	}
	if evt.NewsletterMeta != nil && !evt.NewsletterMeta.EditTS.IsZero() {
		return evt.Info.ID, evt.NewsletterMeta.EditTS.UnixMilli(), evt.Message, true
	}
	return "", 0, nil, false
}

func NewMessageEditEvent(msg Message, editedAtMS int64) map[string]interface{} {
	event := NewMessageEvent(msg)
	event["type"] = "message_edit"
	event["edited_at_ms"] = editedAtMS
	// A caption-only edit may omit media metadata. Keep required wire fields;
	// storage retains the original attachment rather than using these defaults.
	if msg.Content.Type == "image" || msg.Content.Type == "video" || msg.Content.Type == "document" {
		event["content"] = struct {
			MessageContent
			MimeType string `json:"mime_type"`
			FileSize uint64 `json:"file_size"`
		}{msg.Content, msg.Content.MimeType, msg.Content.FileSize}
	}
	return event
}
