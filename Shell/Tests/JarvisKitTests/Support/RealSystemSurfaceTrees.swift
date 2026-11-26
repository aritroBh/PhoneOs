import Foundation

// Generated from the raw XCUITest snapshots ("== raw XCUITest snapshot" sections, copied
// verbatim) of SpringBoard that the Mac QA run 139 (probe round 3) attached (iPhone 17 Pro Max
// simulator, iOS 26.5 build 23F77): every SpringBoard capture of that run. Each tree is
// SpringBoard's application element as `UiTreeBuilder.dump(_: RawElement)` prints it. Parsed
// by `RawTreeText`. With `RealSpringBoardTrees` (runs 134, 135, 137) the corpus of the system
// surface rule (`SystemSurface`).

/// Real SpringBoard trees of run 139 (iOS 26.5 simulator), for the system surface rule.
enum RealSystemSurfaceTrees {
    enum Kind: String, CaseIterable {
        /// Control Centre is up over an app.
        case controlCentre
        /// Notification Centre is up over an app (unlocked).
        case notificationCentre
        /// The App Switcher is up.
        case appSwitcher
        /// SpringBoard shows its Home Screen.
        case home
        /// An app is in front (SpringBoard's tree taken while it was).
        case app
        /// A SpringBoard alert is up over an app.
        case alert
    }

    struct Capture {
        let name: String
        let kind: Kind
        /// Where it was captured.
        let source: String
        /// What was in front or showing.
        let showing: String
        let text: String
    }

