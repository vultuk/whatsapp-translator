import Foundation

actor APIClient {
    private let session: URLSession
    private var configuration: ServerConfiguration?
    private var token: String?
    private var webSocket: URLSessionWebSocketTask?
    private var receiveTask: Task<Void, Never>?

    init(session: URLSession = .shared) {
        self.session = session
    }

    func configure(_ configuration: ServerConfiguration) {
        self.configuration = configuration
        token = nil
    }

    func authenticate() async throws -> BackendStatus {
        guard let configuration else { throw APIError.notConfigured }
        let health: HealthResponse = try await request("/api/health", authenticated: false)
        guard health.ok else { throw APIError.invalidServer }

        let check: AuthCheckResponse = try await request("/api/auth/check", authenticated: false)
        if check.required {
            let body = try JSONEncoder.backend.encode(["password": configuration.password])
            let response: AuthResponse = try await request(
                "/api/auth",
                method: "POST",
                body: body,
                authenticated: false
            )
            guard response.success else { throw APIError.server(response.error ?? "Incorrect password") }
            token = response.token
        }
        return try await status()
    }

    func status() async throws -> BackendStatus {
        try await authorizedRequest("/api/status")
    }

    func contacts() async throws -> [Contact] {
        try await authorizedRequest("/api/contacts")
    }

    func avatar(contactID: String) async throws -> URL? {
        let response: AvatarResponse = try await authorizedRequest("/api/avatar/\(contactID.urlPathEncoded)")
        return response.url
    }

    func messages(contactID: String, before: Int64? = nil, beforeID: String? = nil) async throws -> MessagesResponse {
        var components = URLComponents()
        components.queryItems = [URLQueryItem(name: "limit", value: "50")]
        if let before { components.queryItems?.append(URLQueryItem(name: "before", value: String(before))) }
        if let beforeID { components.queryItems?.append(URLQueryItem(name: "before_id", value: beforeID)) }
        let query = components.percentEncodedQuery.map { "?\($0)" } ?? ""
        return try await authorizedRequest("/api/messages/\(contactID.urlPathEncoded)\(query)")
    }

    func send(contactID: String, text: String) async throws -> SendMessageResponse {
        let payload = SendMessageRequest(
            contactId: contactID,
            text: text,
            replyTo: nil,
            replyToSender: nil,
            replyToText: nil,
            replyToSenderName: nil
        )
        return try await authorizedRequest(
            "/api/send",
            method: "POST",
            body: JSONEncoder.backend.encode(payload)
        )
    }

    func markRead(contactID: String) async throws {
        let body = try JSONEncoder.backend.encode(EmptyMarkReadRequest())
        let _: SuccessResponse = try await authorizedRequest(
            "/api/contacts/\(contactID.urlPathEncoded)/read",
            method: "POST",
            body: body
        )
    }

    func conversationSettings(contactID: String) async throws -> ConversationSettings {
        try await authorizedRequest("/api/contacts/\(contactID.urlPathEncoded)/settings")
    }

    func updateConversationSettings(contactID: String, settings: ConversationSettings) async throws {
        let _: SuccessResponse = try await authorizedRequest(
            "/api/contacts/\(contactID.urlPathEncoded)/settings",
            method: "PUT",
            body: JSONEncoder.backend.encode(settings)
        )
    }

    func openAISettings() async throws -> OpenAISettings {
        try await authorizedRequest("/api/settings/openai")
    }

    func updateOpenAISettings(_ settings: OpenAISettings) async throws {
        let _: SuccessResponse = try await authorizedRequest(
            "/api/settings/openai",
            method: "PUT",
            body: JSONEncoder.backend.encode(settings)
        )
    }

    func registerPushDevice(_ registration: PushDeviceRegistration) async throws {
        let _: SuccessResponse = try await authorizedRequest(
            "/api/push/devices",
            method: "PUT",
            body: JSONEncoder.backend.encode(registration)
        )
    }

    func connectLiveEvents(handler: @escaping @MainActor @Sendable (LiveEvent) -> Void) throws {
        guard let configuration else { throw APIError.notConfigured }
        disconnectLiveEvents()
        var components = URLComponents(url: configuration.baseURL.appending(path: "ws"), resolvingAgainstBaseURL: false)
        components?.scheme = configuration.baseURL.scheme == "https" ? "wss" : "ws"
        if let token { components?.queryItems = [URLQueryItem(name: "token", value: token)] }
        guard let url = components?.url else { throw APIError.invalidServer }

        let socket = session.webSocketTask(with: url)
        webSocket = socket
        socket.resume()
        receiveTask = Task { [weak self] in
            guard let self else { return }
            await self.receiveMessages(socket: socket, handler: handler)
        }
    }

    func disconnectLiveEvents() {
        receiveTask?.cancel()
        receiveTask = nil
        webSocket?.cancel(with: .goingAway, reason: nil)
        webSocket = nil
    }

    private func receiveMessages(
        socket: URLSessionWebSocketTask,
        handler: @escaping @MainActor @Sendable (LiveEvent) -> Void
    ) async {
        while !Task.isCancelled {
            do {
                let message = try await socket.receive()
                let data: Data
                switch message {
                case .string(let value): data = Data(value.utf8)
                case .data(let value): data = value
                @unknown default: continue
                }
                if let event = try? JSONDecoder.backend.decode(LiveEvent.self, from: data) {
                    await handler(event)
                }
            } catch {
                return
            }
        }
    }

    private func authorizedRequest<T: Decodable & Sendable>(
        _ path: String,
        method: String = "GET",
        body: Data? = nil
    ) async throws -> T {
        do {
            return try await request(path, method: method, body: body, authenticated: true)
        } catch APIError.unauthorized {
            _ = try await authenticate()
            return try await request(path, method: method, body: body, authenticated: true)
        }
    }

    private func request<T: Decodable & Sendable>(
        _ path: String,
        method: String = "GET",
        body: Data? = nil,
        authenticated: Bool
    ) async throws -> T {
        guard let configuration else { throw APIError.notConfigured }
        guard let url = URL(string: path, relativeTo: configuration.baseURL)?.absoluteURL else {
            throw APIError.invalidServer
        }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.httpBody = body
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if authenticated, let token {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }

        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw APIError.invalidServer }
        if http.statusCode == 401 { throw APIError.unauthorized }
        guard (200..<300).contains(http.statusCode) else {
            let message = (try? JSONDecoder.backend.decode(ErrorResponse.self, from: data).error)
                ?? String(data: data, encoding: .utf8)
                ?? "Request failed"
            throw APIError.server(message)
        }
        do {
            return try JSONDecoder.backend.decode(T.self, from: data)
        } catch {
            throw APIError.decoding(error.localizedDescription)
        }
    }
}

enum APIError: LocalizedError, Equatable {
    case notConfigured
    case invalidServer
    case unauthorized
    case server(String)
    case decoding(String)

    var errorDescription: String? {
        switch self {
        case .notConfigured: "Set up your translator server first."
        case .invalidServer: "That address is not a WhatsApp Translator server."
        case .unauthorized: "The password is no longer valid."
        case .server(let message), .decoding(let message): message
        }
    }
}

private struct HealthResponse: Decodable, Sendable { let ok: Bool }
private struct SuccessResponse: Decodable, Sendable { let success: Bool }
private struct ErrorResponse: Decodable, Sendable { let error: String }
private struct EmptyMarkReadRequest: Encodable, Sendable {}

extension JSONDecoder {
    static var backend: JSONDecoder { JSONDecoder() }
}

extension JSONEncoder {
    static var backend: JSONEncoder { JSONEncoder() }
}

private extension String {
    var urlPathEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .urlPathAllowed.subtracting(CharacterSet(charactersIn: "/"))) ?? self
    }
}
