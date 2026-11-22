import Foundation
import JarvisCore

/// One finished task as the Activity tab shows it.
public struct TaskRecord: Codable, Sendable, Equatable, Identifiable {
    public enum Status: String, Codable, Sendable {
        case success, failed, cancelled, blocked
        /// The phone stayed locked past the limit; nothing is wrong with the task.
        case interrupted

        public init(_ status: OutcomeStatus) {
            switch status {
            case .success: self = .success
            case .failed: self = .failed
            case .cancelled: self = .cancelled
            case .blocked: self = .blocked
            case .interrupted: self = .interrupted
            }
        }

        /// A workflow's status in the same words: a `stopUnless` that did not hold reads
        /// "Stopped", like an owner's stop; finished with failures reads "Failed".
        public init(_ status: WorkflowStatus) {
            switch status {
            case .succeeded: self = .success
            case .completedWithFailures, .failed: self = .failed
            case .stopped, .cancelled: self = .cancelled
            case .blocked: self = .blocked
            case .interrupted: self = .interrupted
            }
        }
    }

    public var id: String
    public var task: String
    public var status: Status
    public var summary: String
    public var steps: Int
    /// Model calls (0 means the whole task replayed a learned flow).
    public var modelCalls: Int
    public var durationMs: Int
    /// Apps the steps ran in, in order of first use.
    public var apps: [String]
    public var finishedAt: Date
    /// The skill the task ran under (absent in records written before skills).
    public var skillId: String?
    public var skillName: String?
    /// `true` for the entry of a whole workflow run (its sessions are listed in
    /// `sessionIds`); absent for a task.
    public var isWorkflow: Bool?
    /// The workflow run this record belongs to: on a workflow entry its own id, on one of
    /// its sessions the run's id (Activity groups them by it).
    public var workflowRunId: String?
    public var workflowName: String?
    /// The workflow entry's sessions, in order.
    public var sessionIds: [String]?
    /// The paired device that asked for the task (remote control), by name.
    public var requestedBy: String?

    public init(id: String, task: String, status: Status, summary: String, steps: Int, modelCalls: Int,
                durationMs: Int, apps: [String] = [], finishedAt: Date, skillId: String? = nil, skillName: String? = nil,
                isWorkflow: Bool? = nil, workflowRunId: String? = nil, workflowName: String? = nil,
                sessionIds: [String]? = nil, requestedBy: String? = nil) {
        self.id = id
        self.task = task
        self.status = status
        self.summary = summary
        self.steps = steps
        self.modelCalls = modelCalls
        self.durationMs = durationMs
        self.apps = apps
        self.finishedAt = finishedAt
        self.skillId = skillId
        self.skillName = skillName
        self.isWorkflow = isWorkflow
        self.workflowRunId = workflowRunId
        self.workflowName = workflowName
        self.sessionIds = sessionIds
        self.requestedBy = requestedBy
    }

    /// A record of a driver run.
    public init(task: TaskSpec, result: RunResult, apps: [String] = [], skill: SkillContext? = nil,
                origin: TaskOrigin = .owner, finishedAt: Date = Date()) {
        self.init(
            id: task.id, task: task.text, status: Status(result.outcome.status), summary: result.outcome.summary,
            steps: Int(result.outcome.steps), modelCalls: result.modelCalls,
            durationMs: Int(clamping: result.outcome.durationMs), apps: apps, finishedAt: finishedAt,
            skillId: skill?.id, skillName: skill?.name,
            workflowRunId: origin.workflow?.runId, workflowName: origin.workflow?.name,
            requestedBy: origin.remoteDeviceName
        )
    }

    /// The entry of a finished workflow run, grouping `sessionIds`.
    public init(workflowRun runId: String, name: String, outcome: WorkflowOutcome, sessionIds: [String],
                durationMs: Int, requestedBy: String? = nil, finishedAt: Date = Date()) {
        self.init(
            id: runId, task: name, status: Status(outcome.status), summary: outcome.summary,
            steps: Int(outcome.stepsRun), modelCalls: 0, durationMs: durationMs, apps: [], finishedAt: finishedAt,
            isWorkflow: true, workflowRunId: runId, workflowName: name, sessionIds: sessionIds, requestedBy: requestedBy
        )
    }

