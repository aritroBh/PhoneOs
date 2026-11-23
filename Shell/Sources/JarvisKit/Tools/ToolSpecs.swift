import Foundation
import JarvisCore

/// Builders for the generated `ToolSpec` / `ToolParam` records. The only place JarvisKit
/// uses their memberwise initialisers (the core has no defaults for them), so a field
/// added in the core is one edit here.
public enum ToolSpecs {
    /// A parameter.
    public static func param(_ name: String, _ kind: ParamKind, required: Bool = true, _ description: String) -> ToolParam {
        ToolParam(name: name, description: description, kind: kind, required: required)
    }

    /// A tool spec. `tier` defaults to native; `risk` to `RiskCategory.none` (no
    /// confirmation); `app` is the bundle id the tool acts on, if any. `selfContained`
    /// (seventeenth review): the tool reaches no place beyond its own arguments and its
    /// results come from nowhere else (a clock, an echo); default `false` (fail closed: its
    /// earlier results are a source for its later calls).
    public static func spec(
        name: String,
        description: String,
        params: [ToolParam] = [],
        tier: Tier = .native,
        risk: RiskCategory = RiskCategory.none,
        app: String? = nil,
        selfContained: Bool = false
    ) -> ToolSpec {
        ToolSpec(name: name, description: description, params: params, tier: tier, risk: risk, app: app,
                 selfContained: selfContained)
    }
}
