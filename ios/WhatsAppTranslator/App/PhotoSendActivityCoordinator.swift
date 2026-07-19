#if os(iOS)
@preconcurrency import ActivityKit
import Foundation
import UIKit

@MainActor
final class PhotoSendBackgroundLease {
    private var identifier: UIBackgroundTaskIdentifier = .invalid

    init(name: String) {
        identifier = UIApplication.shared.beginBackgroundTask(withName: name) { [weak self] in
            Task { @MainActor in self?.end() }
        }
    }

    func end() {
        guard identifier != .invalid else { return }
        UIApplication.shared.endBackgroundTask(identifier)
        identifier = .invalid
    }

}

@MainActor
final class PhotoSendActivityCoordinator {
    static let shared = PhotoSendActivityCoordinator()
    private var activities: [String: Activity<PhotoSendActivityAttributes>] = [:]

    func start(_ progress: PhotoSendProgress) {
        guard ActivityAuthorizationInfo().areActivitiesEnabled, activities[progress.id] == nil else { return }
        let attributes = PhotoSendActivityAttributes(jobID: progress.id, total: progress.total)
        let content = ActivityContent<PhotoSendActivityAttributes.ContentState>(
            state: PhotoSendActivityAttributes.ContentState(stage: progress.stage.rawValue, completed: progress.completed),
            staleDate: Date().addingTimeInterval(30 * 60)
        )
        activities[progress.id] = try? Activity.request(attributes: attributes, content: content)
    }

    func update(_ progress: PhotoSendProgress) {
        guard let activity = activities[progress.id] else { return }
        Task { @MainActor in
            await activity.update(ActivityContent<PhotoSendActivityAttributes.ContentState>(
                state: PhotoSendActivityAttributes.ContentState(stage: progress.stage.rawValue, completed: progress.completed),
                staleDate: Date().addingTimeInterval(30 * 60)
            ))
        }
    }

    func end(_ progress: PhotoSendProgress) {
        guard let activity = activities.removeValue(forKey: progress.id) else { return }
        Task { @MainActor in
            await activity.end(
                ActivityContent<PhotoSendActivityAttributes.ContentState>(
                    state: PhotoSendActivityAttributes.ContentState(stage: progress.stage.rawValue, completed: progress.completed),
                    staleDate: nil
                ),
                dismissalPolicy: .after(Date().addingTimeInterval(15))
            )
        }
    }
}
#endif
