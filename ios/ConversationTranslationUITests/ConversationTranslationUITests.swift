import XCTest

@MainActor
final class ConversationTranslationUITests: XCTestCase {
    func testTopicsFilterUnifiedAndNormalViewsAndKeepIdenticalNamesSeparate() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoTopics"]
        app.launch()
        let filter = app.buttons["topic-filter"].firstMatch
        XCTAssertTrue(filter.waitForExistence(timeout: 10))
        let before = XCTAttachment(screenshot: app.screenshot()); before.name = "All group discussions in the unified feed"; before.lifetime = .keepAlways; add(before)
        filter.tap()
        let familyTopic = app.buttons["Weekend plans · Family"].firstMatch
        XCTAssertTrue(familyTopic.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Weekend plans · Friends"].exists)
        familyTopic.tap()
        XCTAssertTrue(app.staticTexts["I’ll bring the picnic blanket and sandwiches."].firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["What a goal in last night’s match!"].exists)
        XCTAssertFalse(app.staticTexts["Sunday brunch at eleven?"].exists)
        let filtered = XCTAttachment(screenshot: app.screenshot()); filtered.name = "Unified feed focused on Weekend plans in Family"; filtered.lifetime = .keepAlways; add(filtered)
        app.buttons["Open chat: Family"].firstMatch.tap()
        XCTAssertTrue(app.buttons["topic-filter"].firstMatch.waitForExistence(timeout: 5))
        app.buttons["topic-filter"].firstMatch.tap()
        app.buttons["Football"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["The replay is brilliant too."].firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["I’ll bring the picnic blanket and sandwiches."].exists)
        let conversation = XCTAttachment(screenshot: app.screenshot()); conversation.name = "Conversation focused on the Football topic"; conversation.lifetime = .keepAlways; add(conversation)
        app.buttons["manage-topics"].firstMatch.tap()
        let topicToggle = app.switches["topics-enabled-family@g.us"].firstMatch
        XCTAssertTrue(topicToggle.waitForExistence(timeout: 5))
        XCTAssertEqual(topicToggle.value as? String, "1")
        topicToggle.switches.firstMatch.tap()
        XCTAssertEqual(topicToggle.value as? String, "0")
        app.buttons["Done"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["I’ll bring the picnic blanket and sandwiches."].firstMatch.waitForExistence(timeout: 5))
        let restored = XCTAttachment(screenshot: app.screenshot()); restored.name = "Disabling topics restores all messages"; restored.lifetime = .keepAlways; add(restored)
        app.buttons["manage-topics"].firstMatch.tap()
        let initialImport = app.buttons["import-recent-topics"].firstMatch
        if !initialImport.isHittable { app.swipeUp() }
        XCTAssertTrue(initialImport.waitForExistence(timeout: 5))
        initialImport.tap()
        XCTAssertTrue(app.buttons["Start import"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "5 messages across 2 chats")).firstMatch.exists)
        let preview = XCTAttachment(screenshot: app.screenshot()); preview.name = "Initial import previews seven days across all chats"; preview.lifetime = .keepAlways; add(preview)
        app.buttons["Start import"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "Queued 5 messages across 2 chats")).firstMatch.waitForExistence(timeout: 5))
        app.buttons["Done"].firstMatch.tap()
        app.buttons["topic-filter"].firstMatch.tap()
        XCTAssertTrue(app.buttons["Football"].firstMatch.waitForExistence(timeout: 5))
    }

    func testLiveEditChangesGetToGrrInUnifiedAndNormalViewsWithoutRelaunch() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoLiveEdits"]
        app.launch()
        XCTAssertTrue(app.staticTexts["Get"].firstMatch.waitForExistence(timeout: 5))
        let before = XCTAttachment(screenshot: app.screenshot())
        before.name = "Unified feed before incoming edit"
        before.lifetime = .keepAlways
        add(before)
        XCTAssertTrue(app.staticTexts["Grr"].firstMatch.waitForExistence(timeout: 15))
        XCTAssertFalse(app.staticTexts["Get"].exists)
        XCTAssertTrue(app.staticTexts["Edited"].firstMatch.exists)
        XCTAssertTrue(app.staticTexts["Get what?"].exists)
        let after = XCTAttachment(screenshot: app.screenshot())
        after.name = "Unified feed shows Grr and Edited without relaunch"
        after.lifetime = .keepAlways
        add(after)
        app.buttons["Open chat: Edit preview"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Grr"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Edited"].firstMatch.exists)
        XCTAssertFalse(app.staticTexts["Get"].exists)
        let chat = XCTAttachment(screenshot: app.screenshot())
        chat.name = "Normal conversation shows the same corrected message"
        chat.lifetime = .keepAlways
        add(chat)
    }

    func testLongPressReactionsUpdateUnifiedAndNormalViewsWithoutRelaunch() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoUnifiedFeed"]
        app.launch()
        let target = app.staticTexts["The new draft is ready."].firstMatch
        XCTAssertTrue(target.waitForExistence(timeout: 10))
        target.press(forDuration: 1)
        let heart = app.buttons["React with ❤️"].firstMatch
        XCTAssertTrue(heart.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Reply"].exists)
        XCTAssertTrue(app.buttons["More reactions"].isHittable)
        let menu = XCTAttachment(screenshot: app.screenshot())
        menu.name = "Quick reactions and reply on message long press"
        menu.lifetime = .keepAlways
        add(menu)
        heart.tap()
        let dismissed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: app.buttons["Reply"].firstMatch)
        XCTAssertEqual(XCTWaiter.wait(for: [dismissed], timeout: 5), .completed)
        XCTAssertTrue(app.staticTexts["❤️ 1"].firstMatch.waitForExistence(timeout: 5))
        let visibleGroup = try XCTUnwrap(app.buttons.matching(identifier: "Open chat: Studio team").allElementsBoundByIndex.first(where: { $0.isHittable }))
        visibleGroup.tap()
        XCTAssertTrue(app.staticTexts["❤️ 1"].firstMatch.waitForExistence(timeout: 5))
        target.press(forDuration: 1)
        app.buttons["React with 👍"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["👍 1"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["❤️ 1"].exists)
        target.press(forDuration: 1)
        app.buttons["More reactions"].tap()
        let extra = app.buttons["React with 🎉"].firstMatch
        XCTAssertTrue(extra.waitForExistence(timeout: 5))
        extra.tap()
        XCTAssertTrue(app.staticTexts["🎉 1"].firstMatch.waitForExistence(timeout: 5))
        app.buttons["Messages"].tap()
        XCTAssertTrue(app.staticTexts["🎉 1"].firstMatch.waitForExistence(timeout: 5))
        target.press(forDuration: 1)
        app.buttons["Remove reaction"].tap()
        XCTAssertFalse(app.staticTexts["🎉 1"].waitForExistence(timeout: 1))
        let result = XCTAttachment(screenshot: app.screenshot())
        result.name = "Unified feed updated after reaction removal without relaunch"
        result.lifetime = .keepAlways
        add(result)
    }

    func testGroupTranslationSwitchSavesAcrossUnifiedMessagesAndChatsAndKeepsPeopleOff() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoUnifiedFeed"]
        app.launch()
        let group = app.buttons["Open chat: Studio team"].firstMatch
        XCTAssertTrue(group.waitForExistence(timeout: 10))
        group.press(forDuration: 1)
        app.buttons["Conversation settings"].tap()
        let translation = app.switches["conversation-translation-enabled"].firstMatch
        XCTAssertTrue(translation.waitForExistence(timeout: 5))
        XCTAssertEqual(translation.value as? String, "0")
        translation.switches.firstMatch.tap()
        XCTAssertEqual(translation.value as? String, "1")
        XCTAssertTrue(app.staticTexts["Language"].waitForExistence(timeout: 3))
        app.buttons["Save"].tap()

        // Reopen the same group's setting from the unified feed.
        XCTAssertTrue(group.waitForExistence(timeout: 5))
        group.press(forDuration: 1)
        app.buttons["Conversation settings"].tap()
        XCTAssertTrue(translation.waitForExistence(timeout: 5))
        XCTAssertEqual(translation.value as? String, "1")
        app.buttons["Cancel"].tap()

        // Open the normal chat from the same feed and check the shared setting.
        group.tap()
        let header = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Studio team")).firstMatch
        XCTAssertTrue(header.waitForExistence(timeout: 5))
        header.tap()
        XCTAssertTrue(translation.waitForExistence(timeout: 5))
        XCTAssertEqual(translation.value as? String, "1")
        translation.switches.firstMatch.tap()
        XCTAssertEqual(translation.value as? String, "0")
        app.buttons["Save"].tap()
        app.buttons["Messages"].tap()

        let person = app.buttons["Open chat: Jordan"].firstMatch
        XCTAssertTrue(person.waitForExistence(timeout: 5))
        person.press(forDuration: 1)
        app.buttons["Conversation settings"].tap()
        XCTAssertTrue(translation.waitForExistence(timeout: 5))
        XCTAssertEqual(translation.value as? String, "0")
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Person translation remains off after changing a group"
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }
}
