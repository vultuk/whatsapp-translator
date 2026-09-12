import XCTest
#if os(macOS)
@testable import BabelBridgeMac
#else
@testable import WhatsAppTranslator
#endif

@MainActor
final class ConversationTranslationTests: XCTestCase {
    func testMissingTranslationSettingDefaultsOffAndSavesExplicitly() throws {
        let legacy = try JSONDecoder().decode(ConversationSettings.self, from: Data(#"{"languageOverride":"Hungarian","translationStyle":"friendly","sendOriginalFollowUp":true}"#.utf8))
        XCTAssertFalse(legacy.translationEnabled)
        XCTAssertEqual(legacy.languageOverride, "Hungarian")
        XCTAssertTrue(legacy.sendOriginalFollowUp)
        let encoded = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(legacy)) as? [String: Any])
        XCTAssertEqual(encoded["translationEnabled"] as? Bool, false)
    }

    func testPeopleAndGroupsKeepIndependentSettingsInChatsAndUnifiedMessages() async throws {
        let session = AppSession(demoMode: true)
        await session.start()
        let personID = "person@s.whatsapp.net"
        let groupID = "family@g.us"
        let person = try await session.conversationSettings(for: personID)
        let group = try await session.conversationSettings(for: groupID)
        XCTAssertFalse(person.translationEnabled)
        XCTAssertFalse(group.translationEnabled)
        var enabled = ConversationSettings(languageOverride: "Hungarian", translationStyle: "friendly", translationEnabled: true)
        try await session.saveConversationSettings(enabled, for: groupID)
        session.mainTab = .messages
        let groupFromMessages = try await session.conversationSettings(for: groupID)
        XCTAssertEqual(groupFromMessages, enabled)
        let personFromMessages = try await session.conversationSettings(for: personID)
        XCTAssertFalse(personFromMessages.translationEnabled)
        enabled.translationEnabled = false
        try await session.saveConversationSettings(enabled, for: groupID)
        session.mainTab = .chats
        let restored = try await session.conversationSettings(for: groupID)
        XCTAssertFalse(restored.translationEnabled)
        XCTAssertEqual(restored.languageOverride, "Hungarian")
        XCTAssertEqual(restored.translationStyle, "friendly")
    }

    func testSettingsEventsRefreshOnlyTheNamedConversation() async throws {
        let session = AppSession(demoMode: true)
        await session.start()
        let event = try JSONDecoder().decode(LiveEvent.self, from: Data(#"{"type":"conversation_settings_updated","chat_id":"family@g.us","settings":{"translationEnabled":true,"languageOverride":"Hungarian","translationStyle":null,"sendOriginalFollowUp":false}}"#.utf8))
        session.handle(event)
        XCTAssertEqual(session.savedConversationSettings["family@g.us"]?.translationEnabled, true)
        XCTAssertNil(session.savedConversationSettings["person@s.whatsapp.net"])
        session.forgetServer()
        XCTAssertTrue(session.savedConversationSettings.isEmpty)
    }

    func testOriginalVoicePreparationDoesNotPresentAsAITranslation() throws {
        let raw = #"{"id":"note","contactId":"family@g.us","transcript":"","translation":"","targetLanguage":"","voice":"original","audioData":"Zml4dHVyZQ==","originalData":"Zml4dHVyZQ==","mimeType":"audio/mpeg","durationSeconds":1,"originalFollowUp":false,"isTranslated":false}"#
        let note = try JSONDecoder().decode(TranslatedVoiceNote.self, from: Data(raw.utf8))
        XCTAssertFalse(note.usesTranslation)
        XCTAssertFalse(note.originalFollowUp)
        var legacy = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String: Any])
        legacy.removeValue(forKey: "isTranslated")
        // Older cached preparations predate original-only voice sends.
        let translated = try JSONDecoder().decode(TranslatedVoiceNote.self, from: JSONSerialization.data(withJSONObject: legacy))
        XCTAssertTrue(translated.usesTranslation)
    }
}
