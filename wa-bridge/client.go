package main

import (
	"context"
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"os"
	"strings"

	"github.com/rs/zerolog"
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/store/sqlstore"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	waLog "go.mau.fi/whatsmeow/util/log"

	_ "github.com/mattn/go-sqlite3"
)

// Client wraps the whatsmeow client and handles events
type Client struct {
	client    *whatsmeow.Client
	container *sqlstore.Container
	verbose   bool
	ctx       context.Context
}

// stderrLogger creates a logger that writes to stderr (not stdout)
// This prevents log output from mixing with our JSON protocol on stdout
func stderrLogger(module string, verbose bool) waLog.Logger {
	var level zerolog.Level
	if verbose {
		level = zerolog.DebugLevel
	} else {
		level = zerolog.WarnLevel
	}

	logger := zerolog.New(zerolog.ConsoleWriter{
		Out:        os.Stderr,
		TimeFormat: "15:04:05.000",
	}).Level(level).With().Str("module", module).Timestamp().Logger()

	return waLog.Zerolog(logger)
}

// NewClient creates a new WhatsApp client
func NewClient(ctx context.Context, dataDir string, verbose bool) (*Client, error) {
	// Set up logging to stderr (not stdout, which is reserved for JSON protocol)
	dbLog := stderrLogger("Database", verbose)

	// Create database container for session storage
	dbPath := fmt.Sprintf("%s/session.db", dataDir)
	container, err := sqlstore.New(ctx, "sqlite3", fmt.Sprintf("file:%s?_foreign_keys=on", dbPath), dbLog)
	if err != nil {
		return nil, fmt.Errorf("failed to create database: %w", err)
	}

	// Get or create device store
	deviceStore, err := container.GetFirstDevice(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to get device: %w", err)
	}

	// Create client with stderr logger
	clientLog := stderrLogger("Client", verbose)
	client := whatsmeow.NewClient(deviceStore, clientLog)

	c := &Client{
		client:    client,
		container: container,
		verbose:   verbose,
		ctx:       ctx,
	}

	// Register event handler
	client.AddEventHandler(c.handleEvent)

	return c, nil
}

// Connect initiates the connection to WhatsApp
func (c *Client) Connect(ctx context.Context) error {
	if c.client.Store.ID == nil {
		// No existing session, need to pair with QR code
		qrChan, _ := c.client.GetQRChannel(ctx)
		err := c.client.Connect()
		if err != nil {
			return fmt.Errorf("failed to connect: %w", err)
		}

		// Wait for QR code or successful login
		for evt := range qrChan {
			switch evt.Event {
			case "code":
				// Send QR code to Rust CLI
				SendEvent(NewQREvent(evt.Code))
			case "success":
				// Successfully paired
				c.sendConnectedEvent()
				return nil
			case "timeout":
				return fmt.Errorf("QR code scan timed out")
			}
		}
	} else {
		// Existing session, just connect
		err := c.client.Connect()
		if err != nil {
			return fmt.Errorf("failed to connect: %w", err)
		}
	}

	return nil
}

// Disconnect gracefully disconnects from WhatsApp
func (c *Client) Disconnect() {
	c.client.Disconnect()
}

// Close closes the database connection
func (c *Client) Close() error {
	return c.container.Close()
}

// Logout clears the session and disconnects
func (c *Client) Logout(ctx context.Context) error {
	return c.client.Logout(ctx)
}

// IsConnected returns whether the client is connected
func (c *Client) IsConnected() bool {
	return c.client.IsConnected()
}

// IsLoggedIn returns whether the client is logged in
func (c *Client) IsLoggedIn() bool {
	return c.client.IsLoggedIn()
}

