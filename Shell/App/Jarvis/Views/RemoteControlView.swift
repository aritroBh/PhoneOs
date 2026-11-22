import SwiftUI
import CoreImage
import CoreImage.CIFilterBuiltins
import JarvisCore
import JarvisKit

/// Remote control (PLAN §2.7): the master switch, paired devices with what each may do,
/// pairing (code and QR, expiring), the audit log. Pushed from Control.
struct RemoteControlView: View {
    @EnvironmentObject private var model: AppModel
    @State private var pairing = false
    @State private var error: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                ScreenTitle(title: "Remote control", subtitle: "Start and watch tasks from your other devices. This iPhone stays in charge.",
                            identifier: "remote.title")

                VStack(alignment: .leading, spacing: 0) {
                    Toggle(isOn: Binding(
                        get: { model.settings.remoteControlEnabled },
                        set: { model.setRemoteEnabled($0) }
                    )) {
                        Text("Remote control").font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                    }
                    .tint(Theme.connected)
                    .padding(.horizontal, Theme.gutter)
                    .frame(minHeight: 56)
                    .accessibilityIdentifier("remote.enabled")
                    Divider().overlay(Theme.bezel)
                    Text("Devices you pair here can start, watch and stop tasks over the same Wi-Fi while Jarvis is open on this phone. Every task still follows your rules, and approvals stay on this phone unless you allow them for a device.")
                        .font(Theme.meta)
                        .foregroundStyle(Theme.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(Theme.gutter)
                    if model.settings.remoteControlEnabled {
                        statusLine
                    }
                }
                .card()

                if let message = error ?? model.remoteError {
                    InlineError(text: message, identifier: "remote.error")
                }

                if model.settings.remoteControlEnabled && model.remoteStatus != .off {
                    pairSection
                    devicesSection
                }

                NavigationLink {
                    RemoteAuditView()
                } label: {
                    NavRow(icon: .log, title: "Audit log", detail: model.remoteAudit.isEmpty ? "Empty" : "\(model.remoteAudit.count) entries")
                        .card()
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("remote.audit")
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 24)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $pairing, onDismiss: { model.cancelPairing() }) {
            PairingSheet()
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
    }

    private var statusLine: some View {
        let (text, color): (String, Color) = switch model.remoteStatus {
        case .off: ("Off", Theme.textTertiary)
        case .starting: ("Starting the listener…", Theme.warning)
        case .listening(let detail): ("Listening" + (detail.map { " · \($0)" } ?? ""), Theme.connected)
        case .failed(let why): ("Not listening: \(why)", Theme.error)
        case .storeProblem(let why): (why, Theme.error)
        }
        return HStack(spacing: Theme.gapSmall) {
            StatusDot(color: color)
            Text(text).font(Theme.meta).foregroundStyle(Theme.textSoft).lineLimit(3)
        }
        .padding(.horizontal, Theme.gutter)
        .padding(.bottom, Theme.gap)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("remote.status")
    }

    @ViewBuilder private var pairSection: some View {
        let lockedFor = model.pairingLockedForSeconds
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            Button {
                error = model.openPairing()
                if error == nil { pairing = true }
            } label: {
                Label {
                    Text("Pair a device")
                } icon: {
                    Image.decorative(.qrCode, size: Theme.iconStandard)
                }
            }
            .buttonStyle(PrimaryButtonStyle())
            .disabled(lockedFor > 0)
            .accessibilityIdentifier("remote.pair")
            if lockedFor > 0 {
                Text("Pairing is paused for \(max(1, lockedFor / 60)) min after failed attempts.")
                    .font(Theme.meta)
                    .foregroundStyle(Theme.warning)
                    .accessibilityIdentifier("remote.lockout")
            }
        }
    }

    @ViewBuilder private var devicesSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionLabel(text: "Devices")
            if model.remoteDevices.isEmpty {
                Text("No devices paired yet.")
                    .font(Theme.body)
                    .foregroundStyle(Theme.textSecondary)
                    .padding(Theme.gutter)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .card()
                    .accessibilityIdentifier("remote.noDevices")
            }
            ForEach(Array(model.remoteDevices.enumerated()), id: \.element.id) { index, device in
                DeviceCard(device: device, index: index, error: $error)
            }
        }
    }
}

/// One paired device: name, connection, what it may do, rename, remove.
private struct DeviceCard: View {
    @EnvironmentObject private var model: AppModel
    var device: DeviceInfo
    var index: Int
    @Binding var error: String?
    @State private var renaming = false
    @State private var newName = ""
    @State private var confirmingRemove = false

