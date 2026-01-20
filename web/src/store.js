import Database from 'better-sqlite3';
import { join } from 'path';
import { mkdirSync } from 'fs';

/**
 * SQLite-backed message store
 * Persists contacts and messages to disk
 */
export class MessageStore {
  constructor(dataDir) {
    // Ensure data directory exists
    mkdirSync(dataDir, { recursive: true });
    
    const dbPath = join(dataDir, 'messages.db');
    this.db = new Database(dbPath);
    
    // Enable WAL mode for better concurrent performance
    this.db.pragma('journal_mode = WAL');
    
    this.initSchema();
    this.prepareStatements();
  }

  /**
   * Initialize database schema
   */
  initSchema() {
    this.db.exec(`
      -- Contacts table
      CREATE TABLE IF NOT EXISTS contacts (
        id TEXT PRIMARY KEY,
        name TEXT,
        phone TEXT,
        type TEXT,
        last_message_time INTEGER DEFAULT 0,
        unread_count INTEGER DEFAULT 0
      );

      -- Messages table
      CREATE TABLE IF NOT EXISTS messages (
        id TEXT PRIMARY KEY,
        contact_id TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        is_from_me INTEGER NOT NULL,
        is_forwarded INTEGER DEFAULT 0,
        sender_name TEXT,
        sender_phone TEXT,
        content_type TEXT NOT NULL,
        content_json TEXT NOT NULL,
        chat_type TEXT,
        FOREIGN KEY (contact_id) REFERENCES contacts(id)
      );

      -- Indexes for faster queries
      CREATE INDEX IF NOT EXISTS idx_messages_contact_id ON messages(contact_id);
      CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
      CREATE INDEX IF NOT EXISTS idx_contacts_last_message ON contacts(last_message_time DESC);
    `);
  }

  /**
   * Prepare reusable SQL statements for better performance
   */
  prepareStatements() {
    this.stmts = {
      // Contact statements
      upsertContact: this.db.prepare(`
        INSERT INTO contacts (id, name, phone, type, last_message_time, unread_count)
        VALUES (@id, @name, @phone, @type, @lastMessageTime, @unreadCount)
        ON CONFLICT(id) DO UPDATE SET
          name = COALESCE(
            CASE WHEN excluded.name IS NOT NULL AND excluded.name != excluded.phone THEN excluded.name ELSE NULL END,
            contacts.name
          ),
          phone = COALESCE(excluded.phone, contacts.phone),
          type = COALESCE(excluded.type, contacts.type),
          last_message_time = MAX(contacts.last_message_time, excluded.last_message_time)
      `),
      
      getContact: this.db.prepare('SELECT * FROM contacts WHERE id = ?'),
      
      getAllContacts: this.db.prepare(`
        SELECT * FROM contacts ORDER BY last_message_time DESC
      `),
      
      incrementUnread: this.db.prepare(`
        UPDATE contacts SET unread_count = unread_count + 1 WHERE id = ?
      `),
      
      resetUnread: this.db.prepare(`
        UPDATE contacts SET unread_count = 0 WHERE id = ?
      `),
      
      // Message statements
      insertMessage: this.db.prepare(`
        INSERT OR IGNORE INTO messages 
        (id, contact_id, timestamp, is_from_me, is_forwarded, sender_name, sender_phone, content_type, content_json, chat_type)
        VALUES (@id, @contactId, @timestamp, @isFromMe, @isForwarded, @senderName, @senderPhone, @contentType, @contentJson, @chatType)
      `),
      
      getMessages: this.db.prepare(`
        SELECT * FROM messages WHERE contact_id = ? ORDER BY timestamp ASC
      `),
      
      getRecentMessages: this.db.prepare(`
        SELECT * FROM messages WHERE contact_id = ? ORDER BY timestamp DESC LIMIT ?
      `),
      
      messageExists: this.db.prepare('SELECT 1 FROM messages WHERE id = ?'),
      
      // Stats
      getTotalUnread: this.db.prepare('SELECT SUM(unread_count) as total FROM contacts'),
      
      getMessageCount: this.db.prepare('SELECT COUNT(*) as count FROM messages'),
      
      getContactCount: this.db.prepare('SELECT COUNT(*) as count FROM contacts'),
    };
  }

  /**
   * Add a message to the store
   */
  addMessage(message) {
    const { 
      id, contactId, contactName, contactPhone, chatType, timestamp,
      isFromMe, isForwarded, senderName, senderPhone, content 
    } = message;

    // Use a transaction for atomicity
    const transaction = this.db.transaction(() => {
      // Upsert contact
      this.stmts.upsertContact.run({
        id: contactId,
        name: contactName,
        phone: contactPhone,
        type: chatType,
        lastMessageTime: timestamp,
        unreadCount: 0
      });

      // Increment unread count for incoming messages
      if (!isFromMe) {
        this.stmts.incrementUnread.run(contactId);
      }

      // Insert message
      this.stmts.insertMessage.run({
        id,
        contactId,
        timestamp,
        isFromMe: isFromMe ? 1 : 0,
        isForwarded: isForwarded ? 1 : 0,
        senderName,
        senderPhone,
        contentType: content.type,
        contentJson: JSON.stringify(content),
        chatType
      });
    });

    transaction();
  }

  /**
   * Get all contacts sorted by last message time
   */
  getContacts() {
    const rows = this.stmts.getAllContacts.all();
    return rows.map(row => ({
      id: row.id,
      name: row.name,
      phone: row.phone,
      type: row.type,
      lastMessageTime: row.last_message_time,
      unreadCount: row.unread_count
    }));
  }

  /**
   * Get messages for a specific contact
   */
  getMessages(contactId) {
    const rows = this.stmts.getMessages.all(contactId);
    return rows.map(this.rowToMessage);
  }

  /**
   * Get recent messages for a contact (for pagination)
   */
  getRecentMessages(contactId, limit = 50) {
    const rows = this.stmts.getRecentMessages.all(contactId, limit);
    // Reverse to get chronological order
    return rows.reverse().map(this.rowToMessage);
  }

  /**
   * Convert a database row to a message object
   */
  rowToMessage(row) {
    return {
      id: row.id,
      contactId: row.contact_id,
      timestamp: row.timestamp,
      isFromMe: row.is_from_me === 1,
      isForwarded: row.is_forwarded === 1,
      senderName: row.sender_name,
      senderPhone: row.sender_phone,
      chatType: row.chat_type,
      content: JSON.parse(row.content_json)
    };
  }

  /**
   * Mark messages as read for a contact
   */
  markAsRead(contactId) {
    this.stmts.resetUnread.run(contactId);
  }

  /**
   * Get a contact by ID
   */
  getContact(contactId) {
    const row = this.stmts.getContact.get(contactId);
    if (!row) return null;
    
    return {
      id: row.id,
      name: row.name,
      phone: row.phone,
      type: row.type,
      lastMessageTime: row.last_message_time,
      unreadCount: row.unread_count
    };
  }

  /**
   * Get total unread count
   */
  getTotalUnread() {
    const result = this.stmts.getTotalUnread.get();
    return result?.total || 0;
  }

  /**
   * Get database statistics
   */
  getStats() {
    return {
      messageCount: this.stmts.getMessageCount.get().count,
      contactCount: this.stmts.getContactCount.get().count,
      totalUnread: this.getTotalUnread()
    };
  }

  /**
   * Close the database connection
   */
  close() {
    this.db.close();
  }
}
