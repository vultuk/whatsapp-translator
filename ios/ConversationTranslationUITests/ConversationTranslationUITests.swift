import AVKit
import UIKit
import XCTest

@MainActor
final class ConversationTranslationUITests: XCTestCase {
    func testLostWhatsAppSessionOpensLinkingAndReturnsToInbox() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoRelinking"]
        app.launch()
        XCTAssertTrue(app.staticTexts["Link WhatsApp"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.images["whatsapp-link-qr"].waitForExistence(timeout: 8))
        let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        screenshot.name = "Fresh WhatsApp linking flow after session removal"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        XCTAssertTrue(app.staticTexts["Link WhatsApp"].waitForNonExistence(timeout: 15))
        XCTAssertFalse(app.buttons["Connect translator"].exists)
    }
    func testVideoOpensFullScreenAndReturnsToTheSameMessage() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoVideo"]
        app.launch()
        let video = app.otherElements["Video"].firstMatch
        XCTAssertTrue(video.waitForExistence(timeout: 10))
        video.tap()
        let fullScreen = app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'full screen'")).firstMatch
        XCTAssertTrue(fullScreen.waitForExistence(timeout: 5), app.debugDescription)
        fullScreen.tap()
        XCTAssertGreaterThan(video.frame.width, app.frame.width * 0.95, app.debugDescription)
        defer { XCUIDevice.shared.orientation = .portrait }
        if UIDevice.current.userInterfaceIdiom == .phone {
            XCUIDevice.shared.orientation = .landscapeLeft
            XCTAssertGreaterThan(app.frame.width, app.frame.height)
        }
        // Reveal controls if their normal auto-hide has already elapsed.
        video.tap()
        let done = app.buttons.matching(NSPredicate(format: "label IN %@", ["Done", "Close", "Exit Full Screen"])).firstMatch
        XCTAssertTrue(done.waitForExistence(timeout: 5), app.debugDescription)
        let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        screenshot.name = "Video full screen"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        done.tap()
        XCTAssertTrue(app.staticTexts["A sample video to try full screen and picture in picture."].firstMatch.waitForExistence(timeout: 5))
    }

    func testPlayingVideoStartsPictureInPictureWhenLeavingApp() throws {
        try checkAutomaticPictureInPicture(fullScreen: false)
    }

    func testFullScreenVideoStartsPictureInPictureWhenLeavingApp() throws {
        try checkAutomaticPictureInPicture(fullScreen: true)
    }

    private func checkAutomaticPictureInPicture(fullScreen: Bool) throws {
        try XCTSkipUnless(AVPictureInPictureController.isPictureInPictureSupported(),
                          "AVKit reports that picture in picture is unavailable on this simulator/device")
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoVideo"]
        app.launch()
        let video = app.otherElements["Video"].firstMatch
        XCTAssertTrue(video.waitForExistence(timeout: 10))
        video.tap()
        XCTAssertTrue(app.buttons["Pause"].firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        if fullScreen {
            app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'full screen'")).firstMatch.tap()
        }
        let appWidth = app.frame.width
        XCUIDevice.shared.press(.home)
        XCUIApplication(bundleIdentifier: "com.apple.Preferences").activate()
        // AVKit exposes the floating window in the originating application's tree.
        let pip = app.otherElements["PIPUIView"]
        XCTAssertTrue(pip.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertLessThan(pip.frame.width, appWidth * 0.75)
        let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        screenshot.name = "Video continues in picture in picture outside Babel Bridge"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        app.activate()
    }

    func testOutgoingMessageShowsSavedTopicInItsMenu() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoTopics", "-demoOutgoingTopic"]
        app.launch()
        let target = app.staticTexts["The replay is brilliant too."].firstMatch
        XCTAssertTrue(target.waitForExistence(timeout: 10))
        target.press(forDuration: 1)
        let label = app.buttons["Football"].firstMatch
        XCTAssertTrue(label.waitForExistence(timeout: 5))
        XCTAssertFalse(label.isEnabled)
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Outgoing message with its saved Football category"
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }

    func testLongPressShowsSavedTopicInCombinedMessagesAndChat() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoTopics"]
        app.launch()
        let target = app.staticTexts["The replay is brilliant too."].firstMatch
        XCTAssertTrue(target.waitForExistence(timeout: 10))
        target.press(forDuration: 1)
        let label = app.buttons["Football"].firstMatch
        XCTAssertTrue(label.waitForExistence(timeout: 5))
        XCTAssertFalse(label.isEnabled, "Saved topic is information, not a message action")
        let combined = XCTAttachment(screenshot: app.screenshot())
        combined.name = "Saved topic in combined message long press menu"; combined.lifetime = .keepAlways; add(combined)
        app.buttons["React with ❤️"].firstMatch.tap()
        app.buttons["Open chat: Family"].firstMatch.tap()
        XCTAssertTrue(target.waitForExistence(timeout: 5))
        target.press(forDuration: 1)
        XCTAssertTrue(label.waitForExistence(timeout: 5))
        let chat = XCTAttachment(screenshot: app.screenshot())
        chat.name = "Saved topic in conversation long press menu"; chat.lifetime = .keepAlways; add(chat)
    }

    func testChatListKeepsCompactRowsAndVisibleFiltersWhenScrolling() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoChatList"]
        app.launch()
        app.buttons["Chats"].firstMatch.tap()
        let groups = app.buttons["Groups"].firstMatch
        XCTAssertTrue(groups.waitForExistence(timeout: 5))
        let first = app.staticTexts["Community group 1"].firstMatch
        let second = app.staticTexts["Community group 2"].firstMatch
        XCTAssertTrue(first.waitForExistence(timeout: 5))
        let initial = XCTAttachment(screenshot: app.screenshot())
        initial.name = "Chat list initial spacing"
        initial.lifetime = .keepAlways
        add(initial)
        XCTAssertTrue(groups.isHittable, "Chat filters must remain visible below the navigation bar")
        XCTAssertGreaterThanOrEqual(groups.frame.minY, app.navigationBars.firstMatch.frame.maxY - 1)
        XCTAssertLessThan(first.frame.minY - groups.frame.maxY, 60, "The first chat must sit directly below the filters")
        XCTAssertLessThan(second.frame.minY - first.frame.minY, 100, "Standard chat rows must not stack list padding on top of row padding")
        groups.tap()
        let filterY = groups.frame.minY
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.4)).press(forDuration: 0.1, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.8)))
        XCTAssertEqual(groups.frame.minY, filterY, accuracy: 2, "Pulling the list must not expand a blank navigation title")
        app.swipeUp()
        let filtersSettled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == true"), object: groups)
        XCTAssertEqual(XCTWaiter.wait(for: [filtersSettled], timeout: 5), .completed)
        XCTAssertTrue(groups.isHittable)
        XCTAssertEqual(groups.frame.minY, filterY, accuracy: 2)
        let scrolled = XCTAttachment(screenshot: app.screenshot())
        scrolled.name = "Chat list after scrolling"
        scrolled.lifetime = .keepAlways
        add(scrolled)
    }

    func testTopicFilterStaysFixedWhenPullingShortTimeline() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoTopics"]
        app.launch()
        let filter = app.buttons["topic-filter"].firstMatch
        XCTAssertTrue(filter.waitForExistence(timeout: 10))
        filter.tap()
        let combined = app.buttons["Football"].firstMatch
        (combined.exists ? combined : app.buttons["Football · Family"].firstMatch).tap()
        XCTAssertTrue(app.staticTexts["The replay is brilliant too."].firstMatch.waitForExistence(timeout: 5))
        app.swipeUp()
        let initialY = filter.frame.minY
        let before = XCTAttachment(screenshot: app.screenshot()); before.name = "Topic header before pulling timeline"; before.lifetime = .keepAlways; add(before)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.45)).press(forDuration: 0.1, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.8)))
        let after = XCTAttachment(screenshot: app.screenshot()); after.name = "Topic header after pulling timeline"; after.lifetime = .keepAlways; add(after)
        XCTAssertEqual(filter.frame.minY, initialY, accuracy: 2, "Pulling messages must not expand the header or move the topic filter")
    }

    func testTopicsCombineChatsAndShowQuotesOutsideLoadedHistory() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demoTopics"]
        app.launch()
        let filter = app.buttons["topic-filter"].firstMatch
        XCTAssertTrue(filter.waitForExistence(timeout: 10))
        let before = XCTAttachment(screenshot: app.screenshot()); before.name = "All group discussions in the unified feed"; before.lifetime = .keepAlways; add(before)
        filter.tap()
        let familyTopic = app.buttons["Weekend plans"].firstMatch
        XCTAssertTrue(familyTopic.waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons.matching(identifier: "Weekend plans").count, 1)
        familyTopic.tap()
        XCTAssertTrue(app.staticTexts["I’ll bring the picnic blanket and sandwiches."].firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["What a goal in last night’s match!"].exists)
        XCTAssertTrue(app.staticTexts["Sunday brunch at eleven?"].exists)
        XCTAssertTrue(app.staticTexts["Could someone bring lunch?"].exists)
        let filtered = XCTAttachment(screenshot: app.screenshot()); filtered.name = "Weekend plans combines Family and Friends and preserves the original quote"; filtered.lifetime = .keepAlways; add(filtered)
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