    /// Whether this is one session of a workflow run (Activity shows it under the run).
    public var isWorkflowSession: Bool { isWorkflow != true && workflowRunId != nil }

    /// `m:ss`.
    public var durationText: String {
        let seconds = max(0, durationMs) / 1_000
        return "\(seconds / 60):" + String(format: "%02d", seconds % 60)
    }
}

/// The task records in one JSON file, newest first, bounded.
public struct TaskHistoryStore: Sendable {
    public let fileURL: URL
    public let capacity: Int

    public init(fileURL: URL, capacity: Int = 200) {
        self.fileURL = fileURL
        self.capacity = max(1, capacity)
    }

    /// The records, newest first (empty when none or unreadable).
    public func load() -> [TaskRecord] {
        guard let data = try? Data(contentsOf: fileURL) else { return [] }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return (try? decoder.decode([TaskRecord].self, from: data)) ?? []
    }

    /// Adds `record` in front and returns the new list.
    @discardableResult
    public func append(_ record: TaskRecord) throws -> [TaskRecord] {
        var records = load()
        records.removeAll { $0.id == record.id }
        records.insert(record, at: 0)
        if records.count > capacity { records.removeLast(records.count - capacity) }
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        try FileManager.default.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try encoder.encode(records).write(to: fileURL, options: [.atomic])
        return records
    }
}

/// What the Activity tab reads out of the core's memory JSON (`MemoryStore`): the task log,
/// the learned flows and the approved apps. Tolerant: unknown or missing fields are skipped.
public struct MemorySnapshot: Sendable, Equatable {
    public struct LogEntry: Sendable, Equatable {
        public var task: String
        public var outcome: String
        public var summary: String
        public var steps: Int
        public var apps: [String]
        public var durationMs: Int
    }

    public struct LearnedFlow: Sendable, Equatable, Identifiable {
        public var id: String
        public var intentKey: String
        public var app: String
        public var steps: Int
        public var successes: Int
        public var failures: Int
    }

    public var log: [LogEntry]
    public var flows: [LearnedFlow]
    public var approvedApps: [String]

    public static let empty = MemorySnapshot(log: [], flows: [], approvedApps: [])

    /// Parses memory JSON; empty or invalid input gives `.empty`.
    public init(json: String) {
        guard !json.isEmpty, let root = try? JSONValue(parsing: json) else {
            self = .empty
            return
        }
        let int = { (v: JSONValue?) -> Int in (v?.numberValue ?? 0).clampedInt }
        log = Array((root["log"]?["entries"]?.arrayValue ?? []).map { e in
            let steps = e["steps"]?.arrayValue ?? []
            var apps: [String] = []
            for step in steps {
                if let app = step["app"]?.stringValue, !app.isEmpty, !apps.contains(app) { apps.append(app) }
            }
            return LogEntry(
                task: e["task"]?.stringValue ?? "",
                outcome: e["outcome"]?.stringValue ?? "",
                summary: e["summary"]?.stringValue ?? "",
                steps: steps.count,
                apps: apps,
                durationMs: max(0, int(e["finished_ms"]).subtractingReportingOverflow(int(e["started_ms"])).partialValue)
            )
        }.reversed())
        flows = (root["flows"]?["flows"]?.arrayValue ?? []).map { f in
            LearnedFlow(
                id: f["id"]?.stringValue ?? "",
                intentKey: f["intent_key"]?.stringValue ?? "",
                app: f["app_bundle_id"]?.stringValue ?? "",
                steps: f["steps"]?.arrayValue?.count ?? 0,
                successes: int(f["successes"]),
                failures: int(f["failures"])
            )
        }
        approvedApps = (root["approved_apps"]?.arrayValue ?? []).compactMap(\.stringValue)
    }

    public init(log: [LogEntry], flows: [LearnedFlow], approvedApps: [String]) {
        self.log = log
        self.flows = flows
        self.approvedApps = approvedApps
    }
}
