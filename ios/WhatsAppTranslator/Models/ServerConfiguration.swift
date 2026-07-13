import Foundation

struct ServerConfiguration: Codable, Equatable, Sendable {
    let baseURL: URL
    let password: String

    static func make(address: String, password: String) throws -> ServerConfiguration {
        var value = address.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { throw ConfigurationError.missingAddress }

        if !value.contains("://") {
            value = "https://\(value)"
        }

        guard var components = URLComponents(string: value),
              let scheme = components.scheme?.lowercased(),
              ["http", "https"].contains(scheme),
              components.host != nil else {
            throw ConfigurationError.invalidAddress
        }

        components.path = components.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        components.query = nil
        components.fragment = nil
        guard let url = components.url else { throw ConfigurationError.invalidAddress }
        return ServerConfiguration(baseURL: url, password: password)
    }

    enum ConfigurationError: LocalizedError {
        case missingAddress
        case invalidAddress

        var errorDescription: String? {
            switch self {
            case .missingAddress: "Enter the web address of your translator."
            case .invalidAddress: "Enter a valid HTTP or HTTPS web address."
            }
        }
    }
}
