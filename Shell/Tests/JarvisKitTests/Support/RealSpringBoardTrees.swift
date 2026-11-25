import Foundation

// Generated from the raw XCUITest snapshots ("== raw XCUITest snapshot" sections, copied
// verbatim) of SpringBoard that the simulator QA runs 134, 135 and 137 attached (iPhone 17
// Pro Max simulator, iOS 26.5 build 23F77). Each tree is SpringBoard's application element
// as `UiTreeBuilder.dump(_: RawElement)` prints it. Parsed by `RawTreeText`.

/// Real SpringBoard trees (iOS 26.5 simulator), for the front-app rule (`FrontApp`).
enum RealSpringBoardTrees {
    enum Kind: String, CaseIterable {
        /// SpringBoard shows its Home Screen.
        case home
        /// Spotlight is up over the Home Screen (SpringBoard's tree, Spotlight's own process on top).
        case spotlight
        /// An app is in front (SpringBoard's tree taken while it was).
        case app
        /// A SpringBoard alert is up over an app.
        case alert
        /// The lock screen.
        case lock
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

    /// the Home Screen, page 1 of 2 (run 137, SystemAppAgentTests.testSpringboardOpensAnApp, observation 1; 274 nodes).
    static let homePage1 = Capture(
        name: "homePage1", kind: .home,
        source: "run 137, SystemAppAgentTests.testSpringboardOpensAnApp, observation 1",
        showing: "the Home Screen, page 1 of 2",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 100}}
      Other(1) id=Home screen icons {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{880, 0}, {440, 956}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Maps" value="Widget" id=Maps {{29, 92.0}, {180.7, 203.3}}
                Other(1) {{31, 94.0}, {176.7, 190.7}}
                  Other(1) {{31, 94.0}, {176.7, 176.7}}
                    Other(1) {{31, 94.0}, {176.7, 176.7}}
                      Other(1) {{31, 94.0}, {176.7, 176.7}}
                        ScrollView(46) {{31, 94.0}, {176.7, 176.7}}
                          Other(1) {{31, 94.0}, {176.7, 176.7}}
                            Other(1) {{31, 94.0}, {176.7, 176.7}}
                              Other(1) {{31, 94.0}, {176.7, 176.7}}
                                Other(1) {{31, 94.0}, {176.7, 176.7}}
                                  Other(1) {{31, 94.0}, {176.7, 176.7}}
                                    Other(1) {{31, 94.0}, {176.7, 176.7}}
                                      Other(1) {{0, 0}, {176.7, 176.7}}
                                        Other(1) label="Maps" {{0, 0}, {176.7, 176.7}}
                                          Other(1) {{0, 0}, {176.7, 176.7}}
                                            Other(1) {{0, 0}, {176.7, 176.7}}
                                              Other(1) {{0, 0}, {176.7, 176.7}}
                                                Other(1) {{-0.0, -0.0}, {176.7, 176.7}}
                                                  Image(43) label="Map" id=NearbyWidgetMapImage {{0, 0}, {176.7, 176.7}}
                                          Other(1) label="Maps" {{0, 0}, {176.7, 176.7}}
                                Other(1) {{31, 94.0}, {176.7, 176.7}}
                                  Other(1) {{31, 94.0}, {176.7, 176.7}}
                                    Other(1) {{31, 94.0}, {176.7, 176.7}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 237.7}, {176.7, 30}}
                            Other(1) {{34, 264.7}, {167.7, 3}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 237.7}, {176.7, 30}}
                            Other(1) {{34, 264.7}, {167.7, 3}}
                  Other(1) id=label-view {{95.3, 273.7}, {47.3, 19.7}}
                    Other(1) {{95.3, 273.7}, {47.3, 19.7}}
                      Image(43) {{78.4, 255.1}, {81.7, 55.7}}
                      Image(43) {{95.3, 273.7}, {47.3, 19.7}}
              Icon(44) label="Calendar" value="Widget, Stack" id=Calendar {{230.3, 92.0}, {180.7, 203.3}}
                Other(1) {{232.3, 94.0}, {176.7, 190.7}}
                  Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                    Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                      Other(1) id=widget-stack:<Calendar,News,Photos> {{232.3, 94.0}, {176.7, 176.7}}
                        ScrollView(46) {{232.3, 94.0}, {176.7, 176.7}}
                          Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                            Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                              Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                  Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                    Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                      Other(1) {{0, 0}, {176.7, 176.7}}
                                        Other(1) label="Calendar" {{0, 0}, {176.7, 176.7}}
                                          Other(1) {{0, 0}, {176.7, 176.7}}
                                            Other(1) {{0, 0}, {176.7, 176.7}}
                                              Other(1) {{0, 0}, {176.7, 176.7}}
                                                Other(1) {{0, 0}, {0, 0}}
                                                Other(1) {{19.3, 19.3}, {127.3, 106}}
                                                  Other(1) label="WEDNESDAY, NOVEMBER 25" id=iconic-date-header-view {{19.3, 19.3}, {82.7, 53.3}}
                                                    StaticText(48) label="WEDNESDAY" {{19.3, 19.3}, {82.7, 15.7}}
                                                    StaticText(48) label="25" {{19.3, 32.0}, {40, 40.7}}
                                                  StaticText(48) label="No Events Today" {{19.3, 105.0}, {127.3, 20.3}}
                                          Other(1) label="Calendar" {{0, 0}, {176.7, 176.7}}
                                Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                  Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                    Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{232.3, 237.7}, {176.7, 30}}
                            Other(1) {{235.3, 264.7}, {167.7, 3}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{232.3, 237.7}, {176.7, 30}}
                            Other(1) {{235.3, 264.7}, {167.7, 3}}
                  Other(1) id=label-view {{286.3, 273.7}, {68, 19.7}}
                    Other(1) {{286.3, 273.7}, {68, 19.7}}
                      Image(43) {{269.7, 255.1}, {102, 55.7}}
                      Image(43) {{286.3, 273.7}, {68, 19.7}}
              Icon(44) label="Calendar" value="Wednesday, November 25" id=Calendar {{32.7, 309.3}, {72.3, 94.7}}
                Other(1) {{35, 311.3}, {68, 82}}
                  Other(1) {{35, 311.3}, {68, 68}}
                    Other(1) {{35, 311.3}, {68, 68}}
                      Other(1) {{35, 311.3}, {68, 68}}
                  Other(1) id=label-view {{34.7, 382.3}, {68, 19.7}}
                    Other(1) {{34.7, 382.3}, {68, 19.7}}
                      Image(43) {{18.1, 363.7}, {102, 55.7}}
                      Image(43) {{34.7, 382.3}, {68, 19.7}}
              Icon(44) label="Photos" value="" id=Photos {{133.7, 309.3}, {72, 94.7}}
                Other(1) {{135.7, 311.3}, {68, 82}}
                  Other(1) {{135.7, 311.3}, {68, 68}}
                    Other(1) {{135.7, 311.3}, {68, 68}}
                      Other(1) {{135.7, 311.3}, {68, 68}}
                  Other(1) id=label-view {{141.3, 382.3}, {56.3, 19.7}}
                    Other(1) {{141.3, 382.3}, {56.3, 19.7}}
                      Image(43) {{124.7, 363.7}, {90.3, 55.7}}
                      Image(43) {{141.3, 382.3}, {56.3, 19.7}}
              Icon(44) label="Reminders" value="" id=Reminders {{229.3, 309.3}, {81.7, 94.7}}
                Other(1) {{236.3, 311.3}, {68, 82}}
                  Other(1) {{236.3, 311.3}, {68, 68}}
                    Other(1) {{236.3, 311.3}, {68, 68}}
                      Other(1) {{236.3, 311.3}, {68, 68}}
                  Other(1) id=label-view {{231.3, 382.3}, {77.7, 19.7}}
                    Other(1) {{231.3, 382.3}, {77.7, 19.7}}
                      Image(43) {{214.7, 363.7}, {111.7, 55.7}}
                      Image(43) {{231.3, 382.3}, {77.7, 19.7}}
              Icon(44) label="News" value="" id=News {{335, 309.3}, {72, 94.7}}
                Other(1) {{337, 311.3}, {68, 82}}
                  Other(1) {{337, 311.3}, {68, 68}}
                    Other(1) {{337, 311.3}, {68, 68}}
                      Other(1) {{337, 311.3}, {68, 68}}
                  Other(1) id=label-view {{346.7, 382.3}, {48, 19.7}}
                    Other(1) {{346.7, 382.3}, {48, 19.7}}
                      Image(43) {{330.1, 363.7}, {82, 55.7}}
                      Image(43) {{346.7, 382.3}, {48, 19.7}}
              Icon(44) label="Maps" value="" id=Maps {{33, 418}, {72, 94.7}}
                Other(1) {{35, 420}, {68, 82}}
                  Other(1) {{35, 420}, {68, 68}}
                    Other(1) {{35, 420}, {68, 68}}
                      Other(1) {{35, 420}, {68, 68}}
                  Other(1) id=label-view {{45, 491}, {47.3, 19.7}}
                    Other(1) {{45, 491}, {47.3, 19.7}}
                      Image(43) {{28.1, 472.4}, {81.7, 55.7}}
                      Image(43) {{45, 491}, {47.3, 19.7}}
              Icon(44) label="Health" value="" id=Health {{133.7, 418}, {72, 94.7}}
                Other(1) {{135.7, 420}, {68, 82}}
                  Other(1) {{135.7, 420}, {68, 68}}
                    Other(1) {{135.7, 420}, {68, 68}}
                      Other(1) {{135.7, 420}, {68, 68}}
                  Other(1) id=label-view {{142.3, 491}, {54, 19.7}}
                    Other(1) {{142.3, 491}, {54, 19.7}}
                      Image(43) {{125.7, 472.4}, {88, 55.7}}
                      Image(43) {{142.3, 491}, {54, 19.7}}
              Icon(44) label="Wallet" value="" id=Wallet {{234.3, 418}, {72, 94.7}}
                Other(1) {{236.3, 420}, {68, 82}}
                  Other(1) {{236.3, 420}, {68, 68}}
                    Other(1) {{236.3, 420}, {68, 68}}
                      Other(1) {{236.3, 420}, {68, 68}}
                  Other(1) id=label-view {{244, 491}, {52, 19.7}}
                    Other(1) {{244, 491}, {52, 19.7}}
                      Image(43) {{227.4, 472.4}, {86, 55.7}}
                      Image(43) {{244, 491}, {52, 19.7}}
              Icon(44) label="Settings" value="" id=Settings {{335, 418}, {72, 94.7}}
                Other(1) {{337, 420}, {68, 82}}
                  Other(1) {{337, 420}, {68, 68}}
                    Other(1) {{337, 420}, {68, 68}}
                      Other(1) {{337, 420}, {68, 68}}
                  Other(1) id=label-view {{338.7, 491}, {64.3, 19.7}}
                    Other(1) {{338.7, 491}, {64.3, 19.7}}
                      Image(43) {{322.1, 472.4}, {98.3, 55.7}}
                      Image(43) {{338.7, 491}, {64.3, 19.7}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Fitness" id=Fitness {{0, 0}, {0, 0}}
              Icon(44) label="Watch" id=Watch {{0, 0}, {0, 0}}
              Icon(44) label="Contacts" id=Contacts {{0, 0}, {0, 0}}
              Icon(44) label="Files" id=Files {{0, 0}, {0, 0}}
              Icon(44) label="Preview" id=Preview {{0, 0}, {0, 0}}
              Icon(44) label="Utilities folder" id=Utilities {{0, 0}, {0, 0}}
                Icon(44) label="Passwords" id=Passwords {{0, 0}, {0, 0}}
                Icon(44) label="Shortcuts" id=Shortcuts {{0, 0}, {0, 0}}
              Icon(44) label="JarvisUITests-Runner" id=JarvisUITests-Runner {{0, 0}, {0, 0}}
              Icon(44) label="Jarvis" id=Jarvis {{0, 0}, {0, 0}}
            Other(1) label="Search" value="Page 1 of 2" id=spotlight-pill {{181, 763.7}, {78, 30}}
              PageIndicator(34) value="Page 1 of 2" id=Page control {{181, 763.7}, {78, 35}}
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
          StaticText(48) label="11:18 AM" {{62.3, 22}, {42.7, 21.7}}
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

    /// the Home Screen (run 137, SystemAppAgentTests.testSpringboardUnknownIconAsksOnce, observation 1; 274 nodes).
    static let homeBeforeIconTap = Capture(
        name: "homeBeforeIconTap", kind: .home,
        source: "run 137, SystemAppAgentTests.testSpringboardUnknownIconAsksOnce, observation 1",
        showing: "the Home Screen",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 100}}
      Other(1) id=Home screen icons {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{880, 0}, {440, 956}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Maps" value="Widget" id=Maps {{29, 92.0}, {180.7, 203.3}}
                Other(1) {{31, 94.0}, {176.7, 190.7}}
                  Other(1) {{31, 94.0}, {176.7, 176.7}}
                    Other(1) {{31, 94.0}, {176.7, 176.7}}
                      Other(1) {{31, 94.0}, {176.7, 176.7}}
                        ScrollView(46) {{31, 94.0}, {176.7, 176.7}}
                          Other(1) {{31, 94.0}, {176.7, 176.7}}
                            Other(1) {{31, 94.0}, {176.7, 176.7}}
                              Other(1) {{31, 94.0}, {176.7, 176.7}}
                                Other(1) {{31, 94.0}, {176.7, 176.7}}
                                  Other(1) {{31, 94.0}, {176.7, 176.7}}
                                    Other(1) {{31, 94.0}, {176.7, 176.7}}
                                      Other(1) {{0, 0}, {176.7, 176.7}}
                                        Other(1) label="Maps" {{0, 0}, {176.7, 176.7}}
                                          Other(1) {{0, 0}, {176.7, 176.7}}
                                            Other(1) {{0, 0}, {176.7, 176.7}}
                                              Other(1) {{0, 0}, {176.7, 176.7}}
                                                Other(1) {{-0.0, -0.0}, {176.7, 176.7}}
                                                  Image(43) label="Map" id=NearbyWidgetMapImage {{0, 0}, {176.7, 176.7}}
                                          Other(1) label="Maps" {{0, 0}, {176.7, 176.7}}
                                Other(1) {{31, 94.0}, {176.7, 176.7}}
                                  Other(1) {{31, 94.0}, {176.7, 176.7}}
                                    Other(1) {{31, 94.0}, {176.7, 176.7}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 237.7}, {176.7, 30}}
                            Other(1) {{34, 264.7}, {167.7, 3}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 237.7}, {176.7, 30}}
                            Other(1) {{34, 264.7}, {167.7, 3}}
                  Other(1) id=label-view {{95.3, 273.7}, {47.3, 19.7}}
                    Other(1) {{95.3, 273.7}, {47.3, 19.7}}
                      Image(43) {{78.4, 255.1}, {81.7, 55.7}}
                      Image(43) {{95.3, 273.7}, {47.3, 19.7}}
              Icon(44) label="Calendar" value="Widget, Stack" id=Calendar {{230.3, 92.0}, {180.7, 203.3}}
                Other(1) {{232.3, 94.0}, {176.7, 190.7}}
                  Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                    Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                      Other(1) id=widget-stack:<Calendar,News,Photos> {{232.3, 94.0}, {176.7, 176.7}}
                        ScrollView(46) {{232.3, 94.0}, {176.7, 176.7}}
                          Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                            Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                              Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                  Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                    Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                      Other(1) {{0, 0}, {176.7, 176.7}}
                                        Other(1) label="Calendar" {{0, 0}, {176.7, 176.7}}
                                          Other(1) {{0, 0}, {176.7, 176.7}}
                                            Other(1) {{0, 0}, {176.7, 176.7}}
                                              Other(1) {{0, 0}, {176.7, 176.7}}
                                                Other(1) {{0, 0}, {0, 0}}
                                                Other(1) {{19.3, 19.3}, {127.3, 106}}
                                                  Other(1) label="WEDNESDAY, NOVEMBER 25" id=iconic-date-header-view {{19.3, 19.3}, {82.7, 53.3}}
                                                    StaticText(48) label="WEDNESDAY" {{19.3, 19.3}, {82.7, 15.7}}
                                                    StaticText(48) label="25" {{19.3, 32.0}, {40, 40.7}}
                                                  StaticText(48) label="No Events Today" {{19.3, 105.0}, {127.3, 20.3}}
                                          Other(1) label="Calendar" {{0, 0}, {176.7, 176.7}}
                                Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                  Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                    Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{232.3, 237.7}, {176.7, 30}}
                            Other(1) {{235.3, 264.7}, {167.7, 3}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{232.3, 237.7}, {176.7, 30}}
                            Other(1) {{235.3, 264.7}, {167.7, 3}}
                  Other(1) id=label-view {{286.3, 273.7}, {68, 19.7}}
                    Other(1) {{286.3, 273.7}, {68, 19.7}}
                      Image(43) {{269.7, 255.1}, {102, 55.7}}
                      Image(43) {{286.3, 273.7}, {68, 19.7}}
              Icon(44) label="Calendar" value="Wednesday, November 25" id=Calendar {{32.7, 309.3}, {72.3, 94.7}}
                Other(1) {{35, 311.3}, {68, 82}}
                  Other(1) {{35, 311.3}, {68, 68}}
                    Other(1) {{35, 311.3}, {68, 68}}
                      Other(1) {{35, 311.3}, {68, 68}}
                  Other(1) id=label-view {{34.7, 382.3}, {68, 19.7}}
                    Other(1) {{34.7, 382.3}, {68, 19.7}}
                      Image(43) {{18.1, 363.7}, {102, 55.7}}
                      Image(43) {{34.7, 382.3}, {68, 19.7}}
              Icon(44) label="Photos" value="" id=Photos {{133.7, 309.3}, {72, 94.7}}
                Other(1) {{135.7, 311.3}, {68, 82}}
                  Other(1) {{135.7, 311.3}, {68, 68}}
                    Other(1) {{135.7, 311.3}, {68, 68}}
                      Other(1) {{135.7, 311.3}, {68, 68}}
                  Other(1) id=label-view {{141.3, 382.3}, {56.3, 19.7}}
                    Other(1) {{141.3, 382.3}, {56.3, 19.7}}
                      Image(43) {{124.7, 363.7}, {90.3, 55.7}}
                      Image(43) {{141.3, 382.3}, {56.3, 19.7}}
              Icon(44) label="Reminders" value="" id=Reminders {{229.3, 309.3}, {81.7, 94.7}}
                Other(1) {{236.3, 311.3}, {68, 82}}
                  Other(1) {{236.3, 311.3}, {68, 68}}
                    Other(1) {{236.3, 311.3}, {68, 68}}
                      Other(1) {{236.3, 311.3}, {68, 68}}
                  Other(1) id=label-view {{231.3, 382.3}, {77.7, 19.7}}
                    Other(1) {{231.3, 382.3}, {77.7, 19.7}}
                      Image(43) {{214.7, 363.7}, {111.7, 55.7}}
                      Image(43) {{231.3, 382.3}, {77.7, 19.7}}
              Icon(44) label="News" value="" id=News {{335, 309.3}, {72, 94.7}}
                Other(1) {{337, 311.3}, {68, 82}}
                  Other(1) {{337, 311.3}, {68, 68}}
                    Other(1) {{337, 311.3}, {68, 68}}
                      Other(1) {{337, 311.3}, {68, 68}}
                  Other(1) id=label-view {{346.7, 382.3}, {48, 19.7}}
                    Other(1) {{346.7, 382.3}, {48, 19.7}}
                      Image(43) {{330.1, 363.7}, {82, 55.7}}
                      Image(43) {{346.7, 382.3}, {48, 19.7}}
              Icon(44) label="Maps" value="" id=Maps {{33, 418}, {72, 94.7}}
                Other(1) {{35, 420}, {68, 82}}
                  Other(1) {{35, 420}, {68, 68}}
                    Other(1) {{35, 420}, {68, 68}}
                      Other(1) {{35, 420}, {68, 68}}
                  Other(1) id=label-view {{45, 491}, {47.3, 19.7}}
                    Other(1) {{45, 491}, {47.3, 19.7}}
                      Image(43) {{28.1, 472.4}, {81.7, 55.7}}
                      Image(43) {{45, 491}, {47.3, 19.7}}
              Icon(44) label="Health" value="" id=Health {{133.7, 418}, {72, 94.7}}
                Other(1) {{135.7, 420}, {68, 82}}
                  Other(1) {{135.7, 420}, {68, 68}}
                    Other(1) {{135.7, 420}, {68, 68}}
                      Other(1) {{135.7, 420}, {68, 68}}
                  Other(1) id=label-view {{142.3, 491}, {54, 19.7}}
                    Other(1) {{142.3, 491}, {54, 19.7}}
                      Image(43) {{125.7, 472.4}, {88, 55.7}}
                      Image(43) {{142.3, 491}, {54, 19.7}}
              Icon(44) label="Wallet" value="" id=Wallet {{234.3, 418}, {72, 94.7}}
                Other(1) {{236.3, 420}, {68, 82}}
                  Other(1) {{236.3, 420}, {68, 68}}
                    Other(1) {{236.3, 420}, {68, 68}}
                      Other(1) {{236.3, 420}, {68, 68}}
                  Other(1) id=label-view {{244, 491}, {52, 19.7}}
                    Other(1) {{244, 491}, {52, 19.7}}
                      Image(43) {{227.4, 472.4}, {86, 55.7}}
                      Image(43) {{244, 491}, {52, 19.7}}
              Icon(44) label="Settings" value="" id=Settings {{335, 418}, {72, 94.7}}
                Other(1) {{337, 420}, {68, 82}}
                  Other(1) {{337, 420}, {68, 68}}
                    Other(1) {{337, 420}, {68, 68}}
                      Other(1) {{337, 420}, {68, 68}}
                  Other(1) id=label-view {{338.7, 491}, {64.3, 19.7}}
                    Other(1) {{338.7, 491}, {64.3, 19.7}}
                      Image(43) {{322.1, 472.4}, {98.3, 55.7}}
                      Image(43) {{338.7, 491}, {64.3, 19.7}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Fitness" id=Fitness {{0, 0}, {0, 0}}
              Icon(44) label="Watch" id=Watch {{0, 0}, {0, 0}}
              Icon(44) label="Contacts" id=Contacts {{0, 0}, {0, 0}}
              Icon(44) label="Files" id=Files {{0, 0}, {0, 0}}
              Icon(44) label="Preview" id=Preview {{0, 0}, {0, 0}}
              Icon(44) label="Utilities folder" id=Utilities {{0, 0}, {0, 0}}
                Icon(44) label="Passwords" id=Passwords {{0, 0}, {0, 0}}
                Icon(44) label="Shortcuts" id=Shortcuts {{0, 0}, {0, 0}}
              Icon(44) label="JarvisUITests-Runner" id=JarvisUITests-Runner {{0, 0}, {0, 0}}
              Icon(44) label="Jarvis" id=Jarvis {{0, 0}, {0, 0}}
            Other(1) label="Search" value="Page 1 of 2" id=spotlight-pill {{181, 763.7}, {78, 30}}
              PageIndicator(34) value="Page 1 of 2" id=Page control {{181, 763.7}, {78, 35}}
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
          StaticText(48) label="11:19 AM" {{62.3, 22}, {42.3, 21.7}}
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

    /// the Home Screen (run 137, SystemAppAgentTests.testSpringboardUnknownIconAsksOnce, observation 2; 274 nodes).
    static let homeAfterIconQuestion = Capture(
        name: "homeAfterIconQuestion", kind: .home,
        source: "run 137, SystemAppAgentTests.testSpringboardUnknownIconAsksOnce, observation 2",
        showing: "the Home Screen",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 100}}
      Other(1) id=Home screen icons {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{880, 0}, {440, 956}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Maps" value="Widget" id=Maps {{29, 92.0}, {180.7, 203.3}}
                Other(1) {{31, 94.0}, {176.7, 190.7}}
                  Other(1) {{31, 94.0}, {176.7, 176.7}}
                    Other(1) {{31, 94.0}, {176.7, 176.7}}
                      Other(1) {{31, 94.0}, {176.7, 176.7}}
                        ScrollView(46) {{31, 94.0}, {176.7, 176.7}}
                          Other(1) {{31, 94.0}, {176.7, 176.7}}
                            Other(1) {{31, 94.0}, {176.7, 176.7}}
                              Other(1) {{31, 94.0}, {176.7, 176.7}}
                                Other(1) {{31, 94.0}, {176.7, 176.7}}
                                  Other(1) {{31, 94.0}, {176.7, 176.7}}
                                    Other(1) {{31, 94.0}, {176.7, 176.7}}
                                      Other(1) {{0, 0}, {176.7, 176.7}}
                                        Other(1) label="Maps" {{0, 0}, {176.7, 176.7}}
                                          Other(1) {{0, 0}, {176.7, 176.7}}
                                            Other(1) {{0, 0}, {176.7, 176.7}}
                                              Other(1) {{0, 0}, {176.7, 176.7}}
                                                Other(1) {{-0.0, -0.0}, {176.7, 176.7}}
                                                  Image(43) label="Map" id=NearbyWidgetMapImage {{0, 0}, {176.7, 176.7}}
                                          Other(1) label="Maps" {{0, 0}, {176.7, 176.7}}
                                Other(1) {{31, 94.0}, {176.7, 176.7}}
                                  Other(1) {{31, 94.0}, {176.7, 176.7}}
                                    Other(1) {{31, 94.0}, {176.7, 176.7}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 237.7}, {176.7, 30}}
                            Other(1) {{34, 264.7}, {167.7, 3}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 237.7}, {176.7, 30}}
                            Other(1) {{34, 264.7}, {167.7, 3}}
                  Other(1) id=label-view {{95.3, 273.7}, {47.3, 19.7}}
                    Other(1) {{95.3, 273.7}, {47.3, 19.7}}
                      Image(43) {{78.4, 255.1}, {81.7, 55.7}}
                      Image(43) {{95.3, 273.7}, {47.3, 19.7}}
              Icon(44) label="Calendar" value="Widget, Stack" id=Calendar {{230.3, 92.0}, {180.7, 203.3}}
                Other(1) {{232.3, 94.0}, {176.7, 190.7}}
                  Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                    Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                      Other(1) id=widget-stack:<Calendar,News,Photos> {{232.3, 94.0}, {176.7, 176.7}}
                        ScrollView(46) {{232.3, 94.0}, {176.7, 176.7}}
                          Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                            Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                              Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                  Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                    Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                      Other(1) {{0, 0}, {176.7, 176.7}}
                                        Other(1) label="Calendar" {{0, 0}, {176.7, 176.7}}
                                          Other(1) {{0, 0}, {176.7, 176.7}}
                                            Other(1) {{0, 0}, {176.7, 176.7}}
                                              Other(1) {{0, 0}, {176.7, 176.7}}
                                                Other(1) {{0, 0}, {0, 0}}
                                                Other(1) {{19.3, 19.3}, {127.3, 106}}
                                                  Other(1) label="WEDNESDAY, NOVEMBER 25" id=iconic-date-header-view {{19.3, 19.3}, {82.7, 53.3}}
                                                    StaticText(48) label="WEDNESDAY" {{19.3, 19.3}, {82.7, 15.7}}
                                                    StaticText(48) label="25" {{19.3, 32.0}, {40, 40.7}}
                                                  StaticText(48) label="No Events Today" {{19.3, 105.0}, {127.3, 20.3}}
                                          Other(1) label="Calendar" {{0, 0}, {176.7, 176.7}}
                                Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                  Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                                    Other(1) {{232.3, 94.0}, {176.7, 176.7}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{232.3, 237.7}, {176.7, 30}}
                            Other(1) {{235.3, 264.7}, {167.7, 3}}
                          Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{232.3, 237.7}, {176.7, 30}}
                            Other(1) {{235.3, 264.7}, {167.7, 3}}
                  Other(1) id=label-view {{286.3, 273.7}, {68, 19.7}}
                    Other(1) {{286.3, 273.7}, {68, 19.7}}
                      Image(43) {{269.7, 255.1}, {102, 55.7}}
                      Image(43) {{286.3, 273.7}, {68, 19.7}}
              Icon(44) label="Calendar" value="Wednesday, November 25" id=Calendar {{32.7, 309.3}, {72.3, 94.7}}
                Other(1) {{35, 311.3}, {68, 82}}
                  Other(1) {{35, 311.3}, {68, 68}}
                    Other(1) {{35, 311.3}, {68, 68}}
                      Other(1) {{35, 311.3}, {68, 68}}
                  Other(1) id=label-view {{34.7, 382.3}, {68, 19.7}}
                    Other(1) {{34.7, 382.3}, {68, 19.7}}
                      Image(43) {{18.1, 363.7}, {102, 55.7}}
                      Image(43) {{34.7, 382.3}, {68, 19.7}}
              Icon(44) label="Photos" value="" id=Photos {{133.7, 309.3}, {72, 94.7}}
                Other(1) {{135.7, 311.3}, {68, 82}}
                  Other(1) {{135.7, 311.3}, {68, 68}}
                    Other(1) {{135.7, 311.3}, {68, 68}}
                      Other(1) {{135.7, 311.3}, {68, 68}}
                  Other(1) id=label-view {{141.3, 382.3}, {56.3, 19.7}}
                    Other(1) {{141.3, 382.3}, {56.3, 19.7}}
                      Image(43) {{124.7, 363.7}, {90.3, 55.7}}
                      Image(43) {{141.3, 382.3}, {56.3, 19.7}}
              Icon(44) label="Reminders" value="" id=Reminders {{229.3, 309.3}, {81.7, 94.7}}
                Other(1) {{236.3, 311.3}, {68, 82}}
                  Other(1) {{236.3, 311.3}, {68, 68}}
                    Other(1) {{236.3, 311.3}, {68, 68}}
                      Other(1) {{236.3, 311.3}, {68, 68}}
                  Other(1) id=label-view {{231.3, 382.3}, {77.7, 19.7}}
                    Other(1) {{231.3, 382.3}, {77.7, 19.7}}
                      Image(43) {{214.7, 363.7}, {111.7, 55.7}}
                      Image(43) {{231.3, 382.3}, {77.7, 19.7}}
              Icon(44) label="News" value="" id=News {{335, 309.3}, {72, 94.7}}
                Other(1) {{337, 311.3}, {68, 82}}
                  Other(1) {{337, 311.3}, {68, 68}}
                    Other(1) {{337, 311.3}, {68, 68}}
                      Other(1) {{337, 311.3}, {68, 68}}
                  Other(1) id=label-view {{346.7, 382.3}, {48, 19.7}}
                    Other(1) {{346.7, 382.3}, {48, 19.7}}
                      Image(43) {{330.1, 363.7}, {82, 55.7}}
                      Image(43) {{346.7, 382.3}, {48, 19.7}}
              Icon(44) label="Maps" value="" id=Maps {{33, 418}, {72, 94.7}}
                Other(1) {{35, 420}, {68, 82}}
                  Other(1) {{35, 420}, {68, 68}}
                    Other(1) {{35, 420}, {68, 68}}
                      Other(1) {{35, 420}, {68, 68}}
                  Other(1) id=label-view {{45, 491}, {47.3, 19.7}}
                    Other(1) {{45, 491}, {47.3, 19.7}}
                      Image(43) {{28.1, 472.4}, {81.7, 55.7}}
                      Image(43) {{45, 491}, {47.3, 19.7}}
              Icon(44) label="Health" value="" id=Health {{133.7, 418}, {72, 94.7}}
                Other(1) {{135.7, 420}, {68, 82}}
                  Other(1) {{135.7, 420}, {68, 68}}
                    Other(1) {{135.7, 420}, {68, 68}}
                      Other(1) {{135.7, 420}, {68, 68}}
                  Other(1) id=label-view {{142.3, 491}, {54, 19.7}}
                    Other(1) {{142.3, 491}, {54, 19.7}}
                      Image(43) {{125.7, 472.4}, {88, 55.7}}
                      Image(43) {{142.3, 491}, {54, 19.7}}
              Icon(44) label="Wallet" value="" id=Wallet {{234.3, 418}, {72, 94.7}}
                Other(1) {{236.3, 420}, {68, 82}}
                  Other(1) {{236.3, 420}, {68, 68}}
                    Other(1) {{236.3, 420}, {68, 68}}
                      Other(1) {{236.3, 420}, {68, 68}}
                  Other(1) id=label-view {{244, 491}, {52, 19.7}}
                    Other(1) {{244, 491}, {52, 19.7}}
                      Image(43) {{227.4, 472.4}, {86, 55.7}}
                      Image(43) {{244, 491}, {52, 19.7}}
              Icon(44) label="Settings" value="" id=Settings {{335, 418}, {72, 94.7}}
                Other(1) {{337, 420}, {68, 82}}
                  Other(1) {{337, 420}, {68, 68}}
                    Other(1) {{337, 420}, {68, 68}}
                      Other(1) {{337, 420}, {68, 68}}
                  Other(1) id=label-view {{338.7, 491}, {64.3, 19.7}}
                    Other(1) {{338.7, 491}, {64.3, 19.7}}
                      Image(43) {{322.1, 472.4}, {98.3, 55.7}}
                      Image(43) {{338.7, 491}, {64.3, 19.7}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Fitness" id=Fitness {{0, 0}, {0, 0}}
              Icon(44) label="Watch" id=Watch {{0, 0}, {0, 0}}
              Icon(44) label="Contacts" id=Contacts {{0, 0}, {0, 0}}
              Icon(44) label="Files" id=Files {{0, 0}, {0, 0}}
              Icon(44) label="Preview" id=Preview {{0, 0}, {0, 0}}
              Icon(44) label="Utilities folder" id=Utilities {{0, 0}, {0, 0}}
                Icon(44) label="Shortcuts" id=Shortcuts {{0, 0}, {0, 0}}
                Icon(44) label="Passwords" id=Passwords {{0, 0}, {0, 0}}
              Icon(44) label="JarvisUITests-Runner" id=JarvisUITests-Runner {{0, 0}, {0, 0}}
              Icon(44) label="Jarvis" id=Jarvis {{0, 0}, {0, 0}}
            Other(1) label="Search" value="Page 1 of 2" id=spotlight-pill {{181, 763.7}, {78, 30}}
              PageIndicator(34) value="Page 1 of 2" id=Page control {{181, 763.7}, {78, 35}}
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
          StaticText(48) label="11:19 AM" {{62.3, 22}, {42.3, 21.7}}
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

    /// the Home Screen (run 137, SystemAppAgentTests.testSafariFillsAndSubmitsAForm, observation 1; 209 nodes).
    static let homeBeforeSafari = Capture(
        name: "homeBeforeSafari", kind: .home,
        source: "run 137, SystemAppAgentTests.testSafariFillsAndSubmitsAForm, observation 1",
        showing: "the Home Screen",
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
              Icon(44) label="Jarvis" value="" id=Jarvis {{335, 200.7}, {72, 94.7}}
                Other(1) {{337, 202.7}, {68, 82.0}}
                  Other(1) {{337, 202.7}, {68, 68.0}}
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
          StaticText(48) label="11:17 AM" {{63, 22}, {41, 21.7}}
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

    /// the Home Screen with the Settings icon menu up (run 134, TreeFactProbeTests fact-home-icon-menu capture 01 (the same tree in run 135); 48 nodes).
    static let homeIconMenu = Capture(
        name: "homeIconMenu", kind: .home,
        source: "run 134, TreeFactProbeTests fact-home-icon-menu capture 01 (the same tree in run 135)",
        showing: "the Home Screen with the Settings icon menu up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) label="Preview" {{332, 415}, {78.3, 78.3}}
          Other(1) {{332, 415}, {78.3, 78.3}}
            Other(1) {{332, 415}, {78.3, 78.3}}
              Other(1) {{332, 415}, {78.3, 78.3}}
        Other(1) {{160.3, 501.3}, {250, 272}}
          Other(1) {{160.3, 501.3}, {250, 272}}
            Other(1) {{160.3, 501.3}, {250, 272}}
              Other(1) {{160.3, 501.3}, {250, 272}}
              CollectionView(32) {{160.3, 501.3}, {250, 956}}
                Cell(75) {{160.3, 511.3}, {250, 42}}
                  Button(9) label="Bluetooth" id=bluetooth {{160.3, 511.3}, {250, 42}}
                    Image(43) label="Bluetooth" id=bluetooth {{192.0, 522.7}, {17, 19.7}}
                Cell(75) {{160.3, 553.3}, {250, 42}}
                  Button(9) label="Wi-Fi" id=wifi {{160.3, 553.3}, {250, 42}}
                    Image(43) label="Wi-Fi" id=wifi {{189.0, 566.7}, {22.7, 16}}
                Cell(75) {{160.3, 595.3}, {250, 42}}
                  Button(9) label="Cellular" id=cellular {{160.3, 595.3}, {250, 42}}
                    Image(43) label="cellular data" id=antenna.radiowaves.left.and.right {{190.3, 607.7}, {20.3, 17.7}}
                Cell(75) {{160.3, 637.3}, {250, 42}}
                  Button(9) label="Battery" id=battery {{160.3, 637.3}, {250, 42}}
                    Image(43) label="battery.100" id=battery.100 {{186.0, 652.0}, {28.7, 13.3}}
                Cell(75) {{160.3, 679.3}, {250, 42}}
                  Button(9) label="Edit Home Screen" id=com.apple.springboardhome.application-shortcut-item.rearrange-icons {{160.3, 679.3}, {250, 42}}
                    Image(43) label="Apps" id=apps.iphone {{191.7, 690.7}, {17.3, 19.7}}
                Cell(75) {{160.3, 721.3}, {250, 42}}
                  Button(9) label="Remove App" id=com.apple.springboardhome.application-shortcut-item.remove-app {{160.3, 721.3}, {250, 42}}
                    Image(43) label="remove" id=minus.circle {{190.7, 733.0}, {19.7, 19}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{377.3, 531.3}, {30, 212}}
                  Other(1) {{404.3, 534.3}, {3, 206}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{222.3, 710.3}, {188, 30}}
                  Other(1) {{225.3, 737.3}, {182, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{377.3, 531.3}, {30, 212}}
                  Other(1) {{404.3, 534.3}, {3, 206}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{222.3, 710.3}, {188, 30}}
                  Other(1) {{225.3, 737.3}, {182, 3}}
            Other(1) {{160.3, 501.3}, {250, 272}}
              Other(1) {{160.3, 501.3}, {250, 272}}
                Other(1) {{160.3, 501.3}, {250, 272}}
                  Other(1) {{160.3, 501.3}, {250, 272}}
                    Other(1) {{160.3, 501.3}, {250, 272}}
                      Other(1) {{160.3, 501.3}, {250, 44}}
                  Other(1) {{160.3, 501.3}, {250, 272}}
                    Other(1) {{160.3, 501.3}, {250, 44}}
"""#
    )

    /// Spotlight up over the Home Screen (Settings typed) (run 134, fact-spotlight-rows capture 02; 106 nodes).
    static let spotlightOverHome134 = Capture(
        name: "spotlightOverHome134", kind: .spotlight,
        source: "run 134, fact-spotlight-rows capture 02",
        showing: "Spotlight up over the Home Screen (Settings typed)",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 100}}
      Other(1) id=Home screen icons {{0, 0}, {440, 956}}
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
      Other(1) {{0, 638}, {440, 318}}
        Other(1) id=inputView {{0, 638}, {440, 318}}
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
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 638}, {440, 318}}
        Other(1) id=inputView {{0, 638}, {440, 318}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 638}, {440, 318}}
        Other(1) id=inputView {{0, 638}, {440, 318}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 638}, {440, 318}}
        Other(1) id=inputView {{0, 638}, {440, 318}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
      Other(1) {{0, 0}, {440, 54}}
        Other(1) {{0, 0}, {440, 54}}
          StaticText(48) label="7:38 AM" {{64.3, 22}, {38.3, 21.7}}
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

    /// Spotlight up over the Home Screen (example.com typed) (run 135, fact-spotlight-rows capture 05; 106 nodes).
    static let spotlightOverHome135 = Capture(
        name: "spotlightOverHome135", kind: .spotlight,
        source: "run 135, fact-spotlight-rows capture 05",
        showing: "Spotlight up over the Home Screen (example.com typed)",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 100}}
      Other(1) id=Home screen icons {{0, 0}, {440, 956}}
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
      Other(1) {{0, 638}, {440, 318}}
        Other(1) id=inputView {{0, 638}, {440, 318}}
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
  Window(4) {{0, 0}, {1320, 2868}}
    Other(1) {{0, 0}, {1320, 2868}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 638}, {440, 318}}
        Other(1) id=inputView {{0, 638}, {440, 318}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 638}, {440, 318}}
        Other(1) id=inputView {{0, 638}, {440, 318}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 638}, {440, 318}}
        Other(1) id=inputView {{0, 638}, {440, 318}}
  Window(4) {{0, 0}, {440, 956}}
    StatusBar(25) {{0, 0}, {440, 54}}
      Other(1) {{0, 0}, {440, 54}}
        Other(1) {{0, 0}, {440, 54}}
          StaticText(48) label="8:44 AM" {{63.3, 22}, {40.3, 21.7}}
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

    /// Jarvis in front, its edit menu up (run 134, fact-c-menus capture 02; 128 nodes).
    static let jarvisEditMenu134 = Capture(
        name: "jarvisEditMenu134", kind: .app,
        source: "run 134, fact-c-menus capture 02",
        showing: "Jarvis in front, its edit menu up",
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
                                                    StaticText(48) label="7:34 AM" {{64.3, 22}, {38.3, 21.7}}
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

    /// Jarvis in front, its edit menu up (run 135, fact-c-menus capture 02; 128 nodes).
    static let jarvisEditMenu135 = Capture(
        name: "jarvisEditMenu135", kind: .app,
        source: "run 135, fact-c-menus capture 02",
        showing: "Jarvis in front, its edit menu up",
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
                                                    StaticText(48) label="8:39 AM" {{63.7, 22}, {40, 21.7}}
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

    /// Jarvis in front, a context menu up (run 134, fact-c-menus capture 05; 121 nodes).
    static let jarvisContextMenu134 = Capture(
        name: "jarvisContextMenu134", kind: .app,
        source: "run 134, fact-c-menus capture 05",
        showing: "Jarvis in front, a context menu up",
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
                                                    StaticText(48) label="7:34 AM" {{64.3, 22}, {38.3, 21.7}}
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

    /// Jarvis in front, a context menu up (run 135, fact-c-menus capture 05; 121 nodes).
    static let jarvisContextMenu135 = Capture(
        name: "jarvisContextMenu135", kind: .app,
        source: "run 135, fact-c-menus capture 05",
        showing: "Jarvis in front, a context menu up",
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
                                                    StaticText(48) label="8:39 AM" {{63.7, 22}, {40, 21.7}}
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

    /// Jarvis in front, the share sheet up (run 134, fact-g-share-sheet capture 02; 121 nodes).
    static let jarvisShareSheet134 = Capture(
        name: "jarvisShareSheet134", kind: .app,
        source: "run 134, fact-g-share-sheet capture 02",
        showing: "Jarvis in front, the share sheet up",
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
                                                    StaticText(48) label="7:35 AM" {{64.7, 22}, {38, 21.7}}
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

    /// Jarvis in front, the share sheet up (run 135, fact-g-share-sheet capture 02; 121 nodes).
    static let jarvisShareSheet135 = Capture(
        name: "jarvisShareSheet135", kind: .app,
        source: "run 135, fact-g-share-sheet capture 02",
        showing: "Jarvis in front, the share sheet up",
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
                                                    StaticText(48) label="8:40 AM" {{63.3, 22}, {40.3, 21.7}}
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

    /// Jarvis in front, SFSafariViewController up (run 134, fact-b-browser-sheets capture 02; 122 nodes).
    static let jarvisSafariVC134 = Capture(
        name: "jarvisSafariVC134", kind: .app,
        source: "run 134, fact-b-browser-sheets capture 02",
        showing: "Jarvis in front, SFSafariViewController up",
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
                                                    StaticText(48) label="7:34 AM" {{64.3, 22}, {38.3, 21.7}}
                                                    Other(1) {{65.3, 14.7}, {36.7, 36.7}}
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

    /// Jarvis in front, SFSafariViewController up (run 135, fact-b-browser-sheets capture 02; 122 nodes).
    static let jarvisSafariVC135 = Capture(
        name: "jarvisSafariVC135", kind: .app,
        source: "run 135, fact-b-browser-sheets capture 02",
        showing: "Jarvis in front, SFSafariViewController up",
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
                                                    StaticText(48) label="8:38 AM" {{63.7, 22}, {40, 21.7}}
                                                    Other(1) {{65.3, 14.7}, {36.7, 36.7}}
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

    /// Jarvis in front, the web sign-in sheet up (run 135, fact-b-browser-sheets capture 08; 120 nodes).
    static let jarvisWebSignIn135 = Capture(
        name: "jarvisWebSignIn135", kind: .app,
        source: "run 135, fact-b-browser-sheets capture 08",
        showing: "Jarvis in front, the web sign-in sheet up",
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
                                                    StaticText(48) label="8:38 AM" {{63.7, 22}, {40, 21.7}}
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

    /// Settings in front, a notification banner over it (run 134, fact-a-banner capture 04; 147 nodes).
    static let settingsBanner134 = Capture(
        name: "settingsBanner134", kind: .app,
        source: "run 134, fact-a-banner capture 04",
        showing: "Settings in front, a notification banner over it",
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
              Other(1) label="Settings" id=card:com.apple.Preferences:sceneID:com.apple.Preferences-default {{0, 0}, {440, 956}}
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
                                                    StaticText(48) label="7:33 AM" {{64.7, 14.3}, {38, 21.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                                    Button(9) label="Return to Jarvis" id=breadcrumb {{12.0, 37.3}, {44.3, 13.3}}
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
      Other(1) {{0, 0}, {440, 956}}
      Type83(83) {{8, 58.7}, {424, 66}}
        Other(1) label="Notification" {{8, 58.7}, {424, 66}}
          Other(1) {{8, 58.7}, {424, 66}}
            Type83(83) label="JARVIS, now, Jarvis probe, Probe banner 4417" id=NotificationShortLookView {{8, 58.7}, {424, 66}}
              Other(1) id=ShortLook.Platter {{8, 58.7}, {424, 66}}
                Other(1) {{8, 58.7}, {424, 66}}
                Other(1) {{8, 58.7}, {424, 66}}
                Other(1) id=ShortLook.Platter.Content {{8, 58.7}, {424, 66}}
                  Button(9) label="now, Jarvis probe, Probe banner 4417" id=ShortLook.Platter.Content.Seamless {{8, 58.7}, {424, 66}}
                    Other(1) id=ShortLook.Platter.Content.Seamless.CrossFade {{8, 58.7}, {424, 66}}
                      StaticText(48) label="Jarvis probe" id=NotificationTitle {{72, 72.7}, {308, 18}}
                      Other(1) id=NotificationBody.TopAligned.originalMessage {{72, 90.7}, {343, 18}}
                        StaticText(48) label="Probe banner 4417" id=TextContent.Primary {{72, 90.7}, {343, 18}}
                  Other(1) {{8, 58.7}, {424, 66}}
                    Other(1) {{8, 58.7}, {424, 66}}
                      Other(1) {{8, 58.7}, {424, 66}}
                        Other(1) {{8, 58.7}, {424, 66}}
                          Other(1) {{8, 58.7}, {424, 66}}
                            Other(1) {{8, 58.7}, {424, 66}}
                        Other(1) {{8, 58.7}, {424, 66}}
                          Other(1) {{8, 58.7}, {424, 66}}
      Other(1) {{8, 58.7}, {424, 66}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Settings in front, a notification banner over it (run 135, fact-a-banner capture 04; 147 nodes).
    static let settingsBanner135 = Capture(
        name: "settingsBanner135", kind: .app,
        source: "run 135, fact-a-banner capture 04",
        showing: "Settings in front, a notification banner over it",
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
              Other(1) label="Settings" id=card:com.apple.Preferences:sceneID:com.apple.Preferences-default {{0, 0}, {440, 956}}
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
                                                    StaticText(48) label="8:37 AM" {{64.3, 14.3}, {38.7, 21.7}}
                                                    Other(1) label="Cellular" value="No signal" {{312.3, 25.3}, {21.7, 13.7}}
                                                    Other(1) value="SSID, 3 of 3 Wi-Fi bars" id=3 of 3 Wi-Fi bars {{341.3, 25}, {19.7, 14}}
                                                    Other(1) label="100% battery power" value="Not charging" {{368.3, 25}, {31.3, 14.3}}
                                                    Button(9) label="Return to Jarvis" id=breadcrumb {{12.0, 37.3}, {44.3, 13.3}}
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
      Other(1) {{0, 0}, {440, 956}}
      Type83(83) {{8, 58.7}, {424, 66}}
        Other(1) label="Notification" {{8, 58.7}, {424, 66}}
          Other(1) {{8, 58.7}, {424, 66}}
            Type83(83) label="JARVIS, now, Jarvis probe, Probe banner 4417" id=NotificationShortLookView {{8, 58.7}, {424, 66}}
              Other(1) id=ShortLook.Platter {{8, 58.7}, {424, 66}}
                Other(1) {{8, 58.7}, {424, 66}}
                Other(1) {{8, 58.7}, {424, 66}}
                Other(1) id=ShortLook.Platter.Content {{8, 58.7}, {424, 66}}
                  Button(9) label="now, Jarvis probe, Probe banner 4417" id=ShortLook.Platter.Content.Seamless {{8, 58.7}, {424, 66}}
                    Other(1) id=ShortLook.Platter.Content.Seamless.CrossFade {{8, 58.7}, {424, 66}}
                      StaticText(48) label="Jarvis probe" id=NotificationTitle {{72, 72.7}, {308, 18}}
                      Other(1) id=NotificationBody.TopAligned.originalMessage {{72, 90.7}, {343, 18}}
                        StaticText(48) label="Probe banner 4417" id=TextContent.Primary {{72, 90.7}, {343, 18}}
                  Other(1) {{8, 58.7}, {424, 66}}
                    Other(1) {{8, 58.7}, {424, 66}}
                      Other(1) {{8, 58.7}, {424, 66}}
                        Other(1) {{8, 58.7}, {424, 66}}
                          Other(1) {{8, 58.7}, {424, 66}}
                            Other(1) {{8, 58.7}, {424, 66}}
                        Other(1) {{8, 58.7}, {424, 66}}
                          Other(1) {{8, 58.7}, {424, 66}}
      Other(1) {{8, 58.7}, {424, 66}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// Jarvis in front, the notification permission alert up (run 134, fact-a-banner capture 01 (the same tree in run 135); 42 nodes).
    static let alertNotificationPermission = Capture(
        name: "alertNotificationPermission", kind: .alert,
        source: "run 134, fact-a-banner capture 01 (the same tree in run 135)",
        showing: "Jarvis in front, the notification permission alert up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Alert(7) label="“Jarvis” Would Like to Send You Notifications" {{60, 385}, {320, 214}}
        Other(1) {{60, 385}, {320, 214}}
          Other(1) {{60, 385}, {320, 214}}
            Other(1) {{60, 385}, {320, 214}}
            Other(1) {{60, 385}, {320, 214}}
              ScrollView(46) {{60, 385}, {320, 134}}
                Other(1) {{60, 385}, {320, 134}}
                  StaticText(48) label="“Jarvis” Would Like to Send You Notifications" {{90, 407}, {260, 42.3}}
                  StaticText(48) label="Notifications may include alerts, sounds, and icon badges. These can be configured in Settings." {{90, 456.7}, {260, 58.0}}
                Other(1) {{60, 517}, {320, 2}}
                Other(1) label="Vertical scroll bar, 2 pages" value="0%" {{347, 385}, {30, 134}}
                  Other(1) {{374, 346}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 486}, {320, 30}}
                  Other(1) {{21, 513}, {36, 3}}
                Other(1) label="Vertical scroll bar, 2 pages" value="0%" {{347, 385}, {30, 134}}
                  Other(1) {{374, 346}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 486}, {320, 30}}
                  Other(1) {{21, 513}, {36, 3}}
              ScrollView(46) {{60, 519}, {320, 80}}
                Other(1) {{60, 519}, {320, 80}}
                  Other(1) {{76, 535}, {288, 48}}
                    Button(9) label="Don’t Allow" {{76, 535}, {140, 48}}
                    Button(9) label="Allow" {{224, 535}, {140, 48}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 519}, {30, 80}}
                  Other(1) {{374, 609}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 566}, {320, 30}}
                  Other(1) {{150, 593}, {7, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 519}, {30, 80}}
                  Other(1) {{374, 609}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 566}, {320, 30}}
                  Other(1) {{150, 593}, {7, 3}}
        Other(1) {{60, 385}, {320, 214}}
          Other(1) {{60, 385}, {320, 214}}
            Other(1) {{60, 385}, {320, 214}}
              Other(1) {{60, 385}, {320, 214}}
                Other(1) {{60, 385}, {320, 214}}
                  Other(1) {{60, 385}, {440, 956}}
              Other(1) {{60, 385}, {320, 214}}
                Other(1) {{60, 385}, {440, 956}}
"""#
    )

    /// Jarvis in front, the web sign-in consent alert up (run 135, fact-b-browser-sheets capture 06; 42 nodes).
    static let alertSignInConsent = Capture(
        name: "alertSignInConsent", kind: .alert,
        source: "run 135, fact-b-browser-sheets capture 06",
        showing: "Jarvis in front, the web sign-in consent alert up",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
      Alert(7) label="“Jarvis” Wants to Use “127.0.0.1” to Sign In" {{60, 395}, {320, 194}}
        Other(1) {{60, 395}, {320, 194}}
          Other(1) {{60, 395}, {320, 194}}
            Other(1) {{60, 395}, {320, 194}}
            Other(1) {{60, 395}, {320, 194}}
              ScrollView(46) {{60, 395}, {320, 114}}
                Other(1) {{60, 395}, {320, 114}}
                  StaticText(48) label="“Jarvis” Wants to Use “127.0.0.1” to Sign In" {{90, 417}, {260, 42.3}}
                  StaticText(48) label="This allows the app and website to share information about you." {{90, 466.7}, {260, 38}}
                Other(1) {{60, 507}, {320, 2}}
                Other(1) label="Vertical scroll bar, 2 pages" value="0%" {{347, 395}, {30, 114}}
                  Other(1) {{374, 356}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 476}, {320, 30}}
                  Other(1) {{21, 503}, {36, 3}}
                Other(1) label="Vertical scroll bar, 2 pages" value="0%" {{347, 395}, {30, 114}}
                  Other(1) {{374, 356}, {3, 36}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 476}, {320, 30}}
                  Other(1) {{21, 503}, {36, 3}}
              ScrollView(46) {{60, 509}, {320, 80}}
                Other(1) {{60, 509}, {320, 80}}
                  Other(1) {{76, 525}, {288, 48}}
                    Button(9) label="Cancel" {{76, 525}, {140, 48}}
                    Button(9) label="Continue" {{224, 525}, {140, 48}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 509}, {30, 80}}
                  Other(1) {{374, 599}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 556}, {320, 30}}
                  Other(1) {{150, 583}, {7, 3}}
                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{347, 509}, {30, 80}}
                  Other(1) {{374, 599}, {3, 7}}
                Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{60, 556}, {320, 30}}
                  Other(1) {{150, 583}, {7, 3}}
        Other(1) {{60, 395}, {320, 194}}
          Other(1) {{60, 395}, {320, 194}}
            Other(1) {{60, 395}, {320, 194}}
              Other(1) {{60, 395}, {320, 194}}
                Other(1) {{60, 395}, {320, 194}}
                  Other(1) {{60, 395}, {440, 956}}
              Other(1) {{60, 395}, {320, 194}}
                Other(1) {{60, 395}, {440, 956}}
