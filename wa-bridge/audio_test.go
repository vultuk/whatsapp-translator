package main

import (
	"context"
	"encoding/base64"
	"testing"
)

func TestSendAudioRejectsInvalidInputBeforeUpload(t *testing.T) {
	c := &Client{} // No network client: invalid data must fail before upload.
	for _, data := range []string{"", "not-base64", base64.StdEncoding.EncodeToString([]byte("not an ogg file even though it is long enough"))} {
		if _, _, err := c.SendAudioMessage(context.Background(), "123@s.whatsapp.net", data, 2, "", "", ""); err == nil {
			t.Fatal("invalid audio was accepted")
		}
	}
}