// handleEvent processes events from whatsmeow
func (c *Client) handleEvent(evt interface{}) {
	// Log all event types for debugging (enable with verbose mode)
	if c.verbose {
		SendEvent(NewLogEvent("debug", fmt.Sprintf("Event received: %T", evt)))
	}

	switch v := evt.(type) {
	case *events.Connected:
		c.sendConnectedEvent()

	case *events.Disconnected:
		SendEvent(NewConnectionStateEvent("disconnected"))

	case *events.LoggedOut:
		reason := "unknown"
		if v.Reason != 0 {
			reason = fmt.Sprintf("code: %d", v.Reason)
		}
		SendEvent(NewLoggedOutEvent(reason))

	case *events.StreamReplaced:
		SendEvent(NewLoggedOutEvent("stream replaced by another connection"))

	case *events.Message:
		c.handleMessage(v)

	case *events.Receipt:
		// Delivery/read receipts - could be useful in future
		if c.verbose {
			SendEvent(NewLogEvent("debug", fmt.Sprintf("Receipt: %s from %s", v.Type, v.Sender)))
		}

	case *events.Presence:
		// Online/offline status - could be useful in future
		if c.verbose {
			SendEvent(NewLogEvent("debug", fmt.Sprintf("Presence: %s is %s", v.From, v.Unavailable)))
		}

	case *events.ChatPresence:
		// Typing/recording indicators
		state := "paused"
		switch v.State {
		case types.ChatPresenceComposing:
			// Check if it's audio recording vs text typing
			if v.Media == types.ChatPresenceMediaAudio {
				state = "recording"
			} else {
				state = "typing"
			}
		case types.ChatPresencePaused:
			state = "paused"
		}
		// Log for debugging (use info level so it always shows)
		SendEvent(NewLogEvent("info", fmt.Sprintf("ChatPresence: chat=%s sender=%s state=%s media=%s", v.Chat.String(), v.Sender.String(), v.State, v.Media)))
		// Send chat presence event
		SendEvent(NewChatPresenceEvent(v.Chat.String(), v.Sender.String(), state))

	case *events.HistorySync:
		// History sync - could be useful for archiving old messages
		SendEvent(NewLogEvent("info", fmt.Sprintf("Received history sync: %d conversations", len(v.Data.Conversations))))

	case *events.UndecryptableMessage:
		// Message couldn't be decrypted
		SendEvent(NewLogEvent("error", fmt.Sprintf("Undecryptable message from %s: %v", v.Info.Sender, v.DecryptFailMode)))

	case *events.PushName:
		// Contact name updates
		if c.verbose {
			SendEvent(NewLogEvent("debug", fmt.Sprintf("PushName update: %s = %s", v.JID, v.NewPushName)))
		}

	case *events.OfflineSyncCompleted:
		// Offline sync completed - all pending messages delivered
		SendEvent(NewLogEvent("info", "Offline sync completed"))
	}
}

// sendConnectedEvent sends the connected event with device info
func (c *Client) sendConnectedEvent() {
	phone := ""
	name := ""
	platform := ""

	if c.client.Store.ID != nil {
		phone = c.client.Store.ID.User
	}

	if c.client.Store.PushName != "" {
		name = c.client.Store.PushName
	}

	if c.client.Store.Platform != "" {
		platform = c.client.Store.Platform
	}

	SendEvent(NewConnectedEvent(phone, name, platform))
}

// handleMessage processes incoming messages
func (c *Client) handleMessage(evt *events.Message) {
	msg := Message{
		ID:        evt.Info.ID,
		Timestamp: evt.Info.Timestamp.Unix(),
		IsFromMe:  evt.Info.IsFromMe,
	}

	// Check for forwarding info in the message itself
	if evt.Message != nil && evt.Message.ExtendedTextMessage != nil {
		if evt.Message.ExtendedTextMessage.ContextInfo != nil {
			msg.IsForwarded = evt.Message.ExtendedTextMessage.ContextInfo.IsForwarded != nil &&
				*evt.Message.ExtendedTextMessage.ContextInfo.IsForwarded
		}
	}

	// Set push name
	if evt.Info.PushName != "" {
		msg.PushName = evt.Info.PushName
	}

	// Set sender info
	msg.From = c.buildContact(evt.Info.Sender)

	// Set chat info
	msg.Chat = c.buildChat(evt.Info)

	// Set message content (with media download)
	msg.Content = c.buildMessageContent(evt.Message)

	// Skip protocol messages and unknown types - these shouldn't be displayed
	if msg.Content.Type == "protocol" || msg.Content.Type == "unknown" {
		return
	}

	// Download media for image/video/audio/document messages
	if evt.Message != nil {
		c.downloadMediaForMessage(evt.Message, &msg.Content)
	}

	// Send the message event
	SendEvent(NewMessageEvent(msg))
}

