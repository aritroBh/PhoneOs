import SwiftUI
import UIKit
import JarvisCore
import JarvisKit

/// The confirmation gate (`HumanGate.confirm`): category chip, a picture of the screen from
/// when Jarvis asked with the target outlined (`ApprovalPreview`; the screenshot when "Show
/// the real screen when Jarvis asks" is on and a checked frame of that very observation was
/// kept, else its layout), question, the exact element line, Decline / Approve. The picture
/// gives way first when the sheet is small, so Decline and Approve stay where they are.
struct ConfirmationSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var prompt: HumanPrompt
    var request: ConfirmRequest
    @State private var images = ApprovalImageCache()
    @State private var enlarged = false

    /// The picture of this very question, while it is the one pending.
    private var preview: ApprovalPreview? {
        guard let preview = model.approvalPreview(for: prompt), preview.isLive(pending: model.prompt) else { return nil }
        return preview
    }

    /// The screenshot, never while the app is not active (app switcher snapshot).
    private func image(for preview: ApprovalPreview) -> UIImage? {
        guard scenePhase == .active else { return nil }
        return images.image(for: preview, in: model.approvalFrames)
    }

    private func open() {
        var transaction = Transaction()
        transaction.disablesAnimations = reduceMotion
        withTransaction(transaction) { enlarged = true }
    }

    /// The element line the model chose, when the request came from a model step.
    private var elementLine: String? {
        guard let index = model.live.chosen,
              let element = model.live.sight.first(where: { $0.index == index }) else { return nil }
        return element.line
    }

    private var appName: String {
        model.settings.appTrust.first { $0.bundleIdPrefix == request.app }?.name ?? request.app
    }

    private var wording: ConfirmationWording { ConfirmationWording(request, appName: appName) }

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gutter) {
            Chip(text: wording.chip, icon: .warning,
                 foreground: Theme.warning, fill: Theme.warningFill)
                .accessibilityIdentifier("confirm.category")
            if let preview {
                VStack(spacing: 6) {
                    ApprovalPreviewThumbnail(preview: preview, screen: model.approvalScreen(preview),
                                             image: image(for: preview), onOpen: { open() })
                        .frame(maxWidth: .infinity, minHeight: 0, maxHeight: Theme.approvalPreviewMaxHeight)
                    Text(preview.caption)
                        .font(Theme.meta)
                        .foregroundStyle(Theme.textSecondary)
                        .lineLimit(2)
                        .frame(maxWidth: .infinity, alignment: .center)
                        .accessibilityIdentifier("confirm.preview.caption")
                }
                .layoutPriority(-1)
                .fullScreenCover(isPresented: $enlarged) {
                    ApprovalPreviewFullScreen(preview: preview, screen: model.approvalScreen(preview),
                                              image: image(for: preview), onClose: { enlarged = false })
                        .preferredColorScheme(.dark)
                }
            }
            Text(wording.title)
                .font(Theme.question)
                .foregroundStyle(Theme.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("confirm.title")
            if let subtitle = wording.subtitle {
                Text(subtitle)
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textSecondary)
                    .accessibilityIdentifier("confirm.subtitle")
            }
            ConfirmationQuestionView(question: ConfirmationQuestion(request))
            VStack(alignment: .leading, spacing: 4) {
                Text(request.app)
                    .font(Theme.monoSmall)
                    .foregroundStyle(Theme.textTertiary)
                    .accessibilityIdentifier("confirm.app")
                if let elementLine {
                    Text(elementLine)
                        .font(Theme.mono)
                        .foregroundStyle(Theme.textPrimary)
                        .accessibilityIdentifier("confirm.element")
                }
            }
            .padding(Theme.gap)
            .frame(maxWidth: .infinity, alignment: .leading)
            .card(fill: Theme.well, radius: Theme.radiusElement)
            Spacer(minLength: 0)
                .layoutPriority(-2)
            HStack(spacing: Theme.gap) {
                Button("Decline") { model.decline(prompt) }
                    .buttonStyle(SecondaryButtonStyle())
                    .accessibilityIdentifier("confirm.decline")
                Button("Approve") { model.approve(prompt) }
                    .buttonStyle(PrimaryButtonStyle())
                    .accessibilityIdentifier("confirm.approve")
            }
            if model.launch.playground && model.launch.voiceScripted {
                // The voice UI test's evidence: scripted words reached the core while this
                // question was open (and approved nothing).
                Text("Heard while asking: \(model.voiceHeardDuringQuestion)")
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textTertiary)
                    .accessibilityIdentifier("voice.qa.heard")
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 28)
        .padding(.bottom, 12)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .presentationBackground(Theme.sheet)
        .presentationCornerRadius(Theme.radiusSheet)
        // The decoded screenshot goes with the question.
        .onChange(of: preview == nil) { _, gone in
            if gone { images.reset() }
        }
        .onDisappear { images.reset() }
    }
}

