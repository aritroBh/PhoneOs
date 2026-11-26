import SwiftUI
import JarvisCore
import JarvisKit

/// "Control another phone": this device as the controller of another iPhone that runs
/// Jarvis (`RemoteControllerModel` in JarvisKit). Pairing (address and the code shown on
/// that phone), the connection, a task composer, Stop, the status timeline, the approval
/// card for questions that phone lets this device answer (text only, never a picture;
/// "Answer on the phone" otherwise) and its Live view (wireframes, cursor, captions) when
/// that phone allows "Watch the screen layout". That phone decides everything; this screen
/// shows only what it sent and says what it does not share. Pushed from Remote control.
struct ControllerView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var browser = RemotePhoneBrowser()
    @State private var host = ""
    @State private var port = ""
    @State private var code = ""
    /// The name this device gives the phone (filled from the device when the screen appears).
    @State private var deviceName = ""
    @State private var task = ""
    @State private var working = false
    @State private var error: String?
    @State private var confirmingForget = false
    @FocusState private var composerFocused: Bool

    private var state: RemoteControllerModel.State { model.controllerState }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                ScreenTitle(title: "Control another phone",
                            subtitle: "Start and watch tasks on another iPhone that runs Jarvis. That phone stays in charge.",
                            identifier: "controller.title")
                statusCard
                if let message = error ?? state.lastError {
                    InlineError(text: message, identifier: "controller.error")
                }
                if !state.isPaired {
                    pairingCard
                } else {
                    if state.isConnected {
                        if let question = state.question { QuestionCard(question: question, error: $error) }
                        taskCard
                        liveSection
                    }
                    entriesSection
                    connectionButtons
                }
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 24)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            model.controllerActivate()
            if deviceName.isEmpty { deviceName = model.controllerDefaultDeviceName }
            if !model.launch.playground && !state.isPaired { browser.start() }
        }
        .onDisappear { browser.stop() }
        .onChange(of: state.isPaired) { _, paired in
            if paired { browser.stop() }
        }
        .confirmationDialog("Forget this phone?", isPresented: $confirmingForget, titleVisibility: .visible) {
            Button("Forget", role: .destructive) { model.controllerForget() }
                .accessibilityIdentifier("controller.confirmForget")
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This device forgets its key for that phone. The phone keeps listing it until its owner removes it there.")
        }
    }

    // MARK: Status

    private var statusColor: Color {
        switch state.phase {
        case .notPaired: return Theme.textTertiary
        case .pairing, .connecting: return Theme.warning
        case .disconnected(let d): return d.retrying ? Theme.warning : Theme.error
        case .connected: break
        }
        if state.awaitingConfirmation != nil || state.checkUnverified || state.question != nil { return Theme.warning }
        switch state.activity {
        case .running: return Theme.tally
        case .ended(let e): return e.status == .success ? Theme.connected : Theme.error
        default: return Theme.connected
        }
    }

    private var statusCard: some View {
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            HStack(spacing: Theme.gapSmall) {
                StatusDot(color: statusColor, pulsing: state.isBusy || state.phase == .pairing)
                Text(state.statusTitle)
                    .font(Theme.rowTitle)
                    .foregroundStyle(Theme.textPrimary)
                Spacer(minLength: 0)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("controller.status")
            if state.isPaired {
                Text([state.phoneName, state.address?.description].compactMap { $0 }.joined(separator: " \u{00B7} "))
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textSecondary)
                    .accessibilityIdentifier("controller.phone")
            }
            if let prompt = state.confirmationPrompt, state.isPaired {
                Text(prompt)
                    .font(Theme.rowTitle)
                    .foregroundStyle(Theme.warning)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("controller.confirmCheck")
                Text("That phone lists this device under Waiting for your confirmation. Confirm it there only if it shows these same numbers. If it shows other numbers or none, remove it there and forget the phone here.")
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("controller.confirmNote")
            } else if let reminder = state.checkReminder, let check = state.pairingCheck {
                // Being let in is not proof that the phone's owner confirmed THIS device (a
                // fake phone that saw the code lets it in at once): the numbers stay until
                // the owner says they matched.
                Text(check)
                    .font(.system(size: 28, weight: .semibold).monospaced())
                    .foregroundStyle(Theme.warning)
                    .accessibilityLabel(check)
                    .accessibilityIdentifier("controller.check")
                Text(reminder)
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("controller.checkReminder")
                Button("The numbers matched") { model.controllerCheckMatched() }
                    .buttonStyle(SecondaryButtonStyle())
                    .accessibilityIdentifier("controller.checkMatched")
            }
            if case .disconnected(let d) = state.phase {
                Text(d.reason.text)
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("controller.disconnectReason")
            }
            if case .ended(let e) = state.activity, state.isConnected, state.question == nil {
                Text(e.summary)
                    .font(Theme.body)
                    .foregroundStyle(Theme.textSoft)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("controller.outcome")
            } else if let progress = state.progress, state.isBusy {
                ProgressLines(progress: progress)
            }
            if let note = state.sharingNote, state.isConnected {
                Text(note)
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textTertiary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("controller.sharingNote")
            }
            if let refusal = state.lastRefusal {
                Label {
                    Text(refusal).font(Theme.meta).fixedSize(horizontal: false, vertical: true)
                } icon: {
                    Image.decorative(.denied, size: Theme.iconCompact)
                }
                .foregroundStyle(Theme.warning)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("controller.refusal")
            }
        }
        .padding(Theme.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
    }

    // MARK: Pairing

    private var pairingCard: some View {
        VStack(alignment: .leading, spacing: Theme.gap) {
            Text("On the other phone: Control \u{2192} Remote control, turn it on, then Pair a device. Its address and port are under Connection details there.")
                .font(Theme.meta)
                .foregroundStyle(Theme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
            if !browser.names.isEmpty {
                SectionLabel(text: "On this Wi-Fi")
                VStack(spacing: 0) {
                    ForEach(Array(browser.names.enumerated()), id: \.element) { index, name in
                        if index > 0 { Divider().overlay(Theme.bezel) }
                        Button {
                            pair(.service(name: name))
                        } label: {
                            HStack(spacing: Theme.gap) {
                                Image.decorative(.runner, size: Theme.iconStandard).foregroundStyle(Theme.textSoft)
                                Text(name).font(Theme.body).foregroundStyle(Theme.textPrimary)
                                Spacer(minLength: 0)
                                Text("Pair").font(Theme.meta.weight(.semibold)).foregroundStyle(Theme.textSecondary)
                            }
                            .padding(.horizontal, Theme.gutter)
                            .frame(minHeight: 52)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(working || code.isEmpty)
                        .accessibilityIdentifier("controller.found.\(index)")
                    }
                }
                .card(fill: Theme.raised, radius: Theme.radiusElement)
            }
            field("Address", text: $host, prompt: "192.168.1.20", identifier: "controller.host")
                .keyboardType(.numbersAndPunctuation)
            field("Port", text: $port, prompt: "51234", identifier: "controller.port")
                .keyboardType(.numberPad)
            codeField
            field("This device's name", text: $deviceName, prompt: "Work iPad", identifier: "controller.deviceName",
                  capitalization: .words)
            Text("The code works once, for 5 minutes. After 5 wrong codes the phone pauses that code for a while. The phone then shows this device's name and six digits: confirm it there.")
                .font(Theme.meta)
                .foregroundStyle(Theme.textTertiary)
                .fixedSize(horizontal: false, vertical: true)
            Button {
                Task {
                    working = true
                    error = await model.controllerPair(code: code, host: host, port: port, name: deviceName)
                    if error == nil { code = "" }
                    working = false
                }
            } label: {
                Text(working ? "Pairing\u{2026}" : "Pair")
            }
            .buttonStyle(PrimaryButtonStyle())
            .disabled(working || code.trimmingCharacters(in: .whitespaces).isEmpty || host.isEmpty || port.isEmpty)
            .accessibilityIdentifier("controller.pair")
            if model.launch.playground {
                Button("Pair with this phone (QA)") {
                    Task {
                        working = true
                        error = await model.controllerPairWithThisPhoneForQA(name: deviceName)
                        working = false
                    }
                }
                .buttonStyle(SecondaryButtonStyle())
                .disabled(working)
                .accessibilityIdentifier("controller.qa.pairHere")
            }
        }
        .padding(Theme.gutter)
        .card()
    }

    private func pair(_ address: RemotePhoneAddress) {
        Task {
            working = true
            error = await model.controllerPair(code: Redacted(code), address: address, name: deviceName)
            if error == nil { code = "" }
            working = false
        }
    }

    /// The pairing code: kept out of the accessibility value (VoiceOver and UI tests see
    /// only its label), redacted in screenshots of a locked screen, offered as a one-time
    /// code, never autocorrected or capitalised by the keyboard.
    private var codeField: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Pairing code").font(Theme.meta.weight(.semibold)).foregroundStyle(Theme.textSecondary)
            TextField("", text: $code, prompt: Text("ABCD-EFGH-\u{2026}").foregroundColor(Theme.textTertiary))
                .font(Theme.mono.weight(.semibold))
                .foregroundStyle(Theme.textPrimary)
                .textContentType(.oneTimeCode)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .privacySensitive()
                .padding(.horizontal, 12)
                .frame(minHeight: Theme.minTarget)
                .modifier(Bezeled(fill: Theme.well, radius: Theme.radiusField, stroke: Theme.controlBezel))
                .accessibilityLabel("Pairing code")
                .accessibilityValue(Text(""))
                .accessibilityIdentifier("controller.code")
        }
    }

    private func field(_ title: String, text: Binding<String>, prompt: String, identifier: String, mono: Bool = false,
                       capitalization: TextInputAutocapitalization = .never) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(Theme.meta.weight(.semibold)).foregroundStyle(Theme.textSecondary)
            TextField("", text: text, prompt: Text(prompt).foregroundColor(Theme.textTertiary))
                .font(mono ? Theme.mono.weight(.semibold) : Theme.body)
                .foregroundStyle(Theme.textPrimary)
                .autocorrectionDisabled()
                .textInputAutocapitalization(capitalization)
                .padding(.horizontal, 12)
                .frame(minHeight: Theme.minTarget)
                .modifier(Bezeled(fill: Theme.well, radius: Theme.radiusField, stroke: Theme.controlBezel))
                .accessibilityLabel(title)
                .accessibilityIdentifier(identifier)
        }
    }

    // MARK: Task

    private var taskCard: some View {
        VStack(alignment: .leading, spacing: Theme.gap) {
            HStack(spacing: Theme.gapSmall) {
                TextField("", text: $task, prompt: Text("Ask that phone to do something").foregroundColor(Theme.textTertiary))
                    .font(.system(size: 16))
                    .foregroundStyle(Theme.textPrimary)
                    .focused($composerFocused)
                    .submitLabel(.go)
                    .onSubmit(send)
                    .padding(.leading, 14)
                    .frame(minHeight: Theme.minTarget)
                    .disabled(!state.canStartTask)
                    .accessibilityLabel("Task for the other phone")
                    .accessibilityIdentifier("controller.composer")
                Button(action: send) {
                    Image.decorative(.send, size: Theme.iconStandard)
                        .foregroundStyle(Theme.primaryText)
                        .frame(width: Theme.minTarget, height: Theme.minTarget)
                        .background(Circle().fill(Theme.primaryFill.opacity(state.canStartTask ? 1 : 0.35)))
                }
                .buttonStyle(.plain)
                .disabled(!state.canStartTask || task.trimmingCharacters(in: .whitespaces).isEmpty)
                .accessibilityLabel("Start task on the other phone")
                .accessibilityIdentifier("controller.send")
            }
            .padding(.horizontal, 6)
            .frame(height: Theme.composerHeight)
            .modifier(Bezeled(fill: Theme.surface, radius: Theme.radiusComposer, stroke: Theme.controlBezel))
            if state.permissions?.startTasks == false {
                Text("That phone does not allow this device to start tasks.")
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textTertiary)
                    .accessibilityIdentifier("controller.noStart")
            }
            if state.canStop {
                Button {
                    error = model.controllerStop()
                } label: {
                    Label {
                        Text("Stop")
                    } icon: {
                        Image.decorative(.stop, size: Theme.iconStandard)
                    }
                }
                .buttonStyle(SecondaryButtonStyle())
                .accessibilityIdentifier("controller.stop")
            }
        }
    }

    private func send() {
        let text = task
        guard !text.trimmingCharacters(in: .whitespaces).isEmpty, state.canStartTask else { return }
        composerFocused = false
        error = model.controllerStart(text)
        if error == nil { task = "" }
    }

    // MARK: Live view

    @ViewBuilder private var liveSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionLabel(text: "Screen layout")
            switch state.live {
            case .watching, .starting:
                LiveScreenView(viewer: model.controller.live.model, timeline: model.controllerTimeline,
                               frames: model.controllerFrames, identifierPrefix: "controller.live")
                Button("Stop watching") { error = model.controllerWatch(false) }
                    .buttonStyle(SecondaryButtonStyle())
                    .accessibilityIdentifier("controller.stopWatch")
            default:
                VStack(alignment: .leading, spacing: Theme.gap) {
                    Text(liveNote)
                        .font(Theme.meta)
                        .foregroundStyle(Theme.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("controller.live.note")
                    Button("Watch the screen layout") { error = model.controllerWatch(true) }
                        .buttonStyle(SecondaryButtonStyle())
                        .disabled(!state.canWatch)
                        .accessibilityIdentifier("controller.watch")
                }
                .padding(Theme.gutter)
                .card()
            }
            Text("Outlines of the screen, where Jarvis taps, and step captions. Never pictures of the screen, passwords or the lock screen.")
                .font(Theme.meta)
                .foregroundStyle(Theme.textTertiary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 4)
        }
    }

    private var liveNote: String {
        switch state.live {
        case .denied:
            "That phone does not share its screen layout with this device. Its owner can turn on \u{201C}Watch the screen layout\u{201D} for it in Remote control."
        case .ended(let reason):
            reason.text + "."
        case .incompatible:
            "That phone streams a newer format. Update Jarvis on this device."
        default:
            "Ask that phone for its screen layout. It shows it only if its owner allowed this device."
        }
    }

    // MARK: Timeline and connection

    private var entriesSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionLabel(text: "Status")
            let rows = Array(state.entries.suffix(30).reversed())
            VStack(spacing: 0) {
                if rows.isEmpty {
                    Text("Nothing yet.")
                        .font(Theme.body)
                        .foregroundStyle(Theme.textSecondary)
                        .padding(Theme.gutter)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                ForEach(Array(rows.enumerated()), id: \.element.id) { index, entry in
                    if index > 0 { Divider().overlay(Theme.bezel) }
                    HStack(alignment: .firstTextBaseline, spacing: Theme.gap) {
                        Image.decorative(entry.kind == .refusal ? .warningOutline : .hollowDot, size: Theme.iconCompact)
                            .foregroundStyle(entry.kind == .refusal ? Theme.warning : Theme.textTertiary)
                        Text(entry.text)
                            .font(Theme.body)
                            .foregroundStyle(Theme.textPrimary)
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 0)
                    }
                    .padding(Theme.gap)
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("controller.entry.\(index)")
                }
            }
            .card()
        }
    }

    @ViewBuilder private var connectionButtons: some View {
        HStack(spacing: Theme.gap) {
            switch state.phase {
            case .connected, .connecting:
                Button("Disconnect") { model.controllerDisconnect() }
                    .buttonStyle(SecondaryButtonStyle())
                    .accessibilityIdentifier("controller.disconnect")
            case .disconnected(let d) where !d.retrying:
                Button("Reconnect") { model.controllerReconnect() }
                    .buttonStyle(SecondaryButtonStyle())
                    .accessibilityIdentifier("controller.reconnect")
            default:
                EmptyView()
            }
            Button("Forget") { confirmingForget = true }
                .buttonStyle(SecondaryButtonStyle(foreground: Theme.errorText))
                .accessibilityIdentifier("controller.forget")
        }
    }
}

