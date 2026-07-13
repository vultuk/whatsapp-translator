import Intents

enum MessagingIntentDonor {
    static func donateOutgoing(
        originalText: String,
        response: SendMessageResponse,
        contact: Contact,
        displayName: String
    ) {
        let recipient = INPerson(
            personHandle: INPersonHandle(value: contact.id, type: .unknown),
            nameComponents: nil,
            displayName: displayName,
            image: nil,
            contactIdentifier: nil,
            customIdentifier: contact.id,
            isContactSuggestion: true,
            suggestionType: .instantMessageAddress
        )
        let intent = INSendMessageIntent(
            recipients: [recipient],
            outgoingMessageType: .outgoingMessageText,
            content: response.translatedText ?? originalText,
            speakableGroupName: contact.isGroup
                ? INSpeakableString(spokenPhrase: displayName)
                : nil,
            conversationIdentifier: contact.id,
            serviceName: "Babel Bridge",
            sender: nil,
            attachments: nil
        )
        let interaction = INInteraction(intent: intent, response: nil)
        interaction.direction = .outgoing
        interaction.donate { _ in }
    }
}