/// The core's question, part by part (`ConfirmationQuestion`, from the request's rows): the
/// action prominent; what is sent, the recipient fields and the amounts as labelled rows;
/// what the page itself says set apart in a secondary colour, labelled as from the page; the
/// warnings in the warning colour. A request without rows shows its summary. For VoiceOver
/// the question is one element (identifier `confirm.summary`) reading every line with its
/// label.
private struct ConfirmationQuestionView: View {
    var question: ConfirmationQuestion

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(question.lines) { line in
                QuestionLineView(line: line)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(question.spoken)
        .accessibilityIdentifier("confirm.summary")
    }
}

/// One line of the question.
private struct QuestionLineView: View {
    var line: ConfirmationQuestion.Line

    private var isPage: Bool { line.style == .page }

    private var font: Font {
        switch line.style {
        case .action: return Theme.rowTitle
        default: return Theme.body
        }
    }

    private var textColor: Color {
        switch line.style {
        case .action: return Theme.textPrimary
        case .fact: return Theme.textSoft
        case .page: return Theme.textSecondary
        case .warning: return Theme.warning
        case .plain: return Theme.textSoft
        }
    }

    private var labelColor: Color {
        line.style == .warning ? Theme.warning : Theme.textTertiary
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if let label = line.label {
                Text(label)
                    .font(Theme.meta)
                    .foregroundStyle(labelColor)
            }
            Text(line.text)
                .font(font)
                .foregroundStyle(textColor)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.leading, isPage ? 10 : 0)
        .overlay(alignment: .leading) {
            if isPage {
                Rectangle()
                    .fill(Theme.textTertiary)
                    .frame(width: 2)
            }
        }
    }
}

/// `HumanGate.answer`: the question and a text field.
struct QuestionSheet: View {
    @EnvironmentObject private var model: AppModel
    var prompt: HumanPrompt
    var question: String
    @State private var answer = ""

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gutter) {
            Chip(text: "Jarvis needs you", icon: .info, foreground: Theme.warning, fill: Theme.warningFill)
            Text(question)
                .font(Theme.question)
                .foregroundStyle(Theme.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("question.text")
            TextField("", text: $answer, prompt: Text("Your answer").foregroundColor(Theme.textTertiary))
                .font(.system(size: 16))
                .foregroundStyle(Theme.textPrimary)
                .padding(.horizontal, 14)
                .frame(minHeight: Theme.composerHeight)
                .modifier(Bezeled(fill: Theme.surface, radius: Theme.radiusButton, stroke: Theme.controlBezel))
                .accessibilityLabel("Your answer")
                .accessibilityIdentifier("question.field")
            Spacer(minLength: 0)
            HStack(spacing: Theme.gap) {
                Button("Stop task") { model.stop() }
                    .buttonStyle(SecondaryButtonStyle())
                    .accessibilityIdentifier("question.stop")
                Button("Send") { model.reply(prompt, answer) }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(answer.trimmingCharacters(in: .whitespaces).isEmpty)
                    .accessibilityIdentifier("question.send")
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 28)
        .padding(.bottom, 12)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .presentationBackground(Theme.sheet)
        .presentationCornerRadius(Theme.radiusSheet)
    }
}

/// A workflow's "Ask you" step: the question, No / Yes.
struct YesNoSheet: View {
    @EnvironmentObject private var model: AppModel
    var prompt: HumanPrompt
    var question: String

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gutter) {
            Chip(text: "Workflow asks", icon: .workflows, foreground: Theme.warning, fill: Theme.warningFill)
            Text(question)
                .font(Theme.question)
                .foregroundStyle(Theme.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("yesno.text")
            Spacer(minLength: 0)
            HStack(spacing: Theme.gap) {
                Button("No") { model.answer(prompt, yes: false) }
                    .buttonStyle(SecondaryButtonStyle())
                    .accessibilityIdentifier("yesno.no")
                Button("Yes") { model.answer(prompt, yes: true) }
                    .buttonStyle(PrimaryButtonStyle())
                    .accessibilityIdentifier("yesno.yes")
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 28)
        .padding(.bottom, 12)
        .presentationDetents([.medium])
        .presentationDragIndicator(.visible)
        .presentationBackground(Theme.sheet)
        .presentationCornerRadius(Theme.radiusSheet)
    }
}
