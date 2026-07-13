import SwiftUI

struct TimezonePickerView: View {
    @Environment(\.dismiss) private var dismiss
    @Binding var selection: String?
    @State private var searchText = ""

    private var identifiers: [String] {
        let values = TimeZone.knownTimeZoneIdentifiers
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return values }
        return values.filter {
            $0.localizedCaseInsensitiveContains(query)
                || Self.friendlyName($0).localizedCaseInsensitiveContains(query)
        }
    }

    var body: some View {
        NavigationStack {
            List {
                Button {
                    selection = nil
                    dismiss()
                } label: {
                    HStack {
                        Label("Not set", systemImage: "iphone")
                        Spacer()
                        if selection == nil { Image(systemName: "checkmark").foregroundStyle(.tint) }
                    }
                }
                .foregroundStyle(.primary)

                ForEach(identifiers, id: \.self) { identifier in
                    Button {
                        selection = identifier
                        dismiss()
                    } label: {
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(Self.friendlyName(identifier))
                                    .foregroundStyle(.primary)
                                Text(Self.currentTime(in: identifier))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            if selection == identifier { Image(systemName: "checkmark").foregroundStyle(.tint) }
                        }
                    }
                }
            }
            .navigationTitle("Contact timezone")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(text: $searchText, prompt: "Search city or timezone")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
    }

    static func friendlyName(_ identifier: String) -> String {
        identifier
            .split(separator: "/")
            .map { $0.replacingOccurrences(of: "_", with: " ") }
            .joined(separator: " · ")
    }

    static func currentTime(in identifier: String) -> String {
        guard let timeZone = TimeZone(identifier: identifier) else { return identifier }
        var style = Date.FormatStyle(date: .omitted, time: .shortened)
        style.timeZone = timeZone
        return Date().formatted(style)
    }
}
