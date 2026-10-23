import Foundation

/// How iPhone, iPad and Mac present the bridge connection.
///
/// REST `GET /api/status` and the WebSocket `status` event use `connection_state`
/// with these values: `connecting`, `connected`, `reconnecting`, `disconnected`,
/// `linking_required`. `connected` is a separate boolean. A `{ "type": "disconnected" }`
/// event has no state of its own; it follows a status event for both a retry and a stop.
struct WhatsAppConnectionBanner: Equatable, Sendable {
    enum Kind: Equatable, Sendable {
        case hidden
        case connecting
        case reconnecting
        case disconnected
        case linkingRequired
    }

    /// Shown on the link screen. Logout is the only cause the status payload names.
    static let relinkHint = "Your WhatsApp session needs linking. On your phone, open WhatsApp, then Settings, Linked devices, and link a device. Your chats and server settings are saved."

    let kind: Kind

    var showsBanner: Bool {
        switch kind {
        case .connecting, .reconnecting, .disconnected:
            return true
        case .hidden, .linkingRequired:
            return false
        }
    }

    var title: String {
        switch kind {
        case .hidden:
            return ""
        case .connecting:
            return "Connecting to WhatsApp…"
        case .reconnecting:
            return "Reconnecting to WhatsApp…"
        case .disconnected:
            return "Disconnected from WhatsApp"
        case .linkingRequired:
            return "Link WhatsApp"
        }
    }

    var hint: String? {
        kind == .linkingRequired ? Self.relinkHint : nil
    }

    /// Short value for the Settings connection row.
    var statusLabel: String {
        switch kind {
        case .hidden:
            return "Connected"
        case .connecting:
            return "Connecting"
        case .reconnecting:
            return "Reconnecting"
        case .disconnected:
            return "Disconnected"
        case .linkingRequired:
            return "Needs linking"
        }
    }

    var systemImage: String {
        switch kind {
        case .hidden:
            return "checkmark.circle"
        case .connecting:
            return "wifi"
        case .reconnecting:
            return "arrow.triangle.2.circlepath"
        case .disconnected:
            return "wifi.slash"
        case .linkingRequired:
            return "qrcode"
        }
    }

    /// Title plus hint, for the connection surface the user actually sees.
    var displayedCopy: String {
        [Optional(title), hint].compactMap { value -> String? in
            guard let value else { return nil }
            let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed.isEmpty ? nil : trimmed
        }.joined(separator: " ")
    }

    /// `connected: true` hides the banner, including when an older backend omits
    /// `connection_state`. Reconnecting is shown only for that exact state while
    /// `connected` is false. Anything missing or unrecognised is a stop.
    static func resolve(connected: Bool, connectionState: String?) -> WhatsAppConnectionBanner {
        if connected {
            return WhatsAppConnectionBanner(kind: .hidden)
        }
        switch normalized(connectionState) {
        case "reconnecting":
            return WhatsAppConnectionBanner(kind: .reconnecting)
        case "linking_required":
            return WhatsAppConnectionBanner(kind: .linkingRequired)
        case "connecting":
            return WhatsAppConnectionBanner(kind: .connecting)
        case "connected":
            // The boolean says the session is down. Do not invent a retry.
            return WhatsAppConnectionBanner(kind: .disconnected)
        case "disconnected", nil, "":
            return WhatsAppConnectionBanner(kind: .disconnected)
        default:
            return WhatsAppConnectionBanner(kind: .disconnected)
        }
    }

    /// Keep a retry or a link request already reported by the preceding status
    /// event. A bare disconnect with no such report is a stop.
    static func statePreservedByBareDisconnect(current: String?) -> String {
        switch normalized(current) {
        case "reconnecting":
            return "reconnecting"
        case "linking_required":
            return "linking_required"
        default:
            return "disconnected"
        }
    }

    private static func normalized(_ value: String?) -> String? {
        guard let value else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return trimmed.isEmpty ? "" : trimmed
    }
}