    /// Notification Centre pulled down over Settings, unlocked (two notification rows) (run 139, probe round 3 p12a capture 03 (notification-centre); 239 nodes).
    static let notificationCentre = Capture(
        name: "notificationCentre", kind: .notificationCentre,
        source: "run 139, probe round 3 p12a capture 03 (notification-centre)",
        showing: "Notification Centre pulled down over Settings, unlocked (two notification rows)",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) id=SBCoverSheetWindow {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 956}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 1912}}
            Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
            Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 100}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                ScrollView(46) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, 0}, {440, 956}}
                            ScrollView(46) {{14, 0.2}, {412, 226.3}}
                              ScrollView(46) {{14, 0.3}, {412, 226.3}}
                                Button(9) id=ListCell {{14, 0.3}, {412, 226.0}}
                                  Other(1) id=ListCell.ContentView {{14, 0.3}, {412, 226.0}}
                                    Other(1) {{14, 0.3}, {412, 226.0}}
                                      Other(1) {{14, 0.3}, {412, 226}}
                                        Other(1) {{14, 0.3}, {412, 226}}
                                        Other(1) {{14, 0.3}, {412, 226}}
                                        Other(1) {{14, 0.3}, {412, 226}}
                                          Other(1) {{14, 0.3}, {412, 226.0}}
                                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 48.3}, {30, 178.3}}
                                  Other(1) {{420, 65.3}, {3, 158.3}}
                                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 48.3}, {30, 178.3}}
                                  Other(1) {{420, 65.3}, {3, 158.3}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 48.2}, {30, 178.3}}
                                Other(1) {{420, 65.2}, {3, 158.3}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 48.2}, {30, 178.3}}
                                Other(1) {{420, 65.2}, {3, 158.3}}
                            ScrollView(46) {{14, 590}, {412, 66}}
                              ScrollView(46) {{14, 590}, {412, 66}}
                                Button(9) label="JARVIS, 9 minutes ago, Jarvis probe, Probe banner 4417" id=ListCell {{14, 590}, {412, 66}}
                                  Other(1) id=ListCell.ContentView {{14, 590}, {412, 66}}
                                    Other(1) {{14, 590}, {412, 66}}
                                      Other(1) {{14, 590}, {412, 66}}
                                        Type83(83) label="JARVIS, 9 minutes ago, Jarvis probe, Probe banner 4417" id=NotificationShortLookView {{14, 590}, {412, 66}}
                                          Other(1) id=ShortLook.Platter {{14, 590}, {412, 66}}
                                            Other(1) {{14, 590}, {412, 66}}
                                            Other(1) {{14, 590}, {412, 66}}
                                            Other(1) id=ShortLook.Platter.Content {{14, 590}, {412, 66}}
                                              Button(9) label="9 minutes ago, Jarvis probe, Probe banner 4417" id=ShortLook.Platter.Content.Seamless {{14, 590}, {412, 66}}
                                                Other(1) id=ShortLook.Platter.Content.Seamless.CrossFade {{14, 590}, {412, 66}}
                                                  StaticText(48) label="Jarvis probe" id=NotificationTitle {{78, 604}, {275.3, 18}}
                                                  Other(1) id=NotificationBody.TopAligned.originalMessage {{78, 622}, {331, 18}}
                                                    StaticText(48) label="Probe banner 4417" id=TextContent.Primary {{78, 622}, {331, 18}}
                                              Other(1) {{14, 590}, {412, 66}}
                                                Other(1) {{14, 590}, {412, 66}}
                                                  Other(1) {{14, 590}, {412, 66}}
                                                    Other(1) {{14, 590}, {412, 66}}
                                                      Other(1) {{14, 590}, {412, 66}}
                                                        Other(1) {{14, 590}, {412, 66}}
                                                    Other(1) {{14, 590}, {412, 66}}
                                                      Other(1) {{14, 590}, {412, 66}}
                                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 623}, {30, 33}}
                                  Other(1) {{420, 617}, {3, 36}}
                                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 623}, {30, 33}}
                                  Other(1) {{420, 617}, {3, 36}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 590}, {30, 66}}
                                Other(1) {{420, 593}, {3, 72.7}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 590}, {30, 66}}
                                Other(1) {{420, 593}, {3, 72.7}}
                            ScrollView(46) {{14, 663.8}, {412, 143.7}}
                              Other(1) {{14, 663.8}, {412, 56}}
                                Other(1) {{24, 678.2}, {227.3, 33.7}}
                                  StaticText(48) label="Notification Center" {{24, 678.2}, {227.3, 33.7}}
                                Button(9) label="Clear" value="Notification Center" id=clear-button {{370, 677.8}, {56, 34}}
                              ScrollView(46) {{14, 719.8}, {412, 78.7}}
                                Button(9) label="SETTINGS, 29 minutes ago, Ready for Apple Intelligence, Time to experience the new personal intelligence system." id=ListCell {{14, 719.8}, {412, 78.7}}
                                  Other(1) id=ListCell.ContentView {{14, 719.8}, {412, 78.7}}
                                    Other(1) {{14, 719.8}, {412, 78.7}}
                                      Other(1) {{14, 719.8}, {412, 78.7}}
                                        Type83(83) label="SETTINGS, 29 minutes ago, Ready for Apple Intelligence, Time to experience the new personal intelligence system." id=NotificationShortLookView {{14, 719.8}, {412, 78.7}}
                                          Other(1) id=ShortLook.Platter {{14, 719.8}, {412, 78.7}}
                                            Other(1) {{14, 719.8}, {412, 78.7}}
                                            Other(1) {{14, 719.8}, {412, 78.7}}
                                            Other(1) id=ShortLook.Platter.Content {{14, 719.8}, {412, 78.7}}
                                              Button(9) label="29 minutes ago, Ready for Apple Intelligence, Time to experience the new personal intelligence system." id=ShortLook.Platter.Content.Seamless {{14, 719.8}, {412, 78.7}}
                                                Other(1) id=ShortLook.Platter.Content.Seamless.CrossFade {{14, 719.8}, {412, 78.7}}
                                                  StaticText(48) label="Ready for Apple Intelligence" id=NotificationTitle {{78, 731.2}, {267.7, 18}}
                                                  Other(1) id=NotificationBody.TopAligned.originalMessage {{78, 749.2}, {331, 36}}
                                                    StaticText(48) label="Time to experience the new personal intelligence system." id=TextContent.Primary {{78, 749.2}, {331, 36}}
                                              Other(1) {{14, 719.8}, {412, 78.7}}
                                                Other(1) {{14, 719.8}, {412, 78.7}}
                                                  Other(1) {{14, 719.8}, {412, 78.7}}
                                                    Other(1) {{14, 719.8}, {412, 78.7}}
                                                      Other(1) {{14, 719.8}, {412, 78.7}}
                                                        Other(1) {{14, 719.8}, {412, 78.7}}
                                                    Other(1) {{14, 719.8}, {412, 78.7}}
                                                      Other(1) {{14, 719.8}, {412, 78.7}}
                                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 719.8}, {30, 78.7}}
                                  Other(1) {{420, 680.8}, {3, 36}}
                                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 719.8}, {30, 78.7}}
                                  Other(1) {{420, 680.8}, {3, 36}}
                              Other(1) {{14, 806.7}, {412, 1}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 663.8}, {30, 125.2}}
                                Other(1) {{420, 633.8}, {3, 36}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{393, 663.8}, {30, 125.2}}
                                Other(1) {{420, 633.8}, {3, 36}}
                          Other(1) {{0, 0}, {440, 956}}
                        Button(9) label="1 Notification" {{112, 860}, {216, 18}}
                          Other(1) {{206.5, 866.8}, {26.9, 4.5}}
                            Other(1) id=focus-container-1 Notification {{206.5, 866.8}, {26.9, 4.5}}
                              StaticText(48) label=" 1 Notification" id=focus-text-1 Notification {{206.5, 866.8}, {26.9, 4.5}}
                              Button(9) {{206.5, 866.8}, {26.9, 4.5}}
                  Other(1) label="Vertical scroll bar, 1 page" value="0%" {{407, 55.7}, {30, 844.3}}
                    Other(1) {{434, 58.7}, {3, 838.3}}
                  Other(1) label="Vertical scroll bar, 1 page" value="0%" {{407, 55.7}, {30, 844.3}}
                    Other(1) {{434, 58.7}, {3, 838.3}}
                Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 1912}}
                  Other(1) id=lockscreen-date-view {{0, 0}, {440, 956}}
                    StaticText(48) label="Wednesday, November 25" {{45.0, 72}, {350.3, 36.3}}
                    StaticText(48) label="1:23 PM" {{78.7, 86.7}, {280.7, 155.3}}
                    Other(1) {{35, 218}, {370, 76}}
                      Other(1) {{35, 218}, {370, 76}}
                        Other(1) {{35, 218}, {370, 76}}
                          Other(1) {{15, 198}, {410, 116}}
                            Other(1) {{15, 198}, {410, 116}}
              Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{329.3, 47.3}, {53.3, 2.3}}
                      Other(1) {{329.3, 47.3}, {53.3, 2.3}}
                    Other(1) {{329.3, 75.8}, {53.3, 2.3}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) label="Home indicator" {{141.7, 943}, {157.0, 5}} [disabled]
                          Other(1) {{141.7, 943}, {157.0, 5}}
                            Other(1) {{141.7, 943}, {157.0, 5}}
                              Other(1) {{141.7, 943}, {157.0, 5}}
                                Other(1) {{141.7, 943}, {157.0, 5}}
                  Other(1) {{160, 894.7}, {120, 17.3}}
              Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
      Other(1) {{0, 0}, {440, 54}}
        Other(1) {{0, 0}, {440, 54}}
          Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
          Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
          Other(1) label="100% battery power" value="Not charging" {{368.3, 25.0}, {31.3, 14.3}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Control Centre pulled down over Settings (run 139, probe round 3 p12a capture 06 (control-centre); 33 nodes).
    static let controlCentre = Capture(
        name: "controlCentre", kind: .controlCentre,
        source: "run 139, probe round 3 p12a capture 06 (control-centre)",
        showing: "Control Centre pulled down over Settings",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        StatusBar(25) {{0, 0}, {440, 50}}
        Other(1) {{0, 0}, {440, 141.7}}
          Button(9) label="Add Controls" {{39, 23}, {29, 29}}
          Button(9) label="Power" {{372, 23}, {29, 29}}
          Button(9) label="Services in use" value="" {{210, 50}, {20, 22.7}} [disabled]
          Other(1) {{0, 91.7}, {440, 50}}
            StatusBar(25) {{52.4, 61.3}, {379.8, 53.5}}
            StatusBar(25) {{46, 91.7}, {348, 50}}
              Other(1) {{46, 91.7}, {348, 50}}
                Other(1) {{46, 91.7}, {348, 50}}
                  Other(1) {{52, 89.7}, {166.7, 12}}
                    Other(1) label="Cellular" value="No signal" {{52, 88.3}, {23, 14.7}}
                    StaticText(48) label="Carrier" {{79, 85.3}, {54, 20.3}}
                    StaticText(48) label="4G" {{137, 85.3}, {23.3, 20.3}}
                  Other(1) {{222.7, 89.7}, {166.3, 12}}
                    StaticText(48) label="100%" {{307, 85.3}, {47, 20.3}}
                    Other(1) label="100% battery power" value="Not charging" {{356, 88.0}, {33, 15.3}}
                  Other(1) {{52, 114.7}, {166.7, 12}}
                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{52, 114.0}, {19, 13.7}}
                  Other(1) {{222.7, 114.7}, {166.3, 12}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{0, 0}, {440, 956}}
              Other(1) id=cc-root-folder-view {{0, 0}, {440, 956}}
                Icon(44) {{0, 0}, {0, 0}}
                PageIndicator(34) value="Page 1 of 1" id=Page control {{394, 455.7}, {46, 49.7}}
"""#
    )

    /// the App Switcher over Settings (five cards) (run 139, probe round 3 p12a capture 09 (app-switcher); 123 nodes).
    static let appSwitcher = Capture(
        name: "appSwitcher", kind: .appSwitcher,
        source: "run 139, probe round 3 p12a capture 09 (app-switcher)",
        showing: "the App Switcher over Settings (five cards)",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{23.6, 144.5}, {298.1, 647.7}}
                Other(1) {{23.6, 144.5}, {298.1, 647.7}}
                  Other(1) {{23.6, 144.5}, {298.1, 647.7}}
                    Other(1) {{23.6, 101.8}, {298.1, 42.9}}
                      Other(1) {{46.8, 109.9}, {27.1, 27.1}}
                        Image(43) {{46.8, 109.9}, {27.1, 27.1}}
                        Other(1) {{46.8, 109.9}, {27.1, 27.1}}
              Other(1) {{102.3, 139.6}, {302.6, 657.6}}
                Other(1) {{102.3, 139.6}, {302.6, 657.6}}
                  Other(1) {{102.3, 139.6}, {302.6, 657.6}}
                    Other(1) {{102.3, 96.2}, {302.6, 43.6}}
                      Other(1) {{125.9, 104.4}, {27.5, 27.5}}
                        Image(43) {{125.9, 104.4}, {27.5, 27.5}}
                        Other(1) {{125.9, 104.4}, {27.5, 27.5}}
                      StaticText(48) label="Jarvis" {{161.3, 108.0}, {47.2, 20.0}}
              Other(1) {{-1.1, 149.4}, {293.6, 637.8}}
                Other(1) {{-1.1, 149.4}, {293.6, 637.8}}
                  Other(1) {{-1.1, 149.4}, {293.6, 637.8}}
                    Other(1) {{-1.1, 107.4}, {293.6, 42.3}}
                      Other(1) {{21.8, 115.3}, {26.7, 26.7}}
                        Image(43) {{21.8, 115.3}, {26.7, 26.7}}
                        Other(1) {{21.8, 115.3}, {26.7, 26.7}}
              Other(1) {{-7.5, 154.4}, {289.0, 627.9}}
                Other(1) {{-7.5, 154.4}, {289.0, 627.9}}
                  Other(1) {{-7.5, 154.4}, {289.0, 627.9}}
              Other(1) {{337.1, 134.6}, {307.2, 667.4}}
                Other(1) {{337.1, 134.6}, {307.2, 667.4}}
                  Other(1) {{337.1, 134.6}, {307.2, 667.4}}
                    Other(1) {{337.1, 90.6}, {307.2, 44.2}}
                      Other(1) {{361.0, 98.9}, {27.9, 27.9}}
                        Image(43) {{361.0, 98.9}, {27.9, 27.9}}
                        Other(1) {{361.0, 98.9}, {27.9, 27.9}}
              Other(1) label="Messages" id=card:com.apple.MobileSMS:sceneID:com.apple.MobileSMS-E0CAEA3E-6418-47BF-8887-F7AA36843BF0 {{-7.5, 154.4}, {289.0, 627.9}}
              Other(1) label="Contacts" id=card:com.apple.MobileAddressBook:sceneID:com.apple.MobileAddressBook-CCA0AD14-4D6C-422C-8069-A57CCE4D069A {{-1.1, 149.4}, {293.6, 637.8}}
              Other(1) label="Safari" id=card:com.apple.mobilesafari:sceneID:com.apple.mobilesafari-7EB40446-F62C-40B4-9053-BF3F6952C4EC {{23.6, 144.5}, {298.1, 647.7}}
              Other(1) label="Jarvis" value="Active" id=card:dev.jarvis.app:sceneID:dev.jarvis.app-default {{102.3, 139.6}, {302.6, 657.6}}
              Other(1) label="Settings" value="Active" id=card:com.apple.Preferences:sceneID:com.apple.Preferences-default {{337.1, 134.6}, {307.2, 667.4}}
            Other(1) id=handoff-banner {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// the Home Screen, Jarvis with a badge (run 139, probe round 3 p12a capture 01 (home-badge); 212 nodes).
    static let homeBadge = Capture(
        name: "homeBadge", kind: .home,
        source: "run 139, probe round 3 p12a capture 01 (home-badge)",
        showing: "the Home Screen, Jarvis with a badge",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 100}}
      Other(1) id=Home screen icons {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{440, 0}, {440, 956}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Maps" id=Maps {{0, 0}, {0, 0}}
              Icon(44) label="Calendar" id=Calendar {{0, 0}, {0, 0}}
              Icon(44) label="Calendar" value="Wednesday, November 25" id=Calendar {{0, 0}, {0, 0}}
              Icon(44) label="Photos" id=Photos {{0, 0}, {0, 0}}
              Icon(44) label="Reminders" id=Reminders {{0, 0}, {0, 0}}
              Icon(44) label="News" id=News {{0, 0}, {0, 0}}
              Icon(44) label="Maps" id=Maps {{0, 0}, {0, 0}}
              Icon(44) label="Health" id=Health {{0, 0}, {0, 0}}
              Icon(44) label="Wallet" id=Wallet {{0, 0}, {0, 0}}
              Icon(44) label="Settings" id=Settings {{0, 0}, {0, 0}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Fitness" value="" id=Fitness {{33, 92}, {72, 94.7}}
                Other(1) {{35, 94}, {68, 82}}
                  Other(1) {{35, 94}, {68, 68}}
                    Other(1) {{35, 94}, {68, 68}}
                      Other(1) {{35, 94}, {68, 68}}
                  Other(1) id=label-view {{40, 165}, {57.7, 19.7}}
                    Other(1) {{40, 165}, {57.7, 19.7}}
                      Image(43) {{23.4, 146.4}, {91.7, 55.7}}
                      Image(43) {{40, 165}, {57.7, 19.7}}
              Icon(44) label="Watch" value="" id=Watch {{133.7, 92}, {72, 94.7}}
                Other(1) {{135.7, 94}, {68, 82}}
                  Other(1) {{135.7, 94}, {68, 68}}
                    Other(1) {{135.7, 94}, {68, 68}}
                      Other(1) {{135.7, 94}, {68, 68}}
                  Other(1) id=label-view {{143, 165}, {52.7, 19.7}}
                    Other(1) {{143, 165}, {52.7, 19.7}}
                      Image(43) {{126.4, 146.4}, {86.7, 55.7}}
                      Image(43) {{143, 165}, {52.7, 19.7}}
              Icon(44) label="Contacts" value="" id=Contacts {{234, 92}, {72.3, 94.7}}
                Other(1) {{236.3, 94}, {68, 82}}
                  Other(1) {{236.3, 94}, {68, 68}}
                    Other(1) {{236.3, 94}, {68, 68}}
                      Other(1) {{236.3, 94}, {68, 68}}
                  Other(1) id=label-view {{236, 165}, {68.3, 19.7}}
                    Other(1) {{236, 165}, {68.3, 19.7}}
                      Image(43) {{219.4, 146.4}, {102.3, 55.7}}
                      Image(43) {{236, 165}, {68.3, 19.7}}
              Icon(44) label="Files" value="" id=Files {{335, 92}, {72, 94.7}}
                Other(1) {{337, 94}, {68, 82}}
                  Other(1) {{337, 94}, {68, 68}}
                    Other(1) {{337, 94}, {68, 68}}
                      Other(1) {{337, 94}, {68, 68}}
                  Other(1) id=label-view {{349.3, 165}, {42.7, 19.7}}
                    Other(1) {{349.3, 165}, {42.7, 19.7}}
                      Image(43) {{332.7, 146.4}, {76.7, 55.7}}
                      Image(43) {{349.3, 165}, {42.7, 19.7}}
              Icon(44) label="Preview" value="" id=Preview {{33, 200.7}, {72, 94.7}}
                Other(1) {{35, 202.7}, {68, 82.0}}
                  Other(1) {{35, 202.7}, {68, 68.0}}
                    Other(1) {{35, 202.7}, {68, 68.0}}
                      Other(1) {{35, 202.7}, {68, 68.0}}
                  Other(1) id=label-view {{38, 273.7}, {61.7, 19.7}}
                    Other(1) {{38, 273.7}, {61.7, 19.7}}
                      Image(43) {{21.4, 255.1}, {95.7, 55.7}}
                      Image(43) {{38, 273.7}, {61.7, 19.7}}
              Icon(44) label="Utilities folder" value="2 apps" id=Utilities {{133.7, 200.7}, {72, 94.7}}
                Icon(44) label="Shortcuts" id=Shortcuts {{0, 0}, {0, 0}}
                Icon(44) label="Passwords" id=Passwords {{0, 0}, {0, 0}}
              Icon(44) label="JarvisUITests-Runner" value="" id=JarvisUITests-Runner {{216, 200.7}, {109, 98.0}}
                Other(1) {{236.3, 202.7}, {68, 82.0}}
                  Other(1) {{236.3, 202.7}, {68, 68.0}}
                    Other(1) {{236.3, 202.7}, {68, 68.0}}
                      Other(1) {{236.3, 202.7}, {68, 68.0}}
                  Other(1) id=label-view {{218, 270.7}, {105, 26}}
                    Other(1) {{218, 270.7}, {105, 26}}
                      Image(43) {{201.1, 251.9}, {139, 62}}
                      Image(43) {{218, 270.7}, {105, 26}}
              Icon(44) label="Jarvis" value="7 new items" id=Jarvis {{335, 189.7}, {83, 105.7}}
                Other(1) {{337, 202.7}, {68, 82.0}}
                  Other(1) {{337, 202.7}, {68, 68.0}}
                    Other(1) {{390, 191.7}, {26, 26}}
                      Other(1) {{390, 191.7}, {26, 26}}
                        Image(43) {{398, 195}, {10, 19}}
                    Other(1) {{337, 202.7}, {68, 68.0}}
                      Other(1) {{337, 202.7}, {68, 68.0}}
                  Other(1) id=label-view {{345.3, 273.7}, {50.7, 19.7}}
                    Other(1) {{345.3, 273.7}, {50.7, 19.7}}
                      Image(43) {{328.7, 255.1}, {84.7, 55.7}}
                      Image(43) {{345.3, 273.7}, {50.7, 19.7}}
            Other(1) label="Search" value="Page 2 of 2" id=spotlight-pill {{181, 763.7}, {78, 30}}
              PageIndicator(34) value="Page 2 of 2" id=Page control {{181, 763.7}, {78, 35}}
              Other(1) id=spotlight-pill {{181, 763.7}, {78, 30}}
                Other(1) {{181, 763.7}, {78, 30}}
                  Image(43) label="Search" id=magnifyingglass {{192.7, 773}, {11.3, 10.7}}
                  StaticText(48) label="Search" {{207, 771.3}, {40.3, 14.3}}
            Other(1) {{0, 821.7}, {440, 134.3}}
              Other(1) {{17, 821.7}, {406, 117.3}}
                Other(1) label="Dock" {{0, 821.7}, {440, 117.3}}
                  Other(1) {{0, 821.7}, {440, 117.3}}
                  Icon(44) label="Safari" value="" id=Safari {{136, 844.3}, {72, 72}}
                    Other(1) {{138, 846.3}, {68, 68}}
                      Other(1) {{138, 846.3}, {68, 68}}
                        Other(1) {{138, 846.3}, {68, 68}}
                          Other(1) {{138, 846.3}, {68, 68}}
                  Icon(44) label="Messages" value="" id=Messages {{232, 844.3}, {72, 72}}
                    Other(1) {{234, 846.3}, {68, 68}}
                      Other(1) {{234, 846.3}, {68, 68}}
                        Other(1) {{234, 846.3}, {68, 68}}
                          Other(1) {{234, 846.3}, {68, 68}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
      Other(1) {{0, 0}, {440, 54}}
        Other(1) {{0, 0}, {440, 54}}
          StaticText(48) label="1:22 PM" {{65.7, 22}, {35.7, 21.7}}
          Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
          Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
          Other(1) label="100% battery power" value="Not charging" {{368.3, 25.0}, {31.3, 14.3}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Jarvis in front, PHPickerViewController up (run 139, probe round 3 p2 capture 02 (photo-picker-springboard); 120 nodes).
    static let photoPicker = Capture(
        name: "photoPicker", kind: .app,
        source: "run 139, probe round 3 p2 capture 02 (photo-picker-springboard)",
        showing: "Jarvis in front, PHPickerViewController up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Jarvis" id=card:dev.jarvis.app:sceneID:dev.jarvis.app-default {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    StaticText(48) label="1:24 PM" {{65.3, 22}, {36.3, 21.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Jarvis in front, CNContactPickerViewController up (run 139, probe round 3 p2 capture 06 (contact-picker-springboard); 120 nodes).
    static let contactPicker = Capture(
        name: "contactPicker", kind: .app,
        source: "run 139, probe round 3 p2 capture 06 (contact-picker-springboard)",
        showing: "Jarvis in front, CNContactPickerViewController up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Jarvis" id=card:dev.jarvis.app:sceneID:dev.jarvis.app-default {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    StaticText(48) label="1:25 PM" {{65.7, 22}, {36, 21.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Jarvis in front, UIDocumentPickerViewController up (run 139, probe round 3 p2 capture 10 (document-picker-springboard); 120 nodes).
    static let documentPicker = Capture(
        name: "documentPicker", kind: .app,
        source: "run 139, probe round 3 p2 capture 10 (document-picker-springboard)",
        showing: "Jarvis in front, UIDocumentPickerViewController up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Jarvis" id=card:dev.jarvis.app:sceneID:dev.jarvis.app-default {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    StaticText(48) label="1:25 PM" {{65.7, 22}, {36, 21.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Jarvis in front, an action sheet up (run 139, probe round 3 p4c capture 03 (action-sheet-springboard); 120 nodes).
    static let actionSheet = Capture(
        name: "actionSheet", kind: .app,
        source: "run 139, probe round 3 p4c capture 03 (action-sheet-springboard)",
        showing: "Jarvis in front, an action sheet up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Jarvis" id=card:dev.jarvis.app:sceneID:dev.jarvis.app-default {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    StaticText(48) label="1:29 PM" {{65.3, 22}, {36.3, 21.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Jarvis in front, its own alert with a text field up (the app's, not SpringBoard's) (run 139, probe round 3 p4c capture 06 (alert-field-springboard); 128 nodes).
    static let alertWithField = Capture(
        name: "alertWithField", kind: .app,
        source: "run 139, probe round 3 p4c capture 06 (alert-field-springboard)",
        showing: "Jarvis in front, its own alert with a text field up (the app's, not SpringBoard's)",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Jarvis" id=card:dev.jarvis.app:sceneID:dev.jarvis.app-default {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    StaticText(48) label="1:29 PM" {{65.3, 22}, {36.3, 21.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 611}, {440, 345}}
        Other(1) id=inputView {{0, 611}, {440, 345}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 611}, {440, 345}}
        Other(1) id=inputView {{0, 611}, {440, 345}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 611}, {440, 345}}
        Other(1) id=inputView {{0, 611}, {440, 345}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 611}, {440, 345}}
        Other(1) id=inputView {{0, 611}, {440, 345}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Safari in front, a JavaScript alert up (run 139, probe round 3 p6a capture 05 (js-alert-springboard); 122 nodes).
    static let safariJsAlert = Capture(
        name: "safariJsAlert", kind: .app,
        source: "run 139, probe round 3 p6a capture 05 (js-alert-springboard)",
        showing: "Safari in front, a JavaScript alert up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Safari" id=card:com.apple.mobilesafari:sceneID:com.apple.mobilesafari-7EB40446-F62C-40B4-9053-BF3F6952C4EC {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    Other(1) {{65.3, 14.7}, {36.7, 36.7}}
                                                    StaticText(48) label="1:19 PM" {{66.7, 22}, {34, 21.7}}
                                                    Other(1) {{337.7, 14.7}, {36.7, 36.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Safari in front, a JavaScript confirm up (run 139, probe round 3 p6a capture 08 (js-confirm-springboard); 122 nodes).
    static let safariJsConfirm = Capture(
        name: "safariJsConfirm", kind: .app,
        source: "run 139, probe round 3 p6a capture 08 (js-confirm-springboard)",
        showing: "Safari in front, a JavaScript confirm up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Safari" id=card:com.apple.mobilesafari:sceneID:com.apple.mobilesafari-7EB40446-F62C-40B4-9053-BF3F6952C4EC {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    Other(1) {{65.3, 14.7}, {36.7, 36.7}}
                                                    StaticText(48) label="1:19 PM" {{66.7, 22}, {34, 21.7}}
                                                    Other(1) {{337.7, 14.7}, {36.7, 36.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Safari in front, a JavaScript prompt up (run 139, probe round 3 p6a capture 11 (js-prompt-springboard); 122 nodes).
    static let safariJsPrompt = Capture(
        name: "safariJsPrompt", kind: .app,
        source: "run 139, probe round 3 p6a capture 11 (js-prompt-springboard)",
        showing: "Safari in front, a JavaScript prompt up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Safari" id=card:com.apple.mobilesafari:sceneID:com.apple.mobilesafari-7EB40446-F62C-40B4-9053-BF3F6952C4EC {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    Other(1) {{65.3, 14.7}, {36.7, 36.7}}
                                                    StaticText(48) label="1:19 PM" {{66.7, 22}, {34, 21.7}}
                                                    Other(1) {{337.7, 14.7}, {36.7, 36.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Safari in front, a select menu up (run 139, probe round 3 p6b capture 02 (select-springboard); 122 nodes).
    static let safariSelect = Capture(
        name: "safariSelect", kind: .app,
        source: "run 139, probe round 3 p6b capture 02 (select-springboard)",
        showing: "Safari in front, a select menu up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Safari" id=card:com.apple.mobilesafari:sceneID:com.apple.mobilesafari-7EB40446-F62C-40B4-9053-BF3F6952C4EC {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    Other(1) {{65.3, 14.7}, {36.7, 36.7}}
                                                    StaticText(48) label="1:20 PM" {{65.3, 22}, {36.3, 21.7}}
                                                    Other(1) {{337.7, 14.7}, {36.7, 36.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Safari in front, a date picker up (run 139, probe round 3 p6b capture 05 (date-input-springboard); 122 nodes).
    static let safariDate = Capture(
        name: "safariDate", kind: .app,
        source: "run 139, probe round 3 p6b capture 05 (date-input-springboard)",
        showing: "Safari in front, a date picker up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Safari" id=card:com.apple.mobilesafari:sceneID:com.apple.mobilesafari-7EB40446-F62C-40B4-9053-BF3F6952C4EC {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    Other(1) {{65.3, 14.7}, {36.7, 36.7}}
                                                    StaticText(48) label="1:20 PM" {{65.3, 22}, {36.3, 21.7}}
                                                    Other(1) {{337.7, 14.7}, {36.7, 36.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Safari in front, a file input menu up (run 139, probe round 3 p6b capture 08 (file-input-springboard); 122 nodes).
    static let safariFile = Capture(
        name: "safariFile", kind: .app,
        source: "run 139, probe round 3 p6b capture 08 (file-input-springboard)",
        showing: "Safari in front, a file input menu up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Safari" id=card:com.apple.mobilesafari:sceneID:com.apple.mobilesafari-7EB40446-F62C-40B4-9053-BF3F6952C4EC {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    Other(1) {{65.3, 14.7}, {36.7, 36.7}}
                                                    StaticText(48) label="1:20 PM" {{65.3, 22}, {36.3, 21.7}}
                                                    Other(1) {{337.7, 14.7}, {36.7, 36.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Safari in front, an HTTP Basic auth prompt up (run 139, probe round 3 p6b capture 14 (basic-auth-springboard); 122 nodes).
    static let safariBasicAuth = Capture(
        name: "safariBasicAuth", kind: .app,
        source: "run 139, probe round 3 p6b capture 14 (basic-auth-springboard)",
        showing: "Safari in front, an HTTP Basic auth prompt up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Safari" id=card:com.apple.mobilesafari:sceneID:com.apple.mobilesafari-7EB40446-F62C-40B4-9053-BF3F6952C4EC {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    Other(1) {{65.3, 14.7}, {36.7, 36.7}}
                                                    StaticText(48) label="1:21 PM" {{67, 22}, {33.3, 21.7}}
                                                    Other(1) {{337.7, 14.7}, {36.7, 36.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Jarvis in front after asking for local network (no alert in the tree) (run 139, probe round 3 p9 capture 18 (localnet-springboard); 121 nodes).
    static let localNetwork = Capture(
        name: "localNetwork", kind: .app,
        source: "run 139, probe round 3 p9 capture 18 (localnet-springboard)",
        showing: "Jarvis in front after asking for local network (no alert in the tree)",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158.3, 15.0}, {123.3, 34.7}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
          Other(1) {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{158, 14.7}, {124, 35.3}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) id=jindo-container-view:1 {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
        Other(1) {{237, 28}, {9, 9}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
      Other(1) {{0, 0}, {1320, 2868}}
        Other(1) {{0, 0}, {1320, 2868}}
          Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
            Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) id=SBSwitcherWindow:Main {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            ScrollView(46) {{0, 0}, {440, 956}}
            Other(1) id=AppSwitcherContentView {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
              Other(1) label="Jarvis" id=card:dev.jarvis.app:sceneID:dev.jarvis.app-default {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                        Other(1) {{0, 0}, {440, 956}}
                          ScrollView(46) {{0, -478}, {440, 1434}}
                            Other(1) {{0, 0}, {440, 956}}
                              Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                  Other(1) {{0, 0}, {440, 956}}
                                    Other(1) {{0, 0}, {440, 956}}
                                      Other(1) {{0, 0}, {440, 956}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                              StatusBar(25) {{0, 0}, {440, 54}}
                                                Other(1) {{0, 0}, {440, 54}}
                                                  Other(1) {{0, 0}, {440, 54}}
                                                    StaticText(48) label="1:33 PM" {{65.3, 22}, {36.7, 21.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                        Other(1) {{0, 0}, {440, 956}}
                                          Other(1) {{0, 0}, {440, 956}}
                                            Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
                                Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Jarvis in front, the location permission alert up (run 139, probe round 3 p9 capture 01 (location-springboard); 43 nodes).
    static let alertLocation = Capture(
        name: "alertLocation", kind: .alert,
        source: "run 139, probe round 3 p9 capture 01 (location-springboard)",
        showing: "Jarvis in front, the location permission alert up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Alert(7) label="Allow “Jarvis” to use your location?" {{60, 329}, {320, 326}}
        Other(1) {{60, 329}, {320, 326}}
          Other(1) {{60, 329}, {320, 326}}
            Other(1) {{60, 329}, {320, 326}}
            Other(1) {{60, 329}, {320, 326}}
              ScrollView(46) {{60, 329}, {320, 134}}
                Other(1) {{60, 329}, {320, 134}}
                  StaticText(48) label="Allow “Jarvis” to use your location?" {{90, 351}, {260, 42.3}}
                  StaticText(48) label="Only the QA Playground asks for this, to record the system prompt in simulator tests." {{90, 400.7}, {260, 58}}
                Other(1) {{60, 461}, {320, 2}}
                Other(1) label="Vertical scroll bar, 2 pages" value="0%" {{347, 329}, {30, 134}}
                  Other(1) {{374, 290}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 430}, {320, 30}}
                  Other(1) {{21, 457}, {36, 3}}
                Other(1) label="Vertical scroll bar, 2 pages" value="0%" {{347, 329}, {30, 134}}
                  Other(1) {{374, 290}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 430}, {320, 30}}
                  Other(1) {{21, 457}, {36, 3}}
              ScrollView(46) {{60, 463}, {320, 192}}
                Other(1) {{60, 463}, {320, 192}}
                  Other(1) {{76, 479}, {288, 160}}
                    Button(9) label="Allow Once" {{76, 479}, {288, 48}}
                    Button(9) label="Allow While Using App" {{76, 535}, {288, 48}}
                    Button(9) label="Don’t Allow" {{76, 591}, {288, 48}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 463}, {30, 192}}
                  Other(1) {{374, 553}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 622}, {320, 30}}
                  Other(1) {{150, 649}, {7, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 463}, {30, 192}}
                  Other(1) {{374, 553}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 622}, {320, 30}}
                  Other(1) {{150, 649}, {7, 3}}
        Other(1) {{60, 329}, {320, 326}}
          Other(1) {{60, 329}, {320, 326}}
            Other(1) {{60, 329}, {320, 326}}
              Other(1) {{60, 329}, {320, 326}}
                Other(1) {{60, 329}, {320, 326}}
                  Other(1) {{60, 329}, {440, 956}}
              Other(1) {{60, 329}, {320, 326}}
                Other(1) {{60, 329}, {440, 956}}
"""#
    )

    /// Jarvis in front, the camera permission alert up (run 139, probe round 3 p9 capture 04 (camera-springboard); 44 nodes).
    static let alertCamera = Capture(
        name: "alertCamera", kind: .alert,
        source: "run 139, probe round 3 p9 capture 04 (camera-springboard)",
        showing: "Jarvis in front, the camera permission alert up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Alert(7) label="“Jarvis” would like to access the Camera." {{60, 340.3}, {320, 303.3}}
        Other(1) {{60, 340.3}, {320, 303.3}}
          Other(1) {{60, 340.3}, {320, 303.3}}
            Other(1) {{60, 340.3}, {320, 303.3}}
            Other(1) {{60, 340.3}, {320, 303.3}}
              ScrollView(46) {{60, 340.3}, {320, 223.3}}
                Other(1) {{60, 340.3}, {320, 223.3}}
                  Other(1) {{86, 340.3}, {268, 89}}
                    Other(1) {{86, 340.3}, {268, 89}}
                      Image(43) id=/Users/runner/Library/Developer/CoreSimulator/Devices/F90353E4-F95D-488C-9F4E-B43F74E385F2/data/tmp/com.apple.tccd/kTCCServiceCamera@3x.png {{86, 360.3}, {69, 69}}
                  StaticText(48) label="“Jarvis” would like to access the Camera." {{90, 451.3}, {260, 42.3}}
                  StaticText(48) label="Only the QA Playground asks for this, to record the system prompt in simulator tests." {{90, 501.3}, {260, 58}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 340.3}, {30, 223.3}}
                  Other(1) {{374, 301.3}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 530.7}, {320, 30}}
                  Other(1) {{21, 557.7}, {36, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 340.3}, {30, 223.3}}
                  Other(1) {{374, 301.3}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 530.7}, {320, 30}}
                  Other(1) {{21, 557.7}, {36, 3}}
              ScrollView(46) {{60, 563.7}, {320, 80}}
                Other(1) {{60, 563.7}, {320, 80}}
                  Other(1) {{76, 579.7}, {288, 48}}
                    Button(9) label="Don’t Allow" {{76, 579.7}, {140, 48}}
                    Button(9) label="Allow" {{224, 579.7}, {140, 48}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 563.7}, {30, 80}}
                  Other(1) {{374, 653.7}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 610.7}, {320, 30}}
                  Other(1) {{150, 637.7}, {7, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 563.7}, {30, 80}}
                  Other(1) {{374, 653.7}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 610.7}, {320, 30}}
                  Other(1) {{150, 637.7}, {7, 3}}
        Other(1) {{60, 340.3}, {320, 303.3}}
          Other(1) {{60, 340.3}, {320, 303.3}}
            Other(1) {{60, 340.3}, {320, 303.3}}
              Other(1) {{60, 340.3}, {320, 303.3}}
                Other(1) {{60, 340.3}, {320, 303.3}}
                  Other(1) {{60, 340.3}, {440, 956.0}}
              Other(1) {{60, 340.3}, {320, 303.3}}
                Other(1) {{60, 340.3}, {440, 956.0}}
