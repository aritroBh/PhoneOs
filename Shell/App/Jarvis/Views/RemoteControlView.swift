import SwiftUI
import JarvisKit

/// Remote control (PLAN §2.7): start, watch and stop tasks from another of your devices.
/// The pairing and the hub are not in this build's bindings yet; the screen says so.
struct RemoteControlView: View {
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                ScreenTitle(title: "Remote control", subtitle: "Run Jarvis from your other devices.",
                            identifier: "remote.title")
                ComingCard(
                    icon: .remote,
                    title: "No devices paired",
                    message: "Pairing arrives with the next core update. Until then, Jarvis takes tasks on this phone only.",
                    points: [
                        "Same network only; Jarvis must be open on the phone.",
                        "Approvals stay on the phone unless you allow them per device.",
                        "Payments, purchases, passwords, account changes and deleting are always approved here.",
                    ],
                    identifier: "remote.empty"
                )
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 24)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
    }
}
