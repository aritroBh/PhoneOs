import Foundation
import JarvisCore

/// Two cross-platform tools that prove the tool path end to end. Real native tools
/// (EventKit, Contacts, …) live in the app target.
public enum SampleTools {
    /// `clock_now`: the current date and time as ISO 8601 with the device's time zone.
    public static func registerClockNow(
        in registry: ToolRegistry,
        now: @escaping @Sendable () -> Date = Date.init,
        timeZone: TimeZone = .current
    ) {
        let spec = ToolSpecs.spec(
            name: "clock_now",
            description: "Current local date and time (ISO 8601) and the time zone.",
            selfContained: true
        )
        registry.register(spec) { _ in
            let formatter = ISO8601DateFormatter()
            formatter.timeZone = timeZone
            formatter.formatOptions = [.withInternetDateTime]
            return "\(formatter.string(from: now())) (\(timeZone.identifier))"
        }
    }

    /// `echo`: returns its `text` argument unchanged.
    public static func registerEcho(in registry: ToolRegistry) {
        let spec = ToolSpecs.spec(
            name: "echo",
            description: "Returns the given text unchanged.",
            params: [ToolSpecs.param("text", .string, "Text to return.")],
            selfContained: true
        )
        registry.register(spec) { arguments in
            guard let text = arguments.string("text") else {
                throw ToolError.invalidArguments("text must be a string")
            }
            return text
        }
    }
}
