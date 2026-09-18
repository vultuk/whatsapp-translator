package main

import "go.mau.fi/whatsmeow/types/events"

// Only a revoked session requires new keys. A dropped socket or a second
// connection using the same keys must never delete the saved WhatsApp session.
func connectionEvents(event interface{}) []interface{} {
	switch event := event.(type) {
	case *events.LoggedOut:
		return []interface{}{NewLoggedOutEvent(event.Reason.String())}
	case *events.Disconnected, *events.KeepAliveTimeout:
		return []interface{}{NewConnectionStateEvent("reconnecting")}
	case *events.StreamReplaced:
		return []interface{}{NewConnectionStateEvent("reconnecting"), NewErrorEvent("reconnect_required", "WhatsApp connection was replaced; reconnecting with the saved session")}
	case events.PermanentDisconnect:
		return []interface{}{NewConnectionStateEvent("disconnected"), NewErrorEvent("connection_failed", event.PermanentDisconnectDescription())}
	default:
		return nil
	}
}
