import SwiftUI
import UIKit

/// The Swipe rows, UIKit screen of probe P13h (`TreeFactProbe3UIKitTests`): a plain-style
/// `UITableView` and a `UICollectionView` list (`UICollectionLayoutListConfiguration`), one
/// under the other, each with trailing swipe actions (Delete, Flag) and a leading one
/// (Remove), so the probe can read what the accessibility tree shows while a row's actions
/// are revealed (the earlier probes measured SwiftUI `List` + `.swipeActions` only).
/// Playground only: nothing here runs in a real task.
struct Probe3UIKitSwipesFixture: View {
    @State private var status = "none"

    var body: some View {
        VStack(spacing: 8) {
            Text(status)
                .accessibilityIdentifier("uikit.status")
                .padding(.top, 4)
            Probe3UIKitTableHost(report: { status = $0 })
                .frame(height: 320)
            Probe3UIKitCollectionHost(report: { status = $0 })
                .frame(height: 320)
            Spacer(minLength: 0)
        }
        .navigationTitle("Swipe rows, UIKit")
        .navigationBarTitleDisplayMode(.inline)
    }
}

private struct Probe3UIKitTableHost: UIViewControllerRepresentable {
    let report: (String) -> Void

    func makeUIViewController(context: Context) -> Probe3UIKitTableController {
        let controller = Probe3UIKitTableController()
        controller.report = report
        return controller
    }

    func updateUIViewController(_ controller: Probe3UIKitTableController, context: Context) {
        controller.report = report
    }
}

private struct Probe3UIKitCollectionHost: UIViewControllerRepresentable {
    let report: (String) -> Void

    func makeUIViewController(context: Context) -> Probe3UIKitCollectionController {
        let controller = Probe3UIKitCollectionController()
        controller.report = report
        return controller
    }

    func updateUIViewController(_ controller: Probe3UIKitCollectionController, context: Context) {
        controller.report = report
    }
}

/// A plain `UITableView` of "Table row 1" to "Table row 8". The data is changed first, then the
/// table; an action on a row that is no longer in the data ends with `false`.
final class Probe3UIKitTableController: UIViewController, UITableViewDataSource, UITableViewDelegate {
    private let table = UITableView(frame: .zero, style: .plain)
    private var rows = Array(1...8)
    var report: (String) -> Void = { _ in }

    override func viewDidLoad() {
        super.viewDidLoad()
        table.frame = view.bounds
        table.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        table.accessibilityIdentifier = "uikit.table"
        table.dataSource = self
        table.delegate = self
        table.register(UITableViewCell.self, forCellReuseIdentifier: "row")
        view.addSubview(table)
    }

    func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
        rows.count
    }

    func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = tableView.dequeueReusableCell(withIdentifier: "row", for: indexPath)
        let number = indexPath.row < rows.count ? rows[indexPath.row] : 0
        var content = cell.defaultContentConfiguration()
        content.text = "Table row \(number)"
        cell.contentConfiguration = content
        cell.accessibilityIdentifier = "uikit.table.row.\(number)"
        return cell
    }

    func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath, animated: true)
        guard indexPath.row < rows.count else { return }
        report("Selected table row \(rows[indexPath.row])")
    }

    func tableView(_ tableView: UITableView, trailingSwipeActionsConfigurationForRowAt indexPath: IndexPath) -> UISwipeActionsConfiguration? {
        guard indexPath.row < rows.count else { return nil }
        let number = rows[indexPath.row]
        let delete = UIContextualAction(style: .destructive, title: "Delete") { [weak self] _, _, completion in
            guard let self else {
                completion(false)
                return
            }
            completion(self.remove(number, verb: "Deleted"))
        }
        let flag = UIContextualAction(style: .normal, title: "Flag") { [weak self] _, _, completion in
            guard let self, self.rows.contains(number) else {
                completion(false)
                return
            }
            self.report("Flagged table row \(number)")
            completion(true)
        }
        flag.backgroundColor = .systemOrange
        return UISwipeActionsConfiguration(actions: [delete, flag])
    }

    func tableView(_ tableView: UITableView, leadingSwipeActionsConfigurationForRowAt indexPath: IndexPath) -> UISwipeActionsConfiguration? {
        guard indexPath.row < rows.count else { return nil }
        let number = rows[indexPath.row]
        let removeAction = UIContextualAction(style: .destructive, title: "Remove") { [weak self] _, _, completion in
            guard let self else {
                completion(false)
                return
            }
            completion(self.remove(number, verb: "Removed"))
        }
        return UISwipeActionsConfiguration(actions: [removeAction])
    }

    /// Removes the row numbered `number` from the data, then from the table.
    private func remove(_ number: Int, verb: String) -> Bool {
        guard let index = rows.firstIndex(of: number) else { return false }
        rows.remove(at: index)
        table.deleteRows(at: [IndexPath(row: index, section: 0)], with: .automatic)
        report("\(verb) table row \(number)")
        return true
    }
}