// downloadMediaForMessage downloads media data and adds it to the content
func (c *Client) downloadMediaForMessage(waMsg *waE2E.Message, content *MessageContent) {
	var data []byte
	var err error

	// Size limit for media downloads (50MB)
	const maxMediaSize uint64 = 50 * 1024 * 1024

	SendEvent(NewLogEvent("info", fmt.Sprintf("Attempting media download for type: %s, size: %d", content.Type, content.FileSize)))

	switch content.Type {
	case "image":
		if waMsg.ImageMessage != nil {
			if content.FileSize > 0 && content.FileSize > maxMediaSize {
				SendEvent(NewLogEvent("warn", fmt.Sprintf("Image too large to download: %d bytes", content.FileSize)))
				return
			}
			SendEvent(NewLogEvent("info", "Downloading image..."))
			data, err = c.client.Download(c.ctx, waMsg.ImageMessage)
		} else {
			SendEvent(NewLogEvent("warn", "Image content type but ImageMessage is nil"))
		}
	case "video":
		if waMsg.VideoMessage != nil {
			if content.FileSize > 0 && content.FileSize > maxMediaSize {
				SendEvent(NewLogEvent("warn", fmt.Sprintf("Video too large to download: %d bytes", content.FileSize)))
				return
			}
			SendEvent(NewLogEvent("info", "Downloading video..."))
			data, err = c.client.Download(c.ctx, waMsg.VideoMessage)
		} else {
			SendEvent(NewLogEvent("warn", "Video content type but VideoMessage is nil"))
		}
	case "audio":
		if waMsg.AudioMessage != nil {
			if content.FileSize > 0 && content.FileSize > maxMediaSize {
				SendEvent(NewLogEvent("warn", fmt.Sprintf("Audio too large to download: %d bytes", content.FileSize)))
				return
			}
			SendEvent(NewLogEvent("info", "Downloading audio..."))
			data, err = c.client.Download(c.ctx, waMsg.AudioMessage)
		} else {
			SendEvent(NewLogEvent("warn", "Audio content type but AudioMessage is nil"))
		}
	case "document":
		if waMsg.DocumentMessage != nil {
			if content.FileSize > 0 && content.FileSize > maxMediaSize {
				SendEvent(NewLogEvent("warn", fmt.Sprintf("Document too large to download: %d bytes", content.FileSize)))
				return
			}
			SendEvent(NewLogEvent("info", "Downloading document..."))
			data, err = c.client.Download(c.ctx, waMsg.DocumentMessage)
		} else {
			SendEvent(NewLogEvent("warn", "Document content type but DocumentMessage is nil"))
		}
	case "sticker":
		if waMsg.StickerMessage != nil {
			SendEvent(NewLogEvent("info", "Downloading sticker..."))
			data, err = c.client.Download(c.ctx, waMsg.StickerMessage)
		} else {
			SendEvent(NewLogEvent("warn", "Sticker content type but StickerMessage is nil"))
		}
	default:
		return
	}

	if err != nil {
		SendEvent(NewLogEvent("warn", fmt.Sprintf("Failed to download media: %v", err)))
		return
	}

	if len(data) > 0 {
		SendEvent(NewLogEvent("info", fmt.Sprintf("Media downloaded successfully: %d bytes", len(data))))
		content.MediaData = base64.StdEncoding.EncodeToString(data)
	} else {
		SendEvent(NewLogEvent("warn", "Media download returned empty data"))
	}
}

// buildContact creates a Contact from a JID
func (c *Client) buildContact(jid types.JID) Contact {
	contact := Contact{
		JID:   jid.String(),
		Phone: jid.User,
	}

	// Try to get contact name from store
	contactInfo, err := c.client.Store.Contacts.GetContact(c.ctx, jid)
	if err == nil && contactInfo.Found {
		if contactInfo.FullName != "" {
			contact.Name = contactInfo.FullName
		} else if contactInfo.PushName != "" {
			contact.Name = contactInfo.PushName
		} else if contactInfo.BusinessName != "" {
			contact.Name = contactInfo.BusinessName
		}
	}

	return contact
}