/// The step line the phone sent: detail only where it shares it.
private struct ProgressLines: View {
    var progress: RemoteControllerModel.Progress

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if progress.waitingForUnlock {
                Text("Waiting for the phone to be unlocked").font(Theme.body).foregroundStyle(Theme.warning)
            } else if progress.coarse {
                Text("Working").font(Theme.body).foregroundStyle(Theme.textSoft)
            } else {
                Text(["Step \(progress.step)", RemoteControllerModel.phaseTitle(progress.phase), progress.app]
                    .compactMap { $0 }.joined(separator: " \u{00B7} "))
                    .font(Theme.mono)
                    .foregroundStyle(Theme.textSoft)
                if let text = progress.screenText, !text.isEmpty {
                    Text(text)
                        .font(Theme.meta)
                        .foregroundStyle(Theme.textTertiary)
                        .lineLimit(3)
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("controller.progress")
    }
}

/// The phone's question as text rows only (never a picture). Answer buttons only where that
/// phone said this device may answer; otherwise "Answer on the phone".
private struct QuestionCard: View {
    @EnvironmentObject private var model: AppModel
    var question: RemoteControllerModel.Question
    @Binding var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gap) {
            Chip(text: riskCategoryTitle(question.category), icon: .askOwner, foreground: Theme.warning, fill: Theme.warningFill)
                .accessibilityIdentifier("controller.question.category")
            Text(question.summary)
                .font(Theme.rowTitle)
                .foregroundStyle(Theme.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("controller.question.summary")
            if question.canApprove {
                HStack(spacing: Theme.gap) {
                    Button("Decline") { error = model.controllerAnswer(approved: false) }
                        .buttonStyle(SecondaryButtonStyle())
                        .accessibilityIdentifier("controller.decline")
                    Button("Approve") { error = model.controllerAnswer(approved: true) }
                        .buttonStyle(PrimaryButtonStyle())
                        .accessibilityIdentifier("controller.approve")
                }
            } else {
                Label {
                    Text("Answer on the phone")
                } icon: {
                    Image.decorative(.lock, size: Theme.iconCompact)
                }
                .font(Theme.meta.weight(.semibold))
                .foregroundStyle(Theme.textSoft)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("controller.answerOnPhone")
            }
        }
        .padding(Theme.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(fill: Theme.raised)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("controller.question")
    }
}
