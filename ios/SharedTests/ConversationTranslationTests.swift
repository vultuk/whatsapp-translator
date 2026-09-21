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

@MainActor
final class ReplyOnlyTests: XCTestCase {
    func testSettingDefaultsOffAndEncodesExplicitChoice() throws {
        let old = try JSONDecoder().decode(ConversationSettings.self, from: Data("{}".utf8))
        XCTAssertFalse(old.replyOnly)
        let enabled = ConversationSettings(replyOnly: true)
        XCTAssertEqual(try JSONDecoder().decode(ConversationSettings.self, from: JSONEncoder().encode(enabled)), enabled)
    }

    func testOnlyIncomingRepliesInTheSelectedConversationCanSend() async throws {
        let session = AppSession(demoMode: true)
        await session.start()
        let incoming = try XCTUnwrap(session.messages.values.joined().first { !$0.isFromMe && !$0.isReaction })
        let id = incoming.contactId
        try await session.saveConversationSettings(ConversationSettings(replyOnly: true), for: id)
        XCTAssertFalse(session.canSend(to: id, reply: nil))
        XCTAssertTrue(session.canSend(to: id, reply: incoming.replyTarget))
        XCTAssertTrue(session.feedReplyNeedsQuote(incoming))
        let blocked = await session.send(text: "Unprompted", to: id)
        XCTAssertFalse(blocked)
        XCTAssertFalse(session.startPhotoSend([OutgoingImage(data: Data([1]), mimeType: "image/jpeg")], to: id))
        let sent = await session.send(text: "A considered reply", to: id, reply: incoming.replyTarget, replyOnlyIfNotLatest: true)
        XCTAssertTrue(sent)
        let outgoing = try XCTUnwrap(session.messages[id]?.last)
        XCTAssertFalse(session.canSend(to: id, reply: outgoing.replyTarget))
        XCTAssertTrue(session.canSend(to: "unrestricted@g.us", reply: nil))
        try await session.saveConversationSettings(ConversationSettings(replyOnly: false), for: id)
        XCTAssertTrue(session.canSend(to: id, reply: nil))
    }

    func testAutomaticFeedDestinationIsNotAnExplicitReplyAndCancelClearsIntent() async throws {
        let session = AppSession(demoMode: true)
        await session.start()
        let incoming = try XCTUnwrap(session.messages.values.joined().first { !$0.isFromMe })
        var draft = UnifiedReplyDraft()
        draft.updateText("Draft", latestMessage: incoming)
        XCTAssertFalse(draft.isExplicitReply)
        draft.cancelSelection()
        XCTAssertNotNil(draft.beginAttachment(latestMessage: incoming))
        XCTAssertFalse(draft.isExplicitReply)
        draft.select(incoming)
        XCTAssertTrue(draft.isExplicitReply)
        draft.finishMediaSending(to: incoming)
        XCTAssertFalse(draft.isExplicitReply)
        XCTAssertEqual(draft.drafts[incoming.contactId], "Draft")
    }

    func testLiveReplyOnlySettingRefreshesWithoutRelaunch() async throws {
        let session = AppSession(demoMode: true)
        await session.start()
        let event = try JSONDecoder().decode(LiveEvent.self, from: Data(#"{"type":"conversation_settings_updated","chat_id":"family@g.us","settings":{"replyOnly":true}}"#.utf8))
        session.handle(event)
        XCTAssertTrue(session.isReplyOnly("family@g.us"))
        XCTAssertFalse(session.canSend(to: "family@g.us", reply: nil))
        XCTAssertFalse(session.isReplyOnly("other@g.us"))
    }
}