// buildChat creates a Chat from message info
func (c *Client) buildChat(info types.MessageInfo) Chat {
	chat := Chat{
		JID: info.Chat.String(),
	}

	if info.IsGroup {
		chat.Type = "group"
		// Try to get group info
		groupInfo, err := c.client.GetGroupInfo(c.ctx, info.Chat)
		if err == nil {
			chat.Name = groupInfo.Name
			count := len(groupInfo.Participants)
			chat.ParticipantCount = &count
		}
	} else if strings.HasSuffix(info.Chat.Server, "broadcast") {
		chat.Type = "broadcast"
	} else if info.Chat.Server == "status@broadcast" {
		chat.Type = "status"
	} else {
		chat.Type = "private"
		// For private chats, get the contact name from the chat JID (the other person)
		contactInfo, err := c.client.Store.Contacts.GetContact(c.ctx, info.Chat)
		if err == nil && contactInfo.Found {
			if contactInfo.FullName != "" {
				chat.Name = contactInfo.FullName
			} else if contactInfo.PushName != "" {
				chat.Name = contactInfo.PushName
			} else if contactInfo.BusinessName != "" {
				chat.Name = contactInfo.BusinessName
			}
		}
	}

	return chat
}

// buildMessageContent extracts content from a WhatsApp message
func (c *Client) buildMessageContent(msg *waE2E.Message) MessageContent {
	if msg == nil {
		return MessageContent{Type: "unknown", RawType: "nil"}
	}

	// Text message
	if msg.Conversation != nil && *msg.Conversation != "" {
		return MessageContent{
			Type: "text",
			Body: *msg.Conversation,
		}
	}

	// Extended text message (with URL preview, etc.)
	if msg.ExtendedTextMessage != nil {
		body := ""
		if msg.ExtendedTextMessage.Text != nil {
			body = *msg.ExtendedTextMessage.Text
		}
		return MessageContent{
			Type: "text",
			Body: body,
		}
	}

	// Image message
	if msg.ImageMessage != nil {
		content := MessageContent{
			Type:     "image",
			MimeType: getString(msg.ImageMessage.Mimetype),
			FileSize: getUint64(msg.ImageMessage.FileLength),
		}
		if msg.ImageMessage.Caption != nil {
			content.Caption = *msg.ImageMessage.Caption
		}
		if msg.ImageMessage.FileSHA256 != nil {
			content.FileHash = hex.EncodeToString(msg.ImageMessage.FileSHA256)
		}
		return content
	}

	// Video message
	if msg.VideoMessage != nil {
		content := MessageContent{
			Type:     "video",
			MimeType: getString(msg.VideoMessage.Mimetype),
			FileSize: getUint64(msg.VideoMessage.FileLength),
		}
		if msg.VideoMessage.Caption != nil {
			content.Caption = *msg.VideoMessage.Caption
		}
		if msg.VideoMessage.Seconds != nil {
			dur := *msg.VideoMessage.Seconds
			content.DurationSeconds = &dur
		}
		return content
	}

	// Audio message
	if msg.AudioMessage != nil {
		content := MessageContent{
			Type:     "audio",
			MimeType: getString(msg.AudioMessage.Mimetype),
			FileSize: getUint64(msg.AudioMessage.FileLength),
		}
		if msg.AudioMessage.Seconds != nil {
			dur := *msg.AudioMessage.Seconds
			content.DurationSeconds = &dur
		}
		if msg.AudioMessage.PTT != nil {
			content.IsVoiceNote = *msg.AudioMessage.PTT
		}
		return content
	}

	// Document message
	if msg.DocumentMessage != nil {
		content := MessageContent{
			Type:     "document",
			MimeType: getString(msg.DocumentMessage.Mimetype),
			FileSize: getUint64(msg.DocumentMessage.FileLength),
		}
		if msg.DocumentMessage.FileName != nil {
			content.FileName = *msg.DocumentMessage.FileName
		}
		if msg.DocumentMessage.Caption != nil {
			content.Caption = *msg.DocumentMessage.Caption
		}
		return content
	}

	// Sticker message
	if msg.StickerMessage != nil {
		content := MessageContent{
			Type:     "sticker",
			MimeType: getString(msg.StickerMessage.Mimetype),
		}
		if msg.StickerMessage.IsAnimated != nil {
			content.IsAnimated = *msg.StickerMessage.IsAnimated
		}
		return content
	}

	// Location message
	if msg.LocationMessage != nil {
		content := MessageContent{
			Type: "location",
		}
		if msg.LocationMessage.DegreesLatitude != nil {
			lat := *msg.LocationMessage.DegreesLatitude
			content.Latitude = &lat
		}
		if msg.LocationMessage.DegreesLongitude != nil {
			lng := *msg.LocationMessage.DegreesLongitude
			content.Longitude = &lng
		}
		if msg.LocationMessage.Name != nil {
			content.LocationName = *msg.LocationMessage.Name
		}
		if msg.LocationMessage.Address != nil {
			content.Address = *msg.LocationMessage.Address
		}
		return content
	}

	// Contact message
	if msg.ContactMessage != nil {
		return MessageContent{
			Type:        "contact",
			DisplayName: getString(msg.ContactMessage.DisplayName),
			VCard:       getString(msg.ContactMessage.Vcard),
		}
	}

	// Reaction message
	if msg.ReactionMessage != nil {
		return MessageContent{
			Type:            "reaction",
			Emoji:           getString(msg.ReactionMessage.Text),
			TargetMessageID: msg.ReactionMessage.Key.GetID(),
		}
	}

	// Protocol message (includes revoked/deleted messages, ephemeral settings, etc.)
	if msg.ProtocolMessage != nil {
		if msg.ProtocolMessage.Type != nil {
			switch *msg.ProtocolMessage.Type {
			case waE2E.ProtocolMessage_REVOKE:
				return MessageContent{Type: "revoked"}
			default:
				// Other protocol messages (ephemeral settings, history sync, etc.)
				// should be ignored - return special type that will be filtered out
				return MessageContent{Type: "protocol", RawType: msg.ProtocolMessage.Type.String()}
			}
		}
	}

	// Sender key distribution message (group encryption setup) - ignore
	if msg.SenderKeyDistributionMessage != nil {
		return MessageContent{Type: "protocol", RawType: "sender_key_distribution"}
	}

	// Message context info only (no actual content) - ignore
	if msg.MessageContextInfo != nil && msg.Conversation == nil && msg.ExtendedTextMessage == nil {
		return MessageContent{Type: "protocol", RawType: "context_info_only"}
	}

	// Poll creation message
	if msg.PollCreationMessage != nil {
		content := MessageContent{
			Type: "poll",
		}
		if msg.PollCreationMessage.Name != nil {
			content.Question = *msg.PollCreationMessage.Name
		}
		for _, opt := range msg.PollCreationMessage.Options {
			if opt.OptionName != nil {
				content.Options = append(content.Options, *opt.OptionName)
			}
		}
		return content
	}

	// Unknown message type
	return MessageContent{
		Type:    "unknown",
		RawType: fmt.Sprintf("%T", msg),
	}
}

