import Foundation
import JarvisCore
@testable import JarvisKit

/// Reads back a raw tree as `UiTreeBuilder.dump(_: RawElement)` prints it (the QA artifacts'
/// "raw XCUITest snapshot" sections): two spaces of indentation per level, then
/// `Type(raw number)`, ` label="…"`, ` title="…"`, ` value="…"`, ` placeholder="…"`, ` id=…`
/// (up to the frame), ` {{x, y}, {w, h}}` and ` [flags]`. For the tests that run the runner's
/// pure decisions on real trees.
enum RawTreeText {
    struct Failure: Error, CustomStringConvertible {
        let line: Int
        let why: String
        var description: String { "line \(line): \(why)" }
    }

    static func parse(_ text: String) throws -> RawElement {
        // (depth, element) for the open path from the root.
        var path: [(Int, RawElement)] = []
        var root: RawElement?
        func close(to depth: Int) {
            while let last = path.last, last.0 >= depth {
                path.removeLast()
                if var parent = path.popLast() {
                    parent.1.children.append(last.1)
                    path.append(parent)
                } else {
                    root = last.1
                }
            }
        }
        for (index, line) in text.split(separator: "\n", omittingEmptySubsequences: true).enumerated() {
            let spaces = line.prefix { $0 == " " }.count
            guard spaces % 2 == 0 else { throw Failure(line: index + 1, why: "odd indentation") }
            let depth = spaces / 2
            let element = try element(String(line.dropFirst(spaces)), line: index + 1)
            if depth == 0, root != nil || !path.isEmpty {
                throw Failure(line: index + 1, why: "a second root")
            }
            close(to: depth)
            guard depth == path.count else { throw Failure(line: index + 1, why: "depth \(depth) under \(path.count) open levels") }
            path.append((depth, element))
        }
        close(to: 0)
        guard let root else { throw Failure(line: 0, why: "no element") }
        return root
    }

    private static func element(_ text: String, line: Int) throws -> RawElement {
        var rest = Substring(text)
        guard let open = rest.firstIndex(of: "("), let shut = rest.firstIndex(of: ")"), open < shut,
              let type = UInt(rest[rest.index(after: open)..<shut]) else {
            throw Failure(line: line, why: "no Type(number)")
        }
        rest = rest[rest.index(after: shut)...]
        var e = RawElement(elementType: type, frame: Rect(x: 0, y: 0, width: 0, height: 0))
        func quoted(_ key: String) throws -> String? {
            let head = " \(key)=\""
            guard rest.hasPrefix(head) else { return nil }
            rest = rest.dropFirst(head.count)
            var out = ""
            while let c = rest.first {
                rest = rest.dropFirst()
                if c == "\"" { return out }
                if c == "\\", let next = rest.first {
                    rest = rest.dropFirst()
                    switch next {
                    case "n": out.append("\n")
                    case "r": out.append("\r")
                    case "t": out.append("\t")
                    default: out.append(next)
                    }
                } else {
                    out.append(c)
                }
            }
            throw Failure(line: line, why: "unterminated \(key)")
        }
        if let v = try quoted("label") { e.label = v }
        if let v = try quoted("title") { e.title = v }
        if let v = try quoted("value") { e.value = v }
        if let v = try quoted("placeholder") { e.placeholder = v }
        guard let frameStart = rest.range(of: " {{", options: .backwards) else {
            throw Failure(line: line, why: "no frame")
        }
        if rest.hasPrefix(" id=") {
            e.identifier = String(rest[rest.index(rest.startIndex, offsetBy: 4)..<frameStart.lowerBound])
        }
        let tail = rest[frameStart.upperBound...]
        guard let frameEnd = tail.range(of: "}}") else { throw Failure(line: line, why: "unterminated frame") }
        let numbers = tail[..<frameEnd.lowerBound]
            .replacingOccurrences(of: "}, {", with: ",")
            .split(separator: ",")
            .map { Double($0.trimmingCharacters(in: .whitespaces)) }
        guard numbers.count == 4, let x = numbers[0], let y = numbers[1], let w = numbers[2], let h = numbers[3] else {
            throw Failure(line: line, why: "frame \(numbers)")
        }
        e.frame = Rect(x: x, y: y, width: w, height: h)
        let flags = tail[frameEnd.upperBound...]
        if flags.contains("disabled") { e.isEnabled = false }
        if flags.contains("selected") { e.isSelected = true }
        if flags.contains("focus=?") {
            e.hasFocus = nil
        } else if flags.contains("focus") {
            e.hasFocus = true
        } else {
            e.hasFocus = false
        }
        return e
    }
}
