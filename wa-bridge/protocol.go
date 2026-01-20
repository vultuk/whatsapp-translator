// Package main provides JSON protocol types for communication with the Rust CLI.
package main

import (
	"encoding/json"
	"fmt"
	"time"
)

// Event types sent to Rust CLI (via stdout)

// QREvent is sent when a QR code needs to be displayed for pairing
type QREvent struct {
	Type string `json:"type"`
	Data string `json:"data"`
}

// ConnectedEvent is sent when successfully connected to WhatsApp
type ConnectedEvent struct {
	Type     string `json:"type"`
	Phone    string `json:"phone"`
	Name     string `json:"name"`
	Platform string `json:"platform,omitempty"`
}

// ConnectionStateEvent is sent when connection state changes
type ConnectionStateEvent struct {
	Type  string `json:"type"`
	State string `json:"state"`
}

// MessageEvent is sent when a message is received
type MessageEvent struct {
	Type    string  `json:"type"`
	Message Message `json:"message,omitempty"`
	// Flatten message fields for simpler parsing
	ID          string         `json:"id"`
	Timestamp   int64          `json:"timestamp"`
	From        Contact        `json:"from"`
	Chat        Chat           `json:"chat"`
	Content     MessageContent `json:"content"`
	IsFromMe    bool           `json:"is_from_me"`
	IsForwarded bool           `json:"is_forwarded"`
	PushName    string         `json:"push_name,omitempty"`
}

// ErrorEvent is sent when an error occurs
type ErrorEvent struct {
	Type    string `json:"type"`
	Code    string `json:"code"`
	Message string `json:"message"`
}

// LogEvent is sent for informational logging
type LogEvent struct {
	Type    string `json:"type"`
	Level   string `json:"level"`
	Message string `json:"message"`
}

// LoggedOutEvent is sent when the session is logged out
type LoggedOutEvent struct {
	Type   string `json:"type"`
	Reason string `json:"reason"`
}

// Message represents a WhatsApp message with full metadata
type Message struct {
	ID          string         `json:"id"`
	Timestamp   int64          `json:"timestamp"`
	From        Contact        `json:"from"`
	Chat        Chat           `json:"chat"`
	Content     MessageContent `json:"content"`
	IsFromMe    bool           `json:"is_from_me"`
	IsForwarded bool           `json:"is_forwarded"`
	PushName    string         `json:"push_name,omitempty"`
}

// Contact represents a WhatsApp contact
type Contact struct {
	JID   string `json:"jid"`
	Phone string `json:"phone"`
	Name  string `json:"name,omitempty"`
}

// Chat represents a chat (private, group, broadcast, or status)
type Chat struct {
	Type             string `json:"type"` // "private", "group", "broadcast", "status"
	JID              string `json:"jid"`
	Name             string `json:"name,omitempty"`
	ParticipantCount *int   `json:"participant_count,omitempty"`
}

// MessageContent represents the content of a message
type MessageContent struct {
	Type            string   `json:"type"`
	Body            string   `json:"body,omitempty"`
	Caption         string   `json:"caption,omitempty"`
	MimeType        string   `json:"mime_type,omitempty"`
	FileName        string   `json:"file_name,omitempty"`
	FileSize        uint64   `json:"file_size,omitempty"`
	FileHash        string   `json:"file_hash,omitempty"`
	MediaData       string   `json:"media_data,omitempty"` // Base64 encoded media data
	DurationSeconds *uint32  `json:"duration_seconds,omitempty"`
	IsVoiceNote     bool     `json:"is_voice_note,omitempty"`
	IsAnimated      bool     `json:"is_animated,omitempty"`
	Latitude        *float64 `json:"latitude,omitempty"`
	Longitude       *float64 `json:"longitude,omitempty"`
	LocationName    string   `json:"name,omitempty"`
	Address         string   `json:"address,omitempty"`
	DisplayName     string   `json:"display_name,omitempty"`
	VCard           string   `json:"vcard,omitempty"`
	Emoji           string   `json:"emoji,omitempty"`
	TargetMessageID string   `json:"target_message_id,omitempty"`
	Question        string   `json:"question,omitempty"`
	Options         []string `json:"options,omitempty"`
	RawType         string   `json:"raw_type,omitempty"`
}

