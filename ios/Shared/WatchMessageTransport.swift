import Foundation

enum WatchMessageTransport {
    /// Watch Connectivity calls either completion on its background delegate queue.
    /// Sendable callbacks must not inherit the caller's MainActor isolation.
    @MainActor
    static func response(
        for data: Data,
        send: (Data, @escaping @Sendable (Data) -> Void, @escaping @Sendable (any Error) -> Void) -> Void
    ) async throws -> Data {
        try await withCheckedThrowingContinuation { continuation in
            send(data, { @Sendable response in
                continuation.resume(returning: response)
            }, { @Sendable error in
                continuation.resume(throwing: error)
            })
        }
    }
}
