import ActivityKit
import SwiftUI
import WidgetKit

@main
struct PhotoSendActivityWidgetBundle: WidgetBundle {
    var body: some Widget { PhotoSendActivityWidget() }
}

struct PhotoSendActivityWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: PhotoSendActivityAttributes.self) { context in
            HStack(spacing: 12) {
                Image(systemName: context.state.stage == "complete" ? "checkmark.circle.fill" : "photo.stack.fill")
                    .foregroundStyle(.mint)
                VStack(alignment: .leading, spacing: 5) {
                    Text(status(context)).font(.headline)
                    ProgressView(value: fraction(context)).tint(.mint)
                }
            }
            .padding()
            .activityBackgroundTint(.black.opacity(0.88))
            .activitySystemActionForegroundColor(.white)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) { Image(systemName: "photo.stack.fill").foregroundStyle(.mint) }
                DynamicIslandExpandedRegion(.trailing) { Text("\(context.state.completed)/\(context.attributes.total)").monospacedDigit() }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(spacing: 5) {
                        Text(status(context)).font(.caption)
                        ProgressView(value: fraction(context)).tint(.mint)
                    }
                }
            } compactLeading: {
                Image(systemName: "photo.stack.fill").foregroundStyle(.mint)
            } compactTrailing: {
                Text("\(context.state.completed)/\(context.attributes.total)").monospacedDigit()
            } minimal: {
                Image(systemName: "photo.stack.fill").foregroundStyle(.mint)
            }
        }
    }

    private func fraction(_ context: ActivityViewContext<PhotoSendActivityAttributes>) -> Double {
        context.state.stage == "complete" ? 1 : Double(context.state.completed) / Double(max(1, context.attributes.total))
    }

    private func status(_ context: ActivityViewContext<PhotoSendActivityAttributes>) -> String {
        let count = "\(context.state.completed) of \(context.attributes.total)"
        switch context.state.stage {
        case "preparing": return "Preparing \(count)"
        case "uploading": return "Uploading \(count)"
        case "sending": return "Sending \(count)"
        case "complete": return "Photos sent"
        case "failed": return "Photo send failed"
        default: return "Transferring photos"
        }
    }
}
