#if canImport(ActivityKit)
import ActivityKit

struct PhotoSendActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        let stage: String
        let completed: Int
    }

    let jobID: String
    let total: Int
}
#endif
