import express from 'express';
import { createServer } from 'http';
import { WebSocketServer } from 'ws';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { homedir } from 'os';
import { BridgeManager } from './bridge.js';
import { MessageStore } from './store.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const app = express();
const server = createServer(app);
const wss = new WebSocketServer({ server });

// Data directory for SQLite database
const dataDir = process.env.WA_DATA_DIR || join(homedir(), '.local/share/whatsapp-translator');

// Initialize stores
const messageStore = new MessageStore(dataDir);
const bridgeManager = new BridgeManager(messageStore);

// Track WebSocket clients
const clients = new Set();

// CORS middleware for external access
app.use((req, res, next) => {
  res.header('Access-Control-Allow-Origin', '*');
  res.header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  res.header('Access-Control-Allow-Headers', 'Content-Type');
  if (req.method === 'OPTIONS') {
    return res.sendStatus(200);
  }
  next();
});

// Serve static files
app.use(express.static(join(__dirname, '../public')));
app.use(express.json());

// REST API endpoints

// Get connection status
app.get('/api/status', (req, res) => {
  res.json({
    connected: bridgeManager.isConnected(),
    phone: bridgeManager.getPhone(),
    name: bridgeManager.getName()
  });
});

// Get all contacts (sorted by last message time)
app.get('/api/contacts', (req, res) => {
  const contacts = messageStore.getContacts();
  res.json(contacts);
});

// Get messages for a specific contact
app.get('/api/messages/:contactId', (req, res) => {
  const contactId = decodeURIComponent(req.params.contactId);
  const messages = messageStore.getMessages(contactId);
  res.json(messages);
});

// Get QR code data
app.get('/api/qr', (req, res) => {
  const qrData = bridgeManager.getQRCode();
  if (qrData) {
    res.json({ qr: qrData });
  } else {
    res.json({ qr: null });
  }
});

// Send a message
app.post('/api/send', async (req, res) => {
  const { contactId, text } = req.body;
  
  if (!contactId || !text) {
    return res.status(400).json({ error: 'contactId and text are required' });
  }
  
  if (!bridgeManager.isConnected()) {
    return res.status(503).json({ error: 'Not connected to WhatsApp' });
  }
  
  try {
    const result = await bridgeManager.sendMessage(contactId, text);
    
    // Store the sent message in the database
    const contact = messageStore.getContact(contactId);
    const sentMessage = {
      id: result.messageId,
      contactId: contactId,
      contactName: contact?.name || null,
      contactPhone: contact?.phone || null,
      chatType: contact?.type || 'private',
      timestamp: result.timestamp * 1000, // Convert to milliseconds
      isFromMe: true,
      isForwarded: false,
      senderName: bridgeManager.getName(),
      senderPhone: bridgeManager.getPhone(),
      content: { type: 'text', text: text }
    };
    
    messageStore.addMessage(sentMessage);
    
    // Broadcast to other connected clients
    broadcast({ type: 'message', message: sentMessage });
    
    res.json(result);
  } catch (err) {
    console.error('Failed to send message:', err);
    res.status(500).json({ error: err.message });
  }
});

// Get database stats
app.get('/api/stats', (req, res) => {
  res.json(messageStore.getStats());
});

// WebSocket handling
wss.on('connection', (ws) => {
  console.log('WebSocket client connected');
  clients.add(ws);

  // Send current status
  ws.send(JSON.stringify({
    type: 'status',
    connected: bridgeManager.isConnected(),
    phone: bridgeManager.getPhone(),
    name: bridgeManager.getName()
  }));

  // Send current QR if available
  const qr = bridgeManager.getQRCode();
  if (qr) {
    ws.send(JSON.stringify({ type: 'qr', data: qr }));
  }

  ws.on('close', () => {
    console.log('WebSocket client disconnected');
    clients.delete(ws);
  });

  ws.on('error', (err) => {
    console.error('WebSocket error:', err);
    clients.delete(ws);
  });
});

// Broadcast to all WebSocket clients
function broadcast(message) {
  const data = JSON.stringify(message);
  for (const client of clients) {
    if (client.readyState === 1) { // WebSocket.OPEN
      client.send(data);
    }
  }
}

// Bridge event handlers
bridgeManager.on('qr', (data) => {
  broadcast({ type: 'qr', data });
});

bridgeManager.on('connected', (info) => {
  broadcast({ type: 'connected', ...info });
});

bridgeManager.on('disconnected', () => {
  broadcast({ type: 'disconnected' });
});

bridgeManager.on('message', (message) => {
  broadcast({ type: 'message', message });
});

bridgeManager.on('error', (error) => {
  broadcast({ type: 'error', error });
});

// Start server
const PORT = process.env.PORT || 3000;
const HOST = process.env.HOST || '0.0.0.0';

server.listen(PORT, HOST, () => {
  console.log(`Web server running at http://${HOST}:${PORT}`);
  
  // Start the bridge
  bridgeManager.start().catch(err => {
    console.error('Failed to start bridge:', err);
  });
});

// Graceful shutdown
process.on('SIGINT', async () => {
  console.log('\nShutting down...');
  await bridgeManager.stop();
  server.close();
  process.exit(0);
});

process.on('SIGTERM', async () => {
  await bridgeManager.stop();
  server.close();
  process.exit(0);
});