"""#
    )

    /// the lock screen (run 137, LockScreenProbeTests step 3d (after a tap while locked); 204 nodes).
    static let lockScreen = Capture(
        name: "lockScreen", kind: .lock,
        source: "run 137, LockScreenProbeTests step 3d (after a tap while locked)",
        showing: "the lock screen",
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
          Other(1) id=jindo-container-view:1,client-identifier:com.apple.springboard,element-identifier:systemApertureElementIdentifierLock {{157.3, 14.0}, {125.3, 36.7}}
            Other(1) {{147.3, 10}, {145.3, 52}}
            Other(1) {{157.3, 14.0}, {125.3, 36.7}}
              Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                  Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                    Other(1) label="Swipe up to unlock" id=regular.view {{157.3, 14.0}, {125.3, 36.7}}
                      Other(1) {{157.3, 14.0}, {125.3, 36.7}}
                        Other(1) label="Swipe up to unlock" id=proud-lock {{157.3, 24.8}, {12, 15}}
                          Other(1) {{157.3, 24.8}, {12, 15}}
                            Other(1) {{156.6, 23.9}, {13.6, 16.9}}
                              Other(1) {{-8.98847e+307, -8.98847e+307}, {1.79769e+308, 1.79769e+308}}
                                Other(1) label="Locked" {{-8.98847e+307, -8.98847e+307}, {1.79769e+308, 1.79769e+308}}
                        Other(1) {{270.7, 24.8}, {12, 15}}
                          Other(1) {{270.7, 24.8}, {12, 15}}
                      Other(1) {{169.3, 25.0}, {12, 15.0}}
                        Other(1) {{169.3, 25.0}, {12, 15.0}}
                      Other(1) {{169.3, 25.0}, {0, 15.0}}
                        Other(1) {{169.3, 25.0}, {0, 15.0}}
                      Other(1) {{270.7, 25.0}, {0, 15.0}}
                        Other(1) {{270.7, 25.0}, {0, 15.0}}
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
                Other(1) {{0, 0}, {440, 1912}}
                  Other(1) id=lockscreen-date-view {{6.6, 14.3}, {426.8, 927.3}}
                    StaticText(48) label="Wednesday, November 25" {{45.0, 72}, {350.3, 36.3}}
                    StaticText(48) label="11:10 AM" {{64, 86.7}, {310.3, 155.3}}
                    Other(1) {{40.5, 225.8}, {358.9, 73.7}}
                      Other(1) {{40.5, 225.8}, {358.9, 73.7}}
                        Other(1) {{40.5, 225.8}, {358.9, 73.7}}
                          Other(1) {{21.1, 206.4}, {397.7, 112.5}}
                            Other(1) {{21.1, 206.4}, {397.7, 112.5}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{330.1, 47.4}, {51.7, 2.3}}
                    Other(1) {{329.3, 75.8}, {53.3, 2.3}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) {{0, 0}, {440, 956}}
                  Other(1) {{160, 894.7}, {120, 17.3}}
              Other(1) {{6.6, 14.3}, {426.8, 927.3}}
                ScrollView(46) {{6.6, 14.3}, {426.8, 927.3}}
                  Other(1) {{6.6, 14.3}, {426.8, 927.3}}
                    Other(1) {{6.6, 14.3}, {426.8, 927.3}}
                      Other(1) {{6.6, 14.3}, {426.8, 927.3}}
                        Other(1) {{6.6, 14.3}, {426.8, 927.3}}
                          ScrollView(46) {{6.6, 14.3}, {426.8, 927.3}}
                            ScrollView(46) {{20.2, 14.5}, {399.6, 219.5}}
                              ScrollView(46) {{20.2, 14.7}, {399.6, 219.5}}
                                Button(9) id=ListCell {{20.2, 14.7}, {399.6, 219.2}}
                                  Other(1) id=ListCell.ContentView {{20.2, 14.7}, {399.6, 219.2}}
                                    Other(1) {{20.2, 14.7}, {399.6, 219.2}}
                                      Other(1) {{20.2, 14.7}, {399.6, 219.2}}
                                        Other(1) {{20.2, 14.7}, {399.6, 219.2}}
                                        Other(1) {{20.2, 14.7}, {399.6, 219.2}}
                                        Other(1) {{20.2, 14.7}, {399.6, 219.2}}
                                          Other(1) {{20.2, 14.7}, {399.6, 219.2}}
                                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{387.8, 61.2}, {29.1, 173.0}}
                                  Other(1) {{414, 64.5}, {2.9, 166.8}}
                                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{387.8, 61.2}, {29.1, 173.0}}
                                  Other(1) {{414, 64.5}, {2.9, 166.8}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{387.8, 61.1}, {29.1, 173.0}}
                                Other(1) {{414, 64.0}, {2.9, 167.2}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{387.8, 61.1}, {29.1, 173.0}}
                                Other(1) {{414, 64.0}, {2.9, 167.2}}
                            ScrollView(46) {{20.2, 733.4}, {399.6, 76.3}}
                              ScrollView(46) {{20.2, 733.4}, {399.6, 76.3}}
                                Button(9) label="10 minutes ago, Ready for Apple Intelligence, Time to experience the new personal intelligence system." id=ListCell {{20.2, 733.4}, {399.6, 76.3}}
                                  Other(1) id=ListCell.ContentView {{20.2, 733.4}, {399.6, 76.3}}
                                    Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                      Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                        Type83(83) label="10 minutes ago, Ready for Apple Intelligence, Time to experience the new personal intelligence system." id=NotificationShortLookView {{20.2, 733.4}, {399.6, 76.3}}
                                          Other(1) id=ShortLook.Platter {{20.2, 733.4}, {399.6, 76.3}}
                                            Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                            Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                            Other(1) id=ShortLook.Platter.Content {{20.2, 733.4}, {399.6, 76.3}}
                                              Button(9) label="10 minutes ago, Ready for Apple Intelligence, Time to experience the new personal intelligence system." id=ShortLook.Platter.Content.Seamless {{20.2, 733.4}, {399.6, 76.3}}
                                                Other(1) id=ShortLook.Platter.Content.Seamless.CrossFade {{20.2, 733.4}, {399.6, 76.3}}
                                                  StaticText(48) label="Ready for Apple Intelligence" id=NotificationTitle {{82.3, 744.4}, {261.6, 17.5}}
                                                  Other(1) id=NotificationBody.TopAligned.originalMessage {{82.3, 761.9}, {321.1, 34.9}}
                                                    StaticText(48) label="Time to experience the new personal intelligence system." id=TextContent.Primary {{82.3, 761.9}, {321.1, 34.9}}
                                              Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                                Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                                  Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                                    Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                                      Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                                        Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                                    Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                                      Other(1) {{20.2, 733.4}, {399.6, 76.3}}
                                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{387.8, 733.4}, {29.1, 76.3}}
                                  Other(1) {{414, 736.3}, {2.9, 70.5}}
                                Other(1) label="Vertical scroll bar, 1 page" value="0%" {{387.8, 733.4}, {29.1, 76.3}}
                                  Other(1) {{414, 736.3}, {2.9, 70.5}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{387.8, 733.4}, {29.1, 76.3}}
                                Other(1) {{414, 736.3}, {2.9, 70.5}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{387.8, 733.4}, {29.1, 76.3}}
                                Other(1) {{414, 736.3}, {2.9, 70.5}}
                            ScrollView(46) {{20.2, 817.7}, {399.6, 8.7}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{387.8, 817.7}, {29.1, 8.7}}
                                Other(1) {{414, 788.6}, {2.9, 34.9}}
                              Other(1) label="Vertical scroll bar, 1 page" value="0%" {{387.8, 817.7}, {29.1, 8.7}}
                                Other(1) {{414, 788.6}, {2.9, 34.9}}
                          Other(1) {{6.6, 14.3}, {426.8, 927.3}}
                  Other(1) label="Vertical scroll bar, 1 page" value="0%" {{401.4, 29.9}, {29.1, 895.6}}
                    Other(1) {{427.6, 32.8}, {2.9, 889.8}}
                  Other(1) label="Vertical scroll bar, 1 page" value="0%" {{401.4, 29.9}, {29.1, 895.6}}
                    Other(1) {{427.6, 32.8}, {2.9, 889.8}}
              Other(1) {{6.6, 14.3}, {426.8, 927.3}}
                StatusBar(25) {{6.6, 14.3}, {426.8, 52.4}}
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

    static let all: [Capture] = [
        homePage1,
        homeBeforeIconTap,
        homeAfterIconQuestion,
        homeBeforeSafari,
        homeIconMenu,
        spotlightOverHome134,
        spotlightOverHome135,
        jarvisEditMenu134,
        jarvisEditMenu135,
        jarvisContextMenu134,
        jarvisContextMenu135,
        jarvisShareSheet134,
        jarvisShareSheet135,
        jarvisSafariVC134,
        jarvisSafariVC135,
        jarvisWebSignIn135,
        settingsBanner134,
        settingsBanner135,
        alertNotificationPermission,
        alertSignInConsent,
        lockScreen,
    ]
}
