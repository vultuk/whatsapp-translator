import SwiftUI
import UniformTypeIdentifiers
import XCTest
#if os(macOS)
@testable import BabelBridgeMac
#else
@testable import WhatsAppTranslator
#endif

@MainActor
final class ImagePasteTests: XCTestCase {
    private func imageData(_ width: CGFloat = 40) throws -> Data {
        try XCTUnwrap(DemoImageFactory.landscape(size: CGSize(width: width, height: 30)).platformJPEGData(compressionQuality: 0.9))
    }

    private func provider(_ data: Data, type: UTType = .jpeg) -> NSItemProvider {
        let provider = NSItemProvider()
        provider.registerDataRepresentation(forTypeIdentifier: type.identifier, visibility: .all) { completion in
            completion(data, nil)
            return nil
        }
        return provider
    }

    func testImageRepresentationsAreDeduplicatedAndClipboardOrderIsPreserved() async throws {
        let first = try imageData(), second = try imageData(60)
        let multipleFormats = provider(first)
        multipleFormats.registerDataRepresentation(forTypeIdentifier: UTType.png.identifier, visibility: .all) { completion in
            completion(Data(), nil)
            return nil
        }
        let photos = try await ClipboardImages.load([multipleFormats, NSItemProvider(object: "text" as NSString), provider(second)])
        XCTAssertEqual(photos.map(\.data), [first, second])
        XCTAssertEqual(photos.map(\.mimeType), ["image/jpeg", "image/jpeg"])
    }

    func testUnreadableImageRejectsWholeBatch() async throws {
        do {
            _ = try await ClipboardImages.load([provider(try imageData()), provider(Data("invalid image".utf8))])
            XCTFail("Invalid image must not silently drop part of the selection")
        } catch { XCTAssertTrue(error is ImagePasteError) }
    }

    func testPNGBytesRemainUnchangedForTheUploadPipeline() async throws {
        let image = DemoImageFactory.landscape(size: CGSize(width: 40, height: 30))
        #if os(macOS)
        let representation = try XCTUnwrap(NSBitmapImageRep(data: XCTUnwrap(image.tiffRepresentation)))
        let data = try XCTUnwrap(representation.representation(using: .png, properties: [:]))
        #else
        let data = try XCTUnwrap(image.pngData())
        #endif
        let photos = try await ClipboardImages.load([provider(data, type: .png)])
        XCTAssertEqual(photos.first?.mimeType, "image/png")
        XCTAssertEqual(photos.first?.data, data)
    }

    func testMoreThanThirtyImagesIsRejected() async throws {
        do {
            _ = try await ClipboardImages.load((0..<31).map { _ in provider(Data()) })
            XCTFail("The existing album limit must apply to paste")
        } catch ImagePasteError.tooManyImages { }
    }

    func testPasteCapturesReplyAndRequiresExplicitSend() async throws {
        let controller = ImagePasteController()
        let reply = MessageReplyTarget(messageID: "original", senderJID: nil, senderName: "Alex", text: "Hello")
        var sendCount = 0
        controller.begin([provider(try imageData())], reply: reply, destination: "Original chat") { images, caption in
            XCTAssertEqual(images.count, 1)
            XCTAssertEqual(caption, "My caption")
            sendCount += 1
            return true
        }
        for _ in 0..<100 where controller.isLoading { try await Task.sleep(for: .milliseconds(10)) }
        let selection = try XCTUnwrap(controller.selection)
        XCTAssertEqual(sendCount, 0)
        XCTAssertEqual(selection.reply, reply)
        XCTAssertEqual(selection.destination, "Original chat")
        XCTAssertTrue(selection.send(selection.photos.map { OutgoingImage(data: $0.data, mimeType: $0.mimeType) }, "My caption"))
        XCTAssertEqual(sendCount, 1)
    }

    func testCancelPreventsLateClipboardLoadFromOpeningReview() async throws {
        let data = try imageData()
        let delayed = NSItemProvider()
        delayed.registerDataRepresentation(forTypeIdentifier: UTType.jpeg.identifier, visibility: .all) { completion in
            DispatchQueue.global().asyncAfter(deadline: .now() + 0.1) { completion(data, nil) }
            return nil
        }
        let controller = ImagePasteController()
        controller.begin([delayed], reply: nil) { _, _ in XCTFail("Paste must never send automatically"); return false }
        try await Task.sleep(for: .milliseconds(20))
        controller.cancel()
        try await Task.sleep(for: .milliseconds(150))
        XCTAssertNil(controller.selection)
        XCTAssertNil(controller.error)
        XCTAssertFalse(controller.isLoading)
    }

    func testNativeImagePastePreservesTextAndSelection() throws {
        let data = try imageData()
        let view = ImagePasteTextView()
        #if os(macOS)
        let board = NSPasteboard.withUniqueName()
        defer { board.releaseGlobally() }
        board.setData(data, forType: NSPasteboard.PasteboardType(UTType.jpeg.identifier))
        view.string = "Keep my draft"
        view.setSelectedRange(NSRange(location: 5, length: 2))
        #else
        let board = UIPasteboard.withUniqueName()
        defer { UIPasteboard.remove(withName: board.name) }
        board.setData(data, forPasteboardType: UTType.jpeg.identifier)
        view.text = "Keep my draft"
        view.selectedRange = NSRange(location: 5, length: 2)
        #endif
        view.imagePasteboard = board
        #if os(macOS)
        XCTAssertTrue(view.validateUserInterfaceItem(NSMenuItem(title: "Paste", action: #selector(ImagePasteTextView.paste(_:)), keyEquivalent: "v")))
        #else
        XCTAssertTrue(view.canPerformAction(#selector(ImagePasteTextView.paste(_:)), withSender: nil))
        #endif
        var count = 0
        view.pasteImages = { count += $0.count }
        view.paste(nil)
        XCTAssertEqual(count, 1)
        #if os(macOS)
        XCTAssertEqual(view.string, "Keep my draft")
        XCTAssertEqual(view.selectedRange(), NSRange(location: 5, length: 2))
        #else
        XCTAssertEqual(view.text, "Keep my draft")
        XCTAssertEqual(view.selectedRange, NSRange(location: 5, length: 2))
        #endif
        view.allowsImagePaste = false
        view.paste(nil)
        XCTAssertEqual(count, 1)
    }

    #if os(macOS)
    func testNativeCopiedImageLoadsOnceAndCanBePreparedForSending() async throws {
        let image = DemoImageFactory.landscape(size: CGSize(width: 40, height: 30))
        let board = NSPasteboard.withUniqueName()
        defer { board.releaseGlobally() }
        XCTAssertTrue(board.writeObjects([image]))
        let photos = try await ClipboardImages.load(ClipboardImages.providers(from: board))
        XCTAssertEqual(photos.count, 1)
        let photo = try XCTUnwrap(photos.first)
        let upload = try XCTUnwrap(PhotoUploadPreparer.prepare(data: photo.data, mimeType: photo.mimeType, image: photo.image, maximumBytes: 1_000_000))
        XCTAssertNotNil(PlatformImage(data: upload.data))
    }

    func testCopiedLocalImageFileLoadsAsPhoto() async throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).appendingPathExtension("jpg")
        let data = try imageData()
        try data.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let board = NSPasteboard.withUniqueName()
        defer { board.releaseGlobally() }
        board.writeObjects([url as NSURL])
        let photos = try await ClipboardImages.load(ClipboardImages.providers(from: board))
        XCTAssertEqual(photos.map(\.data), [data])
    }
    #endif
}