"""#
    )

    /// Jarvis in front, the microphone permission alert up (run 139, probe round 3 p9 capture 07 (microphone-springboard); 44 nodes).
    static let alertMicrophone = Capture(
        name: "alertMicrophone", kind: .alert,
        source: "run 139, probe round 3 p9 capture 07 (microphone-springboard)",
        showing: "Jarvis in front, the microphone permission alert up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Alert(7) label="“Jarvis” would like to access the Microphone." {{60, 340.3}, {320, 303.3}}
        Other(1) {{60, 340.3}, {320, 303.3}}
          Other(1) {{60, 340.3}, {320, 303.3}}
            Other(1) {{60, 340.3}, {320, 303.3}}
            Other(1) {{60, 340.3}, {320, 303.3}}
              ScrollView(46) {{60, 340.3}, {320, 223.3}}
                Other(1) {{60, 340.3}, {320, 223.3}}
                  Other(1) {{86, 340.3}, {268, 89}}
                    Other(1) {{86, 340.3}, {268, 89}}
                      Image(43) id=/Users/runner/Library/Developer/CoreSimulator/Devices/F90353E4-F95D-488C-9F4E-B43F74E385F2/data/tmp/com.apple.tccd/kTCCServiceMicrophone@3x.png {{86, 360.3}, {69, 69}}
                  StaticText(48) label="“Jarvis” would like to access the Microphone." {{90, 451.3}, {260, 42.3}}
                  StaticText(48) label="Only the QA Playground asks for this, to record the system prompt in simulator tests." {{90, 501.3}, {260, 58}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 340.3}, {30, 223.3}}
                  Other(1) {{374, 301.3}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 530.7}, {320, 30}}
                  Other(1) {{21, 557.7}, {36, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 340.3}, {30, 223.3}}
                  Other(1) {{374, 301.3}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 530.7}, {320, 30}}
                  Other(1) {{21, 557.7}, {36, 3}}
              ScrollView(46) {{60, 563.7}, {320, 80}}
                Other(1) {{60, 563.7}, {320, 80}}
                  Other(1) {{76, 579.7}, {288, 48}}
                    Button(9) label="Don’t Allow" {{76, 579.7}, {140, 48}}
                    Button(9) label="Allow" {{224, 579.7}, {140, 48}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 563.7}, {30, 80}}
                  Other(1) {{374, 653.7}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 610.7}, {320, 30}}
                  Other(1) {{150, 637.7}, {7, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 563.7}, {30, 80}}
                  Other(1) {{374, 653.7}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 610.7}, {320, 30}}
                  Other(1) {{150, 637.7}, {7, 3}}
        Other(1) {{60, 340.3}, {320, 303.3}}
          Other(1) {{60, 340.3}, {320, 303.3}}
            Other(1) {{60, 340.3}, {320, 303.3}}
              Other(1) {{60, 340.3}, {320, 303.3}}
                Other(1) {{60, 340.3}, {320, 303.3}}
                  Other(1) {{60, 340.3}, {440, 956.0}}
              Other(1) {{60, 340.3}, {320, 303.3}}
                Other(1) {{60, 340.3}, {440, 956.0}}
"""#
    )

    /// Jarvis in front, the contacts permission alert up (run 139, probe round 3 p9 capture 10 (contacts-springboard); 44 nodes).
    static let alertContacts = Capture(
        name: "alertContacts", kind: .alert,
        source: "run 139, probe round 3 p9 capture 10 (contacts-springboard)",
        showing: "Jarvis in front, the contacts permission alert up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Alert(7) label="“Jarvis” would like to access your Contacts." {{60, 340.3}, {320, 303.3}}
        Other(1) {{60, 340.3}, {320, 303.3}}
          Other(1) {{60, 340.3}, {320, 303.3}}
            Other(1) {{60, 340.3}, {320, 303.3}}
            Other(1) {{60, 340.3}, {320, 303.3}}
              ScrollView(46) {{60, 340.3}, {320, 223.3}}
                Other(1) {{60, 340.3}, {320, 223.3}}
                  Other(1) {{86, 340.3}, {268, 89}}
                    Other(1) {{86, 340.3}, {268, 89}}
                      Image(43) id=/Users/runner/Library/Developer/CoreSimulator/Devices/F90353E4-F95D-488C-9F4E-B43F74E385F2/data/tmp/com.apple.tccd/kTCCServiceAddressBook@3x.png {{86, 360.3}, {69, 69}}
                  StaticText(48) label="“Jarvis” would like to access your Contacts." {{90, 451.3}, {260, 42.3}}
                  StaticText(48) label="Only the QA Playground asks for this, to record the system prompt in simulator tests." {{90, 501.3}, {260, 58}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 340.3}, {30, 223.3}}
                  Other(1) {{374, 301.3}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 530.7}, {320, 30}}
                  Other(1) {{21, 557.7}, {36, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 340.3}, {30, 223.3}}
                  Other(1) {{374, 301.3}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 530.7}, {320, 30}}
                  Other(1) {{21, 557.7}, {36, 3}}
              ScrollView(46) {{60, 563.7}, {320, 80}}
                Other(1) {{60, 563.7}, {320, 80}}
                  Other(1) {{76, 579.7}, {288, 48}}
                    Button(9) label="Don’t Allow" {{76, 579.7}, {140, 48}}
                    Button(9) label="Continue" {{224, 579.7}, {140, 48}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 563.7}, {30, 80}}
                  Other(1) {{374, 653.7}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 610.7}, {320, 30}}
                  Other(1) {{150, 637.7}, {7, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 563.7}, {30, 80}}
                  Other(1) {{374, 653.7}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 610.7}, {320, 30}}
                  Other(1) {{150, 637.7}, {7, 3}}
        Other(1) {{60, 340.3}, {320, 303.3}}
          Other(1) {{60, 340.3}, {320, 303.3}}
            Other(1) {{60, 340.3}, {320, 303.3}}
              Other(1) {{60, 340.3}, {320, 303.3}}
                Other(1) {{60, 340.3}, {320, 303.3}}
                  Other(1) {{60, 340.3}, {440, 956.0}}
              Other(1) {{60, 340.3}, {320, 303.3}}
                Other(1) {{60, 340.3}, {440, 956.0}}