// Helper functions

func getString(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

func getUint64(u *uint64) uint64 {
	if u == nil {
		return 0
	}
	return *u
}

// GetStore returns the underlying store for direct access if needed
func (c *Client) GetStore() *store.Device {
	return c.client.Store
}

// SendTextMessage sends a text message to the specified JID
// If replyToID is provided, the message will be a reply to that message
func (c *Client) SendTextMessage(ctx context.Context, jidStr string, text string, replyToID string, replyToSender string) (string, int64, error) {
	// Parse the JID
	jid, err := types.ParseJID(jidStr)
	if err != nil {
		return "", 0, fmt.Errorf("invalid JID: %w", err)
	}

	var msg *waE2E.Message

	// Check if this is a reply
	if replyToID != "" {
		// Create context info for the reply
		contextInfo := &waE2E.ContextInfo{
			StanzaID: &replyToID,
		}

		// Set the participant (sender of the quoted message)
		// Convert phone number to full JID if needed
		if replyToSender != "" {
			participant := replyToSender
			// If it's just a phone number, convert to JID format
			if !strings.Contains(replyToSender, "@") {
				participant = replyToSender + "@s.whatsapp.net"
			}
			contextInfo.Participant = &participant
		}

		// Use ExtendedTextMessage for replies (required for ContextInfo)
		msg = &waE2E.Message{
			ExtendedTextMessage: &waE2E.ExtendedTextMessage{
				Text:        &text,
				ContextInfo: contextInfo,
			},
		}
	} else {
		// Simple message without reply
		msg = &waE2E.Message{
			Conversation: &text,
		}
	}

	// Send the message
	resp, err := c.client.SendMessage(ctx, jid, msg)
	if err != nil {
		return "", 0, fmt.Errorf("failed to send message: %w", err)
	}

	return resp.ID, resp.Timestamp.Unix(), nil
}

// SendImageMessage sends an image message to the specified JID
// If replyToID is provided, the message will be a reply to that message
func (c *Client) SendImageMessage(ctx context.Context, jidStr string, mediaDataB64 string, mimeType string, caption string, replyToID string, replyToSender string) (string, int64, error) {
	// Parse the JID
	jid, err := types.ParseJID(jidStr)
	if err != nil {
		return "", 0, fmt.Errorf("invalid JID: %w", err)
	}

	// Decode base64 image data
	imageData, err := base64.StdEncoding.DecodeString(mediaDataB64)
	if err != nil {
		return "", 0, fmt.Errorf("failed to decode image data: %w", err)
	}

	// Default mime type if not provided
	if mimeType == "" {
		mimeType = "image/jpeg"
	}

	// Upload the image to WhatsApp
	uploadResp, err := c.client.Upload(ctx, imageData, whatsmeow.MediaImage)
	if err != nil {
		return "", 0, fmt.Errorf("failed to upload image: %w", err)
	}

	// Create the image message
	imageMsg := &waE2E.ImageMessage{
		Mimetype:      &mimeType,
		URL:           &uploadResp.URL,
		DirectPath:    &uploadResp.DirectPath,
		MediaKey:      uploadResp.MediaKey,
		FileEncSHA256: uploadResp.FileEncSHA256,
		FileSHA256:    uploadResp.FileSHA256,
		FileLength:    &uploadResp.FileLength,
	}

	if caption != "" {
		imageMsg.Caption = &caption
	}

	// Add reply context if provided
	if replyToID != "" {
		contextInfo := &waE2E.ContextInfo{
			StanzaID: &replyToID,
		}
		// Convert phone number to full JID if needed
		if replyToSender != "" {
			participant := replyToSender
			if !strings.Contains(replyToSender, "@") {
				participant = replyToSender + "@s.whatsapp.net"
			}
			contextInfo.Participant = &participant
		}
		imageMsg.ContextInfo = contextInfo
	}

	msg := &waE2E.Message{
		ImageMessage: imageMsg,
	}

	// Send the message
	resp, err := c.client.SendMessage(ctx, jid, msg)
	if err != nil {
		return "", 0, fmt.Errorf("failed to send image: %w", err)
	}

	return resp.ID, resp.Timestamp.Unix(), nil
}

// SendReaction sends a reaction to a message
func (c *Client) SendReaction(ctx context.Context, chatJIDStr string, targetMessageID string, senderJIDStr string, emoji string) (string, int64, error) {
	// Parse the chat JID
	chatJID, err := types.ParseJID(chatJIDStr)
	if err != nil {
		return "", 0, fmt.Errorf("invalid chat JID: %w", err)
	}

	// Parse the sender JID (who sent the message we're reacting to)
	var senderJID types.JID
	if senderJIDStr != "" {
		senderJID, err = types.ParseJID(senderJIDStr)
		if err != nil {
			return "", 0, fmt.Errorf("invalid sender JID: %w", err)
		}
	} else {
		// If no sender JID provided, assume it's the chat JID (for private chats)
		senderJID = chatJID
	}

	// Build and send the reaction message
	reactionMsg := c.client.BuildReaction(chatJID, senderJID, targetMessageID, emoji)

	resp, err := c.client.SendMessage(ctx, chatJID, reactionMsg)
	if err != nil {
		return "", 0, fmt.Errorf("failed to send reaction: %w", err)
	}

	return resp.ID, resp.Timestamp.Unix(), nil
}

// GetProfilePicture fetches the profile picture URL for a JID
func (c *Client) GetProfilePicture(ctx context.Context, jidStr string) (string, string, error) {
	// Parse the JID
	jid, err := types.ParseJID(jidStr)
	if err != nil {
		return "", "", fmt.Errorf("invalid JID: %w", err)
	}

	// Get profile picture info
	params := &whatsmeow.GetProfilePictureParams{
		Preview: false, // Get full size image
	}
	pic, err := c.client.GetProfilePictureInfo(ctx, jid, params)
	if err != nil {
		return "", "", fmt.Errorf("failed to get profile picture: %w", err)
	}

	if pic == nil {
		return "", "", nil // No profile picture set
	}

	return pic.URL, pic.ID, nil
}
