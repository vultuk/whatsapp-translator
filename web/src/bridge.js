import { spawn } from 'child_process';
import { EventEmitter } from 'events';
import { existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { homedir } from 'os';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

/**
 * Manages the wa-bridge subprocess and handles JSON-line communication
 */
export class BridgeManager extends EventEmitter {
  constructor(messageStore) {
    super();
    this.messageStore = messageStore;
    this.process = null;
    this.connected = false;
    this.phone = null;
    this.name = null;
    this.qrCode = null;
    this.pendingSends = new Map(); // Track pending send requests
    this.sendIdCounter = 0;
  }

  /**
   * Find the wa-bridge binary
   */
  findBridge() {
    const possiblePaths = [
      join(__dirname, '../../wa-bridge/wa-bridge'),
      join(__dirname, '../../target/release/wa-bridge'),
      join(__dirname, '../../target/debug/wa-bridge'),
    ];

    for (const p of possiblePaths) {
      if (existsSync(p)) {
        return p;
      }
    }

    throw new Error('wa-bridge binary not found. Please build it first.');
  }

  /**
   * Get data directory for session storage
   */
  getDataDir() {
    const dataDir = process.env.WA_DATA_DIR || join(homedir(), '.local/share/whatsapp-translator');
    return dataDir;
  }

  /**
   * Start the bridge process
   */
  async start() {
    if (this.process) {
      console.log('Bridge already running');
      return;
    }

    const bridgePath = this.findBridge();
    const dataDir = this.getDataDir();

    console.log(`Starting bridge: ${bridgePath}`);
    console.log(`Data directory: ${dataDir}`);

    this.process = spawn(bridgePath, ['--data-dir', dataDir], {
      stdio: ['pipe', 'pipe', 'pipe']
    });

    // Keep stdin open - the Go bridge exits if stdin closes
    this.process.stdin.setDefaultEncoding('utf8');
    
    // Handle stdout (JSON events)
    let buffer = '';
    this.process.stdout.on('data', (data) => {
      buffer += data.toString();
      const lines = buffer.split('\n');
      buffer = lines.pop(); // Keep incomplete line in buffer

      for (const line of lines) {
        if (line.trim()) {
          this.handleEvent(line);
        }
      }
    });

    // Handle stderr (logs)
    this.process.stderr.on('data', (data) => {
      // Just log to console, don't parse
      process.stderr.write(data);
    });

    this.process.on('close', (code) => {
      console.log(`Bridge process exited with code ${code}`);
      this.process = null;
      this.connected = false;
      this.emit('disconnected');
    });

    this.process.on('error', (err) => {
      console.error('Bridge process error:', err);
      this.emit('error', { message: err.message });
    });
  }

  /**
   * Handle a JSON event from the bridge
   */
  handleEvent(line) {
    try {
      const event = JSON.parse(line);
      
      switch (event.type) {
        case 'qr':
          this.qrCode = event.data;
          this.emit('qr', event.data);
          break;

        case 'connected':
          this.connected = true;
          this.phone = event.phone;
          this.name = event.name;
          this.qrCode = null; // Clear QR code
          this.emit('connected', { phone: event.phone, name: event.name });
          break;

        case 'connection_state':
          if (event.state === 'disconnected') {
            this.connected = false;
            this.emit('disconnected');
          }
          break;

        case 'message':
          // Store the message
          const msg = this.parseMessage(event);
          if (msg) {
            this.messageStore.addMessage(msg);
            this.emit('message', msg);
          }
          break;

        case 'logged_out':
          this.connected = false;
          this.phone = null;
          this.name = null;
          this.emit('disconnected');
          break;

        case 'error':
          console.error('Bridge error:', event.code, event.message);
          this.emit('error', { code: event.code, message: event.message });
          break;

        case 'send_result':
          this.handleSendResult(event);
          break;

        case 'log':
          if (event.level === 'error') {
            console.error('[bridge]', event.message);
          } else if (event.level === 'warn') {
            console.warn('[bridge]', event.message);
          } else if (event.level === 'info') {
            console.log('[bridge]', event.message);
          }
          // Don't emit log events to frontend
          break;

        default:
          // Ignore unknown event types
          break;
      }
    } catch (err) {
      // Ignore parse errors (might be debug output)
    }
  }

  /**
   * Parse a message event into a normalized format
   */
  parseMessage(event) {
    // The message structure from the bridge
    const { id, timestamp, from, chat, content, is_from_me, is_forwarded, push_name } = event;

    if (!from || !chat || !content) {
      return null;
    }

    // Determine contact ID (the other party in the conversation)
    let contactId, contactName, contactPhone;
    
    if (chat.type === 'group') {
      contactId = chat.jid;
      contactName = chat.name || 'Unknown Group';
      contactPhone = null;
    } else {
      // For private chats, the contact is the other person
      contactId = chat.jid;
      contactPhone = from.phone;
      contactName = push_name || from.name || from.phone;
    }

    return {
      id,
      timestamp: timestamp * 1000, // Convert to milliseconds
      contactId,
      contactName,
      contactPhone,
      chatType: chat.type,
      isFromMe: is_from_me,
      isForwarded: is_forwarded,
      senderName: push_name || from.name || from.phone,
      senderPhone: from.phone,
      content: this.formatContent(content)
    };
  }

  /**
   * Format message content for display
   */
  formatContent(content) {
    switch (content.type) {
      case 'text':
        return { type: 'text', text: content.body };
      
      case 'image':
        return { 
          type: 'image', 
          caption: content.caption,
          mimeType: content.mime_type,
          size: content.file_size
        };
      
      case 'video':
        return { 
          type: 'video', 
          caption: content.caption,
          duration: content.duration_seconds,
          size: content.file_size
        };
      
      case 'audio':
        return { 
          type: 'audio', 
          duration: content.duration_seconds,
          isVoiceNote: content.is_voice_note
        };
      
      case 'document':
        return { 
          type: 'document', 
          fileName: content.file_name,
          caption: content.caption,
          size: content.file_size
        };
      
      case 'sticker':
        return { type: 'sticker', animated: content.is_animated };
      
      case 'location':
        return { 
          type: 'location', 
          latitude: content.latitude,
          longitude: content.longitude,
          name: content.name,
          address: content.address
        };
      
      case 'contact':
        return { type: 'contact', name: content.display_name };
      
      case 'reaction':
        return { type: 'reaction', emoji: content.emoji };
      
      case 'revoked':
        return { type: 'revoked' };
      
      case 'poll':
        return { type: 'poll', question: content.question, options: content.options };
      
      default:
        return { type: 'unknown', rawType: content.type || content.raw_type };
    }
  }

  /**
   * Stop the bridge process
   */
  async stop() {
    if (!this.process) return;

    return new Promise((resolve) => {
      // Send disconnect command
      try {
        this.process.stdin.write(JSON.stringify({ type: 'disconnect' }) + '\n');
      } catch (e) {
        // Ignore write errors
      }

      // Wait for graceful shutdown or force kill
      const timeout = setTimeout(() => {
        if (this.process) {
          this.process.kill('SIGKILL');
        }
        resolve();
      }, 5000);

      this.process.once('close', () => {
        clearTimeout(timeout);
        resolve();
      });
    });
  }

  /**
   * Send a text message
   */
  async sendMessage(contactId, text) {
    if (!this.process || !this.connected) {
      throw new Error('Not connected');
    }

    const requestId = ++this.sendIdCounter;
    
    return new Promise((resolve, reject) => {
      // Set up timeout
      const timeout = setTimeout(() => {
        this.pendingSends.delete(requestId);
        reject(new Error('Send timeout'));
      }, 30000);

      // Store the pending request
      this.pendingSends.set(requestId, { resolve, reject, timeout });

      // Send the command to the bridge
      const command = {
        type: 'send',
        request_id: requestId,
        to: contactId,
        text: text
      };

      try {
        this.process.stdin.write(JSON.stringify(command) + '\n');
      } catch (err) {
        clearTimeout(timeout);
        this.pendingSends.delete(requestId);
        reject(err);
      }
    });
  }

  /**
   * Handle send result from bridge
   */
  handleSendResult(event) {
    const { request_id, success, message_id, timestamp, error } = event;
    const pending = this.pendingSends.get(request_id);
    
    if (!pending) {
      return; // Request already timed out or not found
    }

    clearTimeout(pending.timeout);
    this.pendingSends.delete(request_id);

    if (success) {
      pending.resolve({ messageId: message_id, timestamp });
    } else {
      pending.reject(new Error(error || 'Send failed'));
    }
  }

  isConnected() {
    return this.connected;
  }

  getPhone() {
    return this.phone;
  }

  getName() {
    return this.name;
  }

  getQRCode() {
    return this.qrCode;
  }
}