"""#
    )

    /// Jarvis in front, the photo library permission alert up (run 139, probe round 3 p9 capture 13 (photos-springboard); 62 nodes).
    static let alertPhotos = Capture(
        name: "alertPhotos", kind: .alert,
        source: "run 139, probe round 3 p9 capture 13 (photos-springboard)",
        showing: "Jarvis in front, the photo library permission alert up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Alert(7) label="“Jarvis” would like full access to your Photo Library." {{60, 190.3}, {320, 603.3}}
        Other(1) {{60, 190.3}, {320, 603.3}}
          Other(1) {{60, 190.3}, {320, 603.3}}
            Other(1) {{60, 190.3}, {320, 603.3}}
            Other(1) {{60, 190.3}, {320, 603.3}}
              ScrollView(46) {{60, 190.3}, {320, 411.3}}
                Other(1) {{60, 190.3}, {320, 411.3}}
                  Other(1) {{86, 190.3}, {268, 89}}
                    Other(1) {{86, 190.3}, {268, 89}}
                      Image(43) id=/Users/runner/Library/Developer/CoreSimulator/Devices/F90353E4-F95D-488C-9F4E-B43F74E385F2/data/tmp/com.apple.tccd/kTCCServicePhotos@3x.png {{86, 210.3}, {69, 69}}
                  StaticText(48) label="“Jarvis” would like full access to your Photo Library." {{90, 301.3}, {260, 42.3}}
                  StaticText(48) label="Only the QA Playground asks for this, to record the system prompt in simulator tests." {{90, 351}, {260, 58}}
                  Other(1) {{76, 425.3}, {288, 168.3}}
                    Other(1) {{76, 425.3}, {288, 168.3}}
                      Other(1) {{76, 425.3}, {288, 168.3}}
                        Other(1) {{76, 425.3}, {288, 168.3}}
                          Other(1) {{0, 0}, {288, 168.3}}
                            Other(1) {{0, 0}, {288, 168.3}}
                              Other(1) {{0, 0}, {288, 168.3}}
                                Other(1) {{0, 0}, {288, 168.3}}
                                  Other(1) {{0, 0}, {288, 168.3}}
                                    Other(1) {{0, 0}, {288, 168.3}}
                                      Other(1) {{0, 0}, {288, 168.3}}
                                        Other(1) {{0, 0}, {288, 168.3}}
                                          Other(1) {{0, 0}, {288, 65}}
                                          Other(1) {{0, 65}, {280, 103.3}}
                                            StaticText(48) label="6 Photos" {{8, 77}, {272, 15.7}}
                                            StaticText(48) label="Photos may contain data associated with location, depth information, captions, and audio." {{8, 100.7}, {272, 47.7}}
                Other(1) {{60, 599.7}, {320, 2}}
                Other(1) label="Vertical scroll bar, 2 pages" value="0%" {{347, 190.3}, {30, 411.3}}
                  Other(1) {{374, 151.3}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 568.7}, {320, 30}}
                  Other(1) {{21, 595.7}, {36, 3}}
                Other(1) label="Vertical scroll bar, 2 pages" value="0%" {{347, 190.3}, {30, 411.3}}
                  Other(1) {{374, 151.3}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 568.7}, {320, 30}}
                  Other(1) {{21, 595.7}, {36, 3}}
              ScrollView(46) {{60, 601.7}, {320, 192}}
                Other(1) {{60, 601.7}, {320, 192}}
                  Other(1) {{76, 617.7}, {288, 160}}
                    Button(9) label="Limit Access…" {{76, 617.7}, {288, 48}}
                    Button(9) label="Allow Full Access" {{76, 673.7}, {288, 48}}
                    Button(9) label="Don’t Allow" {{76, 729.7}, {288, 48}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 601.7}, {30, 190}}
                  Other(1) {{374, 691.7}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{62, 760.7}, {316, 30}}
                  Other(1) {{150, 787.7}, {7, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 601.7}, {30, 190}}
                  Other(1) {{374, 691.7}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{62, 760.7}, {316, 30}}
                  Other(1) {{150, 787.7}, {7, 3}}
        Other(1) {{60, 190.3}, {320, 603.3}}
          Other(1) {{60, 190.3}, {320, 603.3}}
            Other(1) {{60, 190.3}, {320, 603.3}}
              Other(1) {{60, 190.3}, {320, 603.3}}
                Other(1) {{60, 190.3}, {320, 603.3}}
                  Other(1) {{60, 190.3}, {440, 956}}
              Other(1) {{60, 190.3}, {320, 603.3}}
                Other(1) {{60, 190.3}, {440, 956}}
"""#
    )

    /// Jarvis in front, the tracking permission alert up (run 139, probe round 3 p9 capture 15 (tracking-springboard); 44 nodes).
    static let alertTracking = Capture(
        name: "alertTracking", kind: .alert,
        source: "run 139, probe round 3 p9 capture 15 (tracking-springboard)",
        showing: "Jarvis in front, the tracking permission alert up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Alert(7) label="Allow “Jarvis” to track your activity across other companies’ apps and websites?" {{60, 301.3}, {320, 381.3}}
        Other(1) {{60, 301.3}, {320, 381.3}}
          Other(1) {{60, 301.3}, {320, 381.3}}
            Other(1) {{60, 301.3}, {320, 381.3}}
            Other(1) {{60, 301.3}, {320, 381.3}}
              ScrollView(46) {{60, 301.3}, {320, 245.3}}
                Other(1) {{60, 301.3}, {320, 245.3}}
                  Other(1) {{86, 301.3}, {268, 89}}
                    Other(1) {{86, 301.3}, {268, 89}}
                      Image(43) id=/Users/runner/Library/Developer/CoreSimulator/Devices/F90353E4-F95D-488C-9F4E-B43F74E385F2/data/tmp/com.apple.tccd/kTCCServiceUserTracking@3x.png {{86, 321.3}, {69, 69}}
                  StaticText(48) label="Allow “Jarvis” to track your activity across other companies’ apps and websites?" {{90, 412.3}, {260, 64.3}}
                  StaticText(48) label="Only the QA Playground asks for this, to record the system prompt in simulator tests." {{90, 484.3}, {260, 58}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 301.3}, {30, 245.3}}
                  Other(1) {{374, 262.3}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 513.7}, {320, 30}}
                  Other(1) {{21, 540.7}, {36, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 301.3}, {30, 245.3}}
                  Other(1) {{374, 262.3}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 513.7}, {320, 30}}
                  Other(1) {{21, 540.7}, {36, 3}}
              ScrollView(46) {{60, 546.7}, {320, 136}}
                Other(1) {{60, 546.7}, {320, 136}}
                  Other(1) {{76, 562.7}, {288, 104}}
                    Button(9) label="Ask App Not to Track" {{76, 562.7}, {288, 48}}
                    Button(9) label="Allow" {{76, 618.7}, {288, 48}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 546.7}, {30, 136}}
                  Other(1) {{374, 636.7}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 649.7}, {320, 30}}
                  Other(1) {{150, 676.7}, {7, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 546.7}, {30, 136}}
                  Other(1) {{374, 636.7}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 649.7}, {320, 30}}
                  Other(1) {{150, 676.7}, {7, 3}}
        Other(1) {{60, 301.3}, {320, 381.3}}
          Other(1) {{60, 301.3}, {320, 381.3}}
            Other(1) {{60, 301.3}, {320, 381.3}}
              Other(1) {{60, 301.3}, {320, 381.3}}
                Other(1) {{60, 301.3}, {320, 381.3}}
                  Other(1) {{60, 301.3}, {440, 956.0}}
              Other(1) {{60, 301.3}, {320, 381.3}}
                Other(1) {{60, 301.3}, {440, 956.0}}
