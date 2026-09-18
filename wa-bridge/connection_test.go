package main

import (
	"go.mau.fi/whatsmeow/types/events"
	"testing"
)

func TestConnectionRecoveryDoesNotRelinkTemporaryFailures(t *testing.T) {
	for _, event := range []interface{}{&events.Disconnected{}, &events.KeepAliveTimeout{}, &events.StreamReplaced{}} {
		updates := connectionEvents(event)
		if len(updates) == 0 {
			t.Fatalf("missing recovery for %T", event)
		}
		if state, ok := updates[0].(ConnectionStateEvent); !ok || state.State != "reconnecting" {
			t.Fatalf("expected reconnect for %T, got %#v", event, updates)
		}
		for _, update := range updates {
			if _, logout := update.(LoggedOutEvent); logout {
				t.Fatalf("must preserve session for %T", event)
			}
		}
	}
	replaced := connectionEvents(&events.StreamReplaced{})
	if replaced[1].(ErrorEvent).Code != "reconnect_required" {
		t.Fatal("replaced stream must restart the bridge")
	}
}

func TestRevokedSessionRequiresLinkingButBansDoNotResetKeys(t *testing.T) {
	updates := connectionEvents(&events.LoggedOut{Reason: events.ConnectFailureLoggedOut})
	if _, ok := updates[0].(LoggedOutEvent); !ok {
		t.Fatal("revoked session must trigger linking")
	}
	for _, event := range []interface{}{&events.TemporaryBan{}, &events.ClientOutdated{}} {
		updates := connectionEvents(event)
		if updates[0].(ConnectionStateEvent).State != "disconnected" {
			t.Fatalf("must report failure for %T", event)
		}
		if updates[1].(ErrorEvent).Code != "connection_failed" {
			t.Fatal("must not retry a permanent restriction")
		}
	}
}
