// wa-bridge is a WhatsApp Web bridge that communicates with the Rust CLI via JSON-lines over stdio.
//
// It uses the whatsmeow library to connect to WhatsApp and forwards events to the parent process.
package main

import (
	"bufio"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"syscall"
)

func main() {
	// Parse command line arguments
	dataDir := flag.String("data-dir", "", "Directory for storing session data")
	verbose := flag.Bool("verbose", false, "Enable verbose logging")
	flag.Parse()

	if *dataDir == "" {
		SendEvent(NewErrorEvent("config", "data-dir is required"))
		os.Exit(1)
	}

	// Ensure data directory exists
	if err := os.MkdirAll(*dataDir, 0700); err != nil {
		SendEvent(NewErrorEvent("filesystem", fmt.Sprintf("failed to create data directory: %v", err)))
		os.Exit(1)
	}

	// Set up context with cancellation
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Create client
	client, err := NewClient(ctx, *dataDir, *verbose)
	if err != nil {
		SendEvent(NewErrorEvent("init", fmt.Sprintf("failed to create client: %v", err)))
		os.Exit(1)
	}
	defer client.Close()

	// Handle shutdown signals
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, os.Interrupt, syscall.SIGTERM)
	go func() {
		<-sigChan
		SendEvent(NewLogEvent("info", "Received shutdown signal"))
		cancel()
	}()

	// Start command reader goroutine
	go readCommands(ctx, client, cancel)

	// Connect to WhatsApp
	SendEvent(NewConnectionStateEvent("connecting"))
	if err := client.Connect(ctx); err != nil {
		SendEvent(NewErrorEvent("connect", fmt.Sprintf("failed to connect: %v", err)))
		os.Exit(1)
	}

	// Wait for context cancellation (shutdown)
	<-ctx.Done()

	// Clean shutdown
	SendEvent(NewLogEvent("info", "Disconnecting..."))
	client.Disconnect()
}

// readCommands reads commands from stdin and processes them
func readCommands(ctx context.Context, client *Client, cancel context.CancelFunc) {
	scanner := bufio.NewScanner(os.Stdin)

	// Read commands in a separate goroutine so we can also watch for context cancellation
	lineChan := make(chan string)
	errChan := make(chan error)

	go func() {
		for scanner.Scan() {
			lineChan <- scanner.Text()
		}
		if err := scanner.Err(); err != nil {
			errChan <- err
		} else {
			errChan <- nil // EOF
		}
	}()

	for {
		select {
		case <-ctx.Done():
			return
		case err := <-errChan:
			// stdin closed or error
			if err != nil {
				SendEvent(NewLogEvent("warn", fmt.Sprintf("stdin error: %v", err)))
			} else {
				SendEvent(NewLogEvent("info", "stdin closed, shutting down"))
			}
			cancel()
			return
		case line := <-lineChan:
			if line == "" {
				continue
			}

			var cmd Command
			if err := json.Unmarshal([]byte(line), &cmd); err != nil {
				SendEvent(NewLogEvent("warn", fmt.Sprintf("failed to parse command: %v", err)))
				continue
			}

			handleCommand(ctx, client, cmd, cancel)
		}
	}
}

// handleCommand processes a single command
func handleCommand(ctx context.Context, client *Client, cmd Command, cancel context.CancelFunc) {
	switch cmd.Type {
	case "disconnect":
		SendEvent(NewLogEvent("info", "Received disconnect command"))
		cancel()

	case "logout":
		SendEvent(NewLogEvent("info", "Received logout command"))
		if err := client.Logout(ctx); err != nil {
			SendEvent(NewErrorEvent("logout", fmt.Sprintf("failed to logout: %v", err)))
		}
		cancel()

	case "send":
		if cmd.To == "" || cmd.Text == "" {
			SendEvent(NewSendResultEvent(cmd.RequestID, false, "", 0, "missing 'to' or 'text' field"))
			return
		}

		messageID, timestamp, err := client.SendTextMessage(ctx, cmd.To, cmd.Text)
		if err != nil {
			SendEvent(NewSendResultEvent(cmd.RequestID, false, "", 0, err.Error()))
		} else {
			SendEvent(NewSendResultEvent(cmd.RequestID, true, messageID, timestamp, ""))
		}

	case "get_profile_picture":
		if cmd.To == "" {
			SendEvent(NewProfilePictureEvent(cmd.RequestID, "", "", "", "missing 'to' field"))
			return
		}

		url, id, err := client.GetProfilePicture(ctx, cmd.To)
		if err != nil {
			SendEvent(NewProfilePictureEvent(cmd.RequestID, cmd.To, "", "", err.Error()))
		} else {
			SendEvent(NewProfilePictureEvent(cmd.RequestID, cmd.To, url, id, ""))
		}

	default:
		SendEvent(NewLogEvent("warn", fmt.Sprintf("Unknown command type: %s", cmd.Type)))
	}
}