"""#
    )

    /// Jarvis in front, the paste permission alert up (run 139, probe round 3 p9 capture 20 (paste-springboard); 42 nodes).
    static let alertPaste = Capture(
        name: "alertPaste", kind: .alert,
        source: "run 139, probe round 3 p9 capture 20 (paste-springboard)",
        showing: "Jarvis in front, the paste permission alert up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Alert(7) label="“Jarvis” would like to paste from “JarvisUITests-Runner”" {{60, 377}, {320, 230}}
        Other(1) {{60, 377}, {320, 230}}
          Other(1) {{60, 377}, {320, 230}}
            Other(1) {{60, 377}, {320, 230}}
            Other(1) {{60, 377}, {320, 230}}
              ScrollView(46) {{60, 377}, {320, 94}}
                Other(1) {{60, 377}, {320, 94}}
                  StaticText(48) label="“Jarvis” would like to paste from “JarvisUITests-Runner”" {{90, 399}, {260, 42.3}}
                  StaticText(48) label="Do you want to allow this?" {{90, 448.7}, {260, 18}}
                Other(1) {{60, 469}, {320, 2}}
                Other(1) label="Vertical scroll bar, 2 pages" value="0%" {{347, 377}, {30, 94}}
                  Other(1) {{374, 338}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 438}, {320, 30}}
                  Other(1) {{21, 465}, {36, 3}}
                Other(1) label="Vertical scroll bar, 2 pages" value="0%" {{347, 377}, {30, 94}}
                  Other(1) {{374, 338}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 438}, {320, 30}}
                  Other(1) {{21, 465}, {36, 3}}
              ScrollView(46) {{60, 471}, {320, 136}}
                Other(1) {{60, 471}, {320, 136}}
                  Other(1) {{76, 487}, {288, 104}}
                    Button(9) label="Don’t Allow Paste" {{76, 487}, {288, 48}}
                    Button(9) label="Allow Paste" {{76, 543}, {288, 48}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 471}, {30, 136}}
                  Other(1) {{374, 561}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 574}, {320, 30}}
                  Other(1) {{150, 601}, {7, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 471}, {30, 136}}
                  Other(1) {{374, 561}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 574}, {320, 30}}
                  Other(1) {{150, 601}, {7, 3}}
        Other(1) {{60, 377}, {320, 230}}
          Other(1) {{60, 377}, {320, 230}}
            Other(1) {{60, 377}, {320, 230}}
              Other(1) {{60, 377}, {320, 230}}
                Other(1) {{60, 377}, {320, 230}}
                  Other(1) {{60, 377}, {440, 956}}
              Other(1) {{60, 377}, {320, 230}}
                Other(1) {{60, 377}, {440, 956}}
"""#
    )

    static let all: [Capture] = [
        notificationCentre,
        controlCentre,
        appSwitcher,
        homeBadge,
        photoPicker,
        contactPicker,
        documentPicker,
        actionSheet,
        alertWithField,
        safariJsAlert,
        safariJsConfirm,
        safariJsPrompt,
        safariSelect,
        safariDate,
        safariFile,
        safariBasicAuth,
        localNetwork,
        alertLocation,
        alertCamera,
        alertMicrophone,
        alertContacts,
        alertPhotos,
        alertTracking,
        alertPaste,
    ]
}
