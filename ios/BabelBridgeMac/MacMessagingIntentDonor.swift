enum MessagingIntentDonor {
    static func donateOutgoing(
        originalText: String,
        response: SendMessageResponse,
        contact: Contact,
        displayName: String
    ) {
        // Native macOS messaging remains available through the app and notification replies.
        // Siri/CarPlay intent donation belongs to the iOS target and its Intents extension.
    }
}
