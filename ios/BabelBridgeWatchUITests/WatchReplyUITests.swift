import XCTest

final class WatchReplyUITests: XCTestCase {
    @MainActor
    func testUnifiedFeedLatestAndSpecificMessageReplies() {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        let latest = app.buttons["watch-reply-latest"]
        XCTAssertTrue(latest.waitForExistence(timeout: 15))
        let feed = XCTAttachment(screenshot: app.screenshot())
        feed.name = "Watch unified Messages"
        feed.lifetime = .keepAlways
        add(feed)
        latest.tap()
        XCTAssertTrue(app.staticTexts["Reply to Studio team"].waitForExistence(timeout: 5))
        app.buttons["Cancel"].tap()
        let older = app.buttons["watch-message-one"]
        for _ in 0..<12 {
            if older.exists, older.frame.minY >= 58, older.frame.maxY <= 220 { break }
            let down = older.exists && older.frame.minY < 58
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: down ? 0.4 : 0.8))
                .press(forDuration: 0.1, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: down ? 0.7 : 0.45)))
        }
        XCTAssertTrue(older.exists)
        older.coordinate(withNormalizedOffset: CGVector(dx: 0.8, dy: 0.5))
            .press(forDuration: 0.1, thenDragTo: older.coordinate(withNormalizedOffset: CGVector(dx: 0.25, dy: 0.5)))
        let reply = app.buttons["Reply"]
        XCTAssertTrue(reply.waitForExistence(timeout: 5))
        reply.tap()
        XCTAssertTrue(app.staticTexts["Reply to Jordan"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Shall we meet by the café at six?"].exists)
        XCTAssertTrue(app.textFields["watch-reply-text"].exists)
        let selected = XCTAttachment(screenshot: app.screenshot())
        selected.name = "Watch selected message reply"
        selected.lifetime = .keepAlways
        add(selected)
        app.textFields["watch-reply-text"].tap()
        app.typeText("Thanks")
        if app.buttons["Done"].exists { app.buttons["Done"].tap() }
        let send = app.buttons["watch-send"]
        XCTAssertTrue(send.waitForExistence(timeout: 5))
        send.tap()
        XCTAssertTrue(app.staticTexts["Message sent"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["OK"].exists)
        let confirmation = XCTAttachment(screenshot: app.screenshot())
        confirmation.name = "Watch reply sent"
        confirmation.lifetime = .keepAlways
        add(confirmation)
        app.buttons["OK"].tap()
        XCTAssertFalse(app.textFields["watch-reply-text"].exists)
    }
}
