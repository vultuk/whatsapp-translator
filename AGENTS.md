# WhatsApp Translator Operating Rules

## TestFlight release completion gate

- After every completed request that changes this application, publish a new TestFlight build before treating the work as complete.
- Always increment the shared build number and update both native Apple targets in TestFlight: `WhatsAppTranslator` for iOS/iPadOS and `BabelBridgeMac` for macOS.
- Test, archive, and upload both targets. Do not assume an iOS upload also updates macOS, or that a backend/web deployment removes the need for the native TestFlight releases.
- Verify that App Store Connect accepted both uploads and that both builds have entered processing. Report the version and build number for each platform in the final handoff.
- If either upload cannot be completed, do not describe the request as fully released; report the exact blocker and which platform remains outstanding.
