import XCTest

@MainActor
final class ConversationTranslationUITests: XCTestCase {
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
