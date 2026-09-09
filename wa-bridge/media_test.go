package main

import (
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
	"testing"
)

func TestMediaMessagesPreserveUploadAndReplyContext(t *testing.T) {
	jid, _ := types.ParseJID("one@g.us")
	upload := whatsmeow.UploadResponse{URL: "https://example.invalid/media", DirectPath: "/media", FileLength: 42, MediaKey: []byte{1, 2}, FileSHA256: []byte{3}, FileEncSHA256: []byte{4}}
	for _, kind := range []string{"video", "document"} {
		for _, reply := range []string{"", "captured-message"} {
			cmd := Command{MediaKind: kind, MimeType: "video/mp4", FileName: "Attachment.mp4", Caption: "Caption", ReplyTo: reply, ReplyToSender: "sam@s.whatsapp.net", ReplyToText: "Original"}
			msg := buildMediaMessage(cmd, jid, upload)
			context := msg.GetVideoMessage().GetContextInfo()
			if kind == "document" {
				if msg.GetDocumentMessage().GetFileName() != cmd.FileName || msg.GetDocumentMessage().GetFileLength() != 42 {
					t.Fatal("document fields lost")
				}
				context = msg.GetDocumentMessage().GetContextInfo()
			} else if msg.GetVideoMessage().GetCaption() != "Caption" || msg.GetVideoMessage().GetFileLength() != 42 {
				t.Fatal("video fields lost")
			}
			if reply == "" {
				if context != nil {
					t.Fatal("latest message must send without a quote")
				}
			} else if context.GetStanzaID() != reply || context.GetParticipant() != cmd.ReplyToSender || context.GetRemoteJID() != jid.String() {
				t.Fatal("captured reply context lost")
			}
		}
	}
}