    private var connected: Bool { model.remoteConnected.contains(device.id) }
    private var prefix: String { "remote.device.\(index)" }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: Theme.gap) {
                Image.decorative(.model, size: Theme.iconStandard)
                    .foregroundStyle(Theme.textSoft)
                    .frame(width: Theme.rowTile, height: Theme.rowTile)
                    .modifier(Bezeled(fill: Theme.raisedHigh, radius: Theme.radiusElement))
                VStack(alignment: .leading, spacing: 2) {
                    Text(device.name).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                        .accessibilityIdentifier("\(prefix).name")
                    HStack(spacing: 6) {
                        StatusDot(color: connected ? Theme.connected : Theme.textTertiary)
                        Text(connected ? "Connected now" : "Not connected").font(Theme.meta).foregroundStyle(Theme.textSecondary)
                    }
                }
                Spacer()
            }
            .padding(Theme.gutter)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier(prefix)
            Divider().overlay(Theme.bezel)
            permissionRow("Start and stop tasks", isOn: device.permissions.startTasks, id: "startTasks") { p, on in
                p.startTasks = on
                p.seeProgress = on
            }
            Divider().overlay(Theme.bezel)
            permissionRow("See what Jarvis sees", isOn: device.permissions.seeScreenText, id: "seeScreen") { p, on in
                p.seeScreenText = on
            }
            Divider().overlay(Theme.bezel)
            permissionRow("Approve for you", isOn: device.permissions.approve, id: "approve") { p, on in
                p.approve = on
            }
            NeverRemoteNote(identifier: "\(prefix).neverRemote")
                .padding(.horizontal, Theme.gutter)
                .padding(.bottom, Theme.gap)
            Divider().overlay(Theme.bezel)
            HStack(spacing: Theme.gap) {
                Button("Rename") {
                    newName = device.name
                    renaming = true
                }
                .buttonStyle(SecondaryButtonStyle())
                .accessibilityIdentifier("\(prefix).rename")
                Button("Remove") { confirmingRemove = true }
                    .buttonStyle(SecondaryButtonStyle(foreground: Theme.errorText))
                    .accessibilityIdentifier("\(prefix).remove")
            }
            .padding(Theme.gap)
        }
        .card()
        .alert("Rename device", isPresented: $renaming) {
            TextField("Name", text: $newName)
                .accessibilityIdentifier("\(prefix).newName")
            Button("Cancel", role: .cancel) {}
            Button("Save") { error = model.renameDevice(device.id, to: newName) }
        }
        .confirmationDialog("Remove \(device.name)?", isPresented: $confirmingRemove, titleVisibility: .visible) {
            Button("Remove", role: .destructive) { error = model.removeDevice(device.id) }
                .accessibilityIdentifier("\(prefix).confirmRemove")
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("It is disconnected at once, its key is forgotten and a task it started stops. Pair it again to use it.")
        }
    }

    private func permissionRow(_ title: String, isOn: Bool, id: String,
                               change: @escaping (inout RemotePermissions, Bool) -> Void) -> some View {
        Toggle(isOn: Binding(
            get: { isOn },
            set: { on in
                var p = device.permissions
                change(&p, on)
                error = model.setPermissions(device.id, p)
            }
        )) {
            Text(title).font(Theme.body).foregroundStyle(Theme.textPrimary)
        }
        .tint(Theme.connected)
        .padding(.horizontal, Theme.gutter)
        .frame(minHeight: 52)
        .accessibilityIdentifier("\(prefix).\(id)")
    }
}

/// The fixed list of what no device can ever approve, from the core.
struct NeverRemoteNote: View {
    @EnvironmentObject private var model: AppModel
    var identifier: String

    private var names: String {
        let titles = model.neverRemoteCategories.map { category -> String in
            RiskCategoryOption(category)?.title.lowercased() ?? riskCategoryTitle(category).lowercased()
        }
        return titles.joined(separator: ", ")
    }

    var body: some View {
        Text("Never approved remotely, whatever this switch says: \(names). Those are always approved on this phone.")
            .font(Theme.meta)
            .foregroundStyle(Theme.textTertiary)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityIdentifier(identifier)
    }
}

