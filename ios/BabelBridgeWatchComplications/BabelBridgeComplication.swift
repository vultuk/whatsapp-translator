import SwiftUI
import WidgetKit

private struct LauncherEntry: TimelineEntry {
    let date: Date
}

private struct LauncherProvider: TimelineProvider {
    func placeholder(in context: Context) -> LauncherEntry { LauncherEntry(date: .now) }

    func getSnapshot(in context: Context, completion: @escaping (LauncherEntry) -> Void) {
        completion(LauncherEntry(date: .now))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<LauncherEntry>) -> Void) {
        completion(Timeline(entries: [LauncherEntry(date: .now)], policy: .never))
    }
}

private struct LauncherView: View {
    @Environment(\.widgetFamily) private var family

    var body: some View {
        Group {
            switch family {
            case .accessoryInline:
                Label("Babel Bridge", systemImage: "text.bubble.fill")
            case .accessoryRectangular:
                HStack(spacing: 8) {
                    Image(systemName: "text.bubble.fill")
                        .font(.title2).widgetAccentable()
                    VStack(alignment: .leading) {
                        Text("Babel Bridge").font(.headline)
                        Text("Messages").font(.caption)
                    }
                }
            case .accessoryCorner:
                Image(systemName: "text.bubble.fill")
                    .font(.title2).widgetAccentable()
                    .widgetLabel { Text("Babel Bridge") }
            default:
                ZStack {
                    AccessoryWidgetBackground()
                    Image(systemName: "text.bubble.fill")
                        .font(.title2).widgetAccentable()
                }
            }
        }
        .accessibilityLabel("Open Babel Bridge Messages")
        .containerBackground(for: .widget) { Color.clear }
        // A launcher complication opens its containing Watch app on tap.
    }
}

@main
struct BabelBridgeComplication: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "BabelBridgeMessages", provider: LauncherProvider()) { _ in
            LauncherView()
        }
        .configurationDisplayName("Babel Bridge")
        .description("Open Messages and reply from your Watch.")
        .supportedFamilies([.accessoryCircular, .accessoryCorner, .accessoryRectangular, .accessoryInline])
    }
}

#Preview("Circular", as: .accessoryCircular) {
    BabelBridgeComplication()
} timeline: {
    LauncherEntry(date: .now)
}

#Preview("Corner", as: .accessoryCorner) {
    BabelBridgeComplication()
} timeline: {
    LauncherEntry(date: .now)
}

#Preview("Rectangular", as: .accessoryRectangular) {
    BabelBridgeComplication()
} timeline: {
    LauncherEntry(date: .now)
}

#Preview("Inline", as: .accessoryInline) {
    BabelBridgeComplication()
} timeline: {
    LauncherEntry(date: .now)
}
