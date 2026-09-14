package main

import (
	"testing"

	"go.mau.fi/whatsmeow/types"
)

func TestPrivateConversationUsesAccountAfterDevicePhoneResolution(t *testing.T) {
	for _, device := range []uint8{17, 22} {
		phone := types.NewADJID("447700900123", 0, device)
		lid := types.NewADJID("900000000001", 1, device)
		for _, outgoing := range []bool{false, true} {
			info := types.MessageInfo{MessageSource: types.MessageSource{
				Chat: lid.ToNonAD(), IsFromMe: outgoing, SenderAlt: phone, RecipientAlt: phone,
			}}
			if got := normalizePrivateChatJID(info).String(); got != "447700900123@s.whatsapp.net" {
				t.Fatalf("device %d outgoing %t: got %s", device, outgoing, got)
			}
		}
		if got := normalizeAddress(phone, types.JID{}).String(); got != "447700900123@s.whatsapp.net" {
			t.Fatalf("direct phone address: %s", got)
		}
		if got := normalizeAddress(lid, types.JID{}).String(); got != "900000000001@lid" {
			t.Fatalf("unmapped LID must retain its namespace: %s", got)
		}
	}
}

func TestConversationNormalizationPreservesNonPersonAddresses(t *testing.T) {
	for _, raw := range []string{"123-456@g.us", "status@broadcast", "123@broadcast", "123@newsletter"} {
		jid, err := types.ParseJID(raw)
		if err != nil {
			t.Fatal(err)
		}
		if got := normalizeAddress(jid, types.NewJID("447700900123", types.DefaultUserServer)).String(); got != raw {
			t.Fatalf("%s became %s", raw, got)
		}
	}
	lid := types.NewJID("900000000001", types.HiddenUserServer)
	if got := normalizeAddress(lid, types.NewJID("123-456", types.GroupServer)); got != lid {
		t.Fatal("an unrelated alternate namespace must not replace a person")
	}
}