/// The pairing code and its QR, with the time left. The code is read from the model each
/// second and is gone once the offer is used, cancelled or expired; nothing keeps a copy.
private struct PairingSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gutter) {
            Chip(text: "Pair a device", icon: .qrCode, foreground: Theme.textSoft, fill: Theme.raisedHigh)
            TimelineView(.periodic(from: .now, by: 1)) { _ in
                let seconds = model.pairingSecondsLeft()
                if let offer = model.remotePairing, seconds > 0 {
                    offerView(offer, seconds: seconds)
                } else {
                    Text(model.remoteDevices.isEmpty ? "The code expired." : "Done. The code no longer works.")
                        .font(Theme.question)
                        .foregroundStyle(Theme.textPrimary)
                        .accessibilityIdentifier("remote.pair.ended")
                }
            }
            Spacer(minLength: 0)
            Button("Cancel") {
                model.cancelPairing()
                dismiss()
            }
            .buttonStyle(SecondaryButtonStyle())
            .accessibilityIdentifier("remote.pair.cancel")
        }
        .padding(.horizontal, 20)
        .padding(.top, 28)
        .padding(.bottom, 12)
        .presentationDetents([.large])
        .presentationBackground(Theme.sheet)
        .presentationCornerRadius(Theme.radiusSheet)
    }

    @ViewBuilder private func offerView(_ offer: RemotePairingOffer, seconds: Int) -> some View {
        VStack(alignment: .leading, spacing: Theme.gutter) {
            Text("On the other device, scan this or type the code. It works once.")
                .font(Theme.body)
                .foregroundStyle(Theme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
            if let qr = QRCode.image(offer.qrPayload.reveal()) {
                Image(decorative: qr, scale: 1)
                    .interpolation(.none)
                    .resizable()
                    .scaledToFit()
                    .frame(maxWidth: 240, maxHeight: 240)
                    .padding(12)
                    .background(RoundedRectangle(cornerRadius: Theme.radiusElement, style: .continuous).fill(Color.white))
                    .frame(maxWidth: .infinity)
                    .accessibilityElement()
                    .accessibilityLabel("Pairing QR code")
                    .accessibilityIdentifier("remote.pair.qr")
            }
            Text(offer.code.reveal())
                .font(.system(size: 20, weight: .semibold).monospaced())
                .foregroundStyle(Theme.textPrimary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
                .textSelection(.disabled)
                .accessibilityLabel("Pairing code")
                .accessibilityIdentifier("remote.pair.code")
            Text("Expires in \(seconds / 60):" + String(format: "%02d", seconds % 60))
                .font(Theme.mono)
                .foregroundStyle(Theme.textTertiary)
                .frame(maxWidth: .infinity)
                .accessibilityIdentifier("remote.pair.countdown")
        }
    }
}

/// QR images with CoreImage's `CIQRCodeGenerator`.
enum QRCode {
    static func image(_ payload: String, scale: CGFloat = 10) -> CGImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(payload.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage?.transformed(by: CGAffineTransform(scaleX: scale, y: scale)) else { return nil }
        return CIContext().createCGImage(output, from: output.extent)
    }
}

/// Everything remote control did and refused, newest first.
struct RemoteAuditView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Theme.gap) {
                ScreenTitle(title: "Audit log", subtitle: "Pairings, connections, requests and refusals.", identifier: "remote.audit.title")
                if model.remoteAudit.isEmpty {
                    Text("Nothing yet.").font(Theme.body).foregroundStyle(Theme.textSecondary)
                        .accessibilityIdentifier("remote.audit.empty")
                }
                VStack(spacing: 0) {
                    ForEach(Array(model.remoteAudit.enumerated()), id: \.element.id) { index, record in
                        if index > 0 { Divider().overlay(Theme.bezel) }
                        HStack(alignment: .firstTextBaseline, spacing: Theme.gap) {
                            Image.decorative(record.isWarning ? .warningOutline : .hollowDot, size: Theme.iconCompact)
                                .foregroundStyle(record.isWarning ? Theme.warning : Theme.textTertiary)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(record.text).font(Theme.body).foregroundStyle(Theme.textPrimary)
                                    .fixedSize(horizontal: false, vertical: true)
                                Text([record.device, record.at.formatted(date: .abbreviated, time: .standard)].compactMap { $0 }.joined(separator: " · "))
                                    .font(Theme.meta)
                                    .foregroundStyle(Theme.textSecondary)
                            }
                            Spacer(minLength: 0)
                        }
                        .padding(Theme.gap)
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("remote.audit.row.\(index)")
                    }
                }
                .card()
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 24)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
    }
}
