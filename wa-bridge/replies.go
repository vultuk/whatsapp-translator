package main

import (
	"strings"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/reflect/protoreflect"
)

// Only unwrap known containers, with a bound even for malformed nested input.
func unwrapQuote(message *waE2E.Message) *waE2E.Message {
	for depth := 0; message != nil && depth < 8; depth++ {
		var next *waE2E.Message
		switch {
		case message.GetEphemeralMessage() != nil:
			next = message.GetEphemeralMessage().GetMessage()
		case message.GetViewOnceMessage() != nil:
			next = message.GetViewOnceMessage().GetMessage()
		case message.GetViewOnceMessageV2() != nil:
			next = message.GetViewOnceMessageV2().GetMessage()
		case message.GetViewOnceMessageV2Extension() != nil:
			next = message.GetViewOnceMessageV2Extension().GetMessage()
		case message.GetDocumentWithCaptionMessage() != nil:
			next = message.GetDocumentWithCaptionMessage().GetMessage()
		default:
			return message
		}
		message = next
	}
	return nil
}

// Every WhatsApp payload carries the same typed ContextInfo. Inspect only direct
// payloads so a nested quote's context can never be mistaken for this reply.
func replyContextInfo(message *waE2E.Message) *waE2E.ContextInfo {
	message = unwrapQuote(message)
	if message == nil {
		return nil
	}
	var context *waE2E.ContextInfo
	message.ProtoReflect().Range(func(field protoreflect.FieldDescriptor, value protoreflect.Value) bool {
		if field.Kind() == protoreflect.MessageKind && !field.IsList() && !field.IsMap() {
			if payload, ok := value.Message().Interface().(interface{ GetContextInfo() *waE2E.ContextInfo }); ok {
				candidate := payload.GetContextInfo()
				if candidate.GetStanzaID() != "" {
					context = candidate
					return false
				}
			}
		}
		return true
	})
	return context
}

func quotedPreview(message *waE2E.Message) string {
	message = unwrapQuote(message)
	var text string
	switch {
	case message == nil:
		text = "Original message unavailable"
	case message.GetConversation() != "":
		text = message.GetConversation()
	case message.GetExtendedTextMessage() != nil:
		text = message.GetExtendedTextMessage().GetText()
	case message.GetImageMessage() != nil:
		text = "Photo"
		if caption := message.GetImageMessage().GetCaption(); caption != "" {
			text += ": " + caption
		}
	case message.GetVideoMessage() != nil:
		text = "Video"
		if caption := message.GetVideoMessage().GetCaption(); caption != "" {
			text += ": " + caption
		}
	case message.GetAudioMessage() != nil:
		text = "Audio"
		if message.GetAudioMessage().GetPTT() {
			text = "Voice message"
		}
	case message.GetDocumentMessage() != nil:
		text = "Document"
		if name := message.GetDocumentMessage().GetFileName(); name != "" {
			text += ": " + name
		}
		if caption := message.GetDocumentMessage().GetCaption(); caption != "" {
			text += ": " + caption
		}
	case message.GetStickerMessage() != nil:
		text = "Sticker"
	case message.GetLocationMessage() != nil:
		text = "Location"
		if name := message.GetLocationMessage().GetName(); name != "" {
			text += ": " + name
		}
	case message.GetLiveLocationMessage() != nil:
		text = "Live location"
	case message.GetContactMessage() != nil:
		text = "Contact"
		if name := message.GetContactMessage().GetDisplayName(); name != "" {
			text += ": " + name
		}
	case message.GetContactsArrayMessage() != nil:
		text = "Contacts"
	case message.GetPollCreationMessage() != nil:
		text = message.GetPollCreationMessage().GetName()
	case message.GetPollCreationMessageV2() != nil:
		text = message.GetPollCreationMessageV2().GetName()
	case message.GetPollCreationMessageV3() != nil:
		text = message.GetPollCreationMessageV3().GetName()
	case message.GetAlbumMessage() != nil:
		text = "Photo album"
	default:
		text = "Original message unavailable"
	}
	text = strings.TrimSpace(text)
	if text == "" {
		text = "Original message unavailable"
	}
	runes := []rune(text)
	if len(runes) > 2000 {
		text = string(runes[:2000]) + "…"
	}
	return text
}

func (c *Client) incomingReplyContext(message *waE2E.Message) *ReplyContext {
	context := replyContextInfo(message)
	if context == nil {
		return nil
	}
	name := "Unknown sender"
	jid, err := types.ParseJID(context.GetParticipant())
	if err == nil && jid.User != "" {
		if jid.Server == types.DefaultUserServer {
			name = "+" + jid.User
		}
		if c != nil && c.client != nil && c.client.Store != nil {
			if c.client.Store.ID != nil && jid.ToNonAD() == c.client.Store.ID.ToNonAD() || jid.ToNonAD() == c.client.Store.LID.ToNonAD() {
				name = "You"
			} else if c.client.Store.Contacts != nil {
				for _, contact := range c.buildMentions([]string{jid.String()}) {
					if contact.Name != "" {
						name = contact.Name
					} else if contact.Phone != "" {
						name = "+" + contact.Phone
					}
				}
			}
		}
	}
	return &ReplyContext{MessageID: context.GetStanzaID(), SenderName: name, Text: quotedPreview(context.GetQuotedMessage())}
}