// SendResultEvent is sent after attempting to send a message
type SendResultEvent struct {
	Type      string `json:"type"`
	RequestID int    `json:"request_id"`
	Success   bool   `json:"success"`
	MessageID string `json:"message_id,omitempty"`
	Timestamp int64  `json:"timestamp,omitempty"`
	Error     string `json:"error,omitempty"`
}

// ProfilePictureEvent is sent with profile picture data
type ProfilePictureEvent struct {
	Type      string `json:"type"`
	RequestID int    `json:"request_id"`
	JID       string `json:"jid"`
	URL       string `json:"url,omitempty"`
	ID        string `json:"id,omitempty"`
	Error     string `json:"error,omitempty"`
}

// ChatPresenceEvent is sent when someone starts/stops typing
type ChatPresenceEvent struct {
	Type   string `json:"type"`
	ChatID string `json:"chat_id"` // The chat JID
	UserID string `json:"user_id"` // Who is typing (for groups)
	State  string `json:"state"`   // "typing", "paused", or "recording"
}

// Command types received from Rust CLI (via stdin)

// Command represents a command from the Rust CLI
type Command struct {
	Type      string `json:"type"`
	RequestID int    `json:"request_id,omitempty"`
	To        string `json:"to,omitempty"`
	Text      string `json:"text,omitempty"`
	// For send_image command
	MediaData string `json:"media_data,omitempty"` // Base64 encoded image
	MimeType  string `json:"mime_type,omitempty"`
	Caption   string `json:"caption,omitempty"`
}

// Helper functions to create events

func NewQREvent(data string) QREvent {
	return QREvent{Type: "qr", Data: data}
}

func NewConnectedEvent(phone, name, platform string) ConnectedEvent {
	return ConnectedEvent{
		Type:     "connected",
		Phone:    phone,
		Name:     name,
		Platform: platform,
	}
}

func NewConnectionStateEvent(state string) ConnectionStateEvent {
	return ConnectionStateEvent{Type: "connection_state", State: state}
}

func NewMessageEvent(msg Message) map[string]interface{} {
	return map[string]interface{}{
		"type":         "message",
		"id":           msg.ID,
		"timestamp":    msg.Timestamp,
		"from":         msg.From,
		"chat":         msg.Chat,
		"content":      msg.Content,
		"is_from_me":   msg.IsFromMe,
		"is_forwarded": msg.IsForwarded,
		"push_name":    msg.PushName,
	}
}

func NewErrorEvent(code, message string) ErrorEvent {
	return ErrorEvent{Type: "error", Code: code, Message: message}
}

func NewLogEvent(level, message string) LogEvent {
	return LogEvent{Type: "log", Level: level, Message: message}
}

func NewLoggedOutEvent(reason string) LoggedOutEvent {
	return LoggedOutEvent{Type: "logged_out", Reason: reason}
}

func NewSendResultEvent(requestID int, success bool, messageID string, timestamp int64, errMsg string) SendResultEvent {
	return SendResultEvent{
		Type:      "send_result",
		RequestID: requestID,
		Success:   success,
		MessageID: messageID,
		Timestamp: timestamp,
		Error:     errMsg,
	}
}

func NewProfilePictureEvent(requestID int, jid, url, id, errMsg string) ProfilePictureEvent {
	return ProfilePictureEvent{
		Type:      "profile_picture",
		RequestID: requestID,
		JID:       jid,
		URL:       url,
		ID:        id,
		Error:     errMsg,
	}
}

func NewChatPresenceEvent(chatID, userID, state string) ChatPresenceEvent {
	return ChatPresenceEvent{
		Type:   "chat_presence",
		ChatID: chatID,
		UserID: userID,
		State:  state,
	}
}

// SendEvent marshals an event to JSON and prints it to stdout
func SendEvent(event interface{}) {
	data, err := json.Marshal(event)
	if err != nil {
		// If we can't marshal, send an error event the hard way
		errJSON := `{"type":"error","code":"marshal_error","message":"failed to marshal event"}`
		fmt.Println(errJSON)
		return
	}
	fmt.Println(string(data))
}

// Helper to get current timestamp
func Now() int64 {
	return time.Now().Unix()
}