/// A `UICollectionView` list (inset grouped) of "Collection row 1" to "Collection row 8".
final class Probe3UIKitCollectionController: UIViewController, UICollectionViewDataSource, UICollectionViewDelegate {
    private var collection: UICollectionView?
    private var rows = Array(1...8)
    private let registration = UICollectionView.CellRegistration<UICollectionViewListCell, Int> { cell, indexPath, number in
        var content = cell.defaultContentConfiguration()
        content.text = "Collection row \(number)"
        cell.contentConfiguration = content
        cell.accessibilityIdentifier = "uikit.collection.row.\(number)"
    }
    var report: (String) -> Void = { _ in }

    override func viewDidLoad() {
        super.viewDidLoad()
        var config = UICollectionLayoutListConfiguration(appearance: .insetGrouped)
        config.trailingSwipeActionsConfigurationProvider = { [weak self] indexPath in
            self?.trailingActions(indexPath)
        }
        config.leadingSwipeActionsConfigurationProvider = { [weak self] indexPath in
            self?.leadingActions(indexPath)
        }
        let layout = UICollectionViewCompositionalLayout.list(using: config)
        let list = UICollectionView(frame: view.bounds, collectionViewLayout: layout)
        list.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        list.accessibilityIdentifier = "uikit.collection"
        list.dataSource = self
        list.delegate = self
        view.addSubview(list)
        collection = list
    }

    func collectionView(_ collectionView: UICollectionView, numberOfItemsInSection section: Int) -> Int {
        rows.count
    }

    func collectionView(_ collectionView: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell {
        let number = indexPath.item < rows.count ? rows[indexPath.item] : 0
        return collectionView.dequeueConfiguredReusableCell(using: registration, for: indexPath, item: number)
    }

    func collectionView(_ collectionView: UICollectionView, didSelectItemAt indexPath: IndexPath) {
        collectionView.deselectItem(at: indexPath, animated: true)
        guard indexPath.item < rows.count else { return }
        report("Selected collection row \(rows[indexPath.item])")
    }

    private func trailingActions(_ indexPath: IndexPath) -> UISwipeActionsConfiguration? {
        guard indexPath.item < rows.count else { return nil }
        let number = rows[indexPath.item]
        let delete = UIContextualAction(style: .destructive, title: "Delete") { [weak self] _, _, completion in
            guard let self else {
                completion(false)
                return
            }
            completion(self.remove(number, verb: "Deleted"))
        }
        let flag = UIContextualAction(style: .normal, title: "Flag") { [weak self] _, _, completion in
            guard let self, self.rows.contains(number) else {
                completion(false)
                return
            }
            self.report("Flagged collection row \(number)")
            completion(true)
        }
        flag.backgroundColor = .systemOrange
        return UISwipeActionsConfiguration(actions: [delete, flag])
    }

    private func leadingActions(_ indexPath: IndexPath) -> UISwipeActionsConfiguration? {
        guard indexPath.item < rows.count else { return nil }
        let number = rows[indexPath.item]
        let removeAction = UIContextualAction(style: .destructive, title: "Remove") { [weak self] _, _, completion in
            guard let self else {
                completion(false)
                return
            }
            completion(self.remove(number, verb: "Removed"))
        }
        return UISwipeActionsConfiguration(actions: [removeAction])
    }

    /// Removes the row numbered `number` from the data, then from the collection.
    private func remove(_ number: Int, verb: String) -> Bool {
        guard let index = rows.firstIndex(of: number), let collection else { return false }
        rows.remove(at: index)
        collection.deleteItems(at: [IndexPath(item: index, section: 0)])
        report("\(verb) collection row \(number)")
        return true
    }
}
