import Foundation

// Generated from the raw XCUITest snapshots ("== raw XCUITest snapshot" sections, copied
// verbatim) of SpringBoard that the Mac QA run 146 (probe round 3, P15,
// `TreeFactProbe3SystemTests`) attached (iPhone 17 Pro Max simulator, iOS 26.5): the Home
// Screen and the four SpringBoard states that still hold its icon container. Parsed by
// `RawTreeText`.

/// Real SpringBoard trees of run 146's probe P15, for the Home Screen states of
/// `SystemSurface` (edit mode, App Library, open folder) and the Today view.
enum RealHomeScreenModeTrees {
    struct Capture {
        let name: String
        /// The state (`nil`: the Home Screen, the Today view included).
        let mode: String?
        let showing: String
        let text: String
    }

    /// The plain Home Screen, first page (run 146, P15 capture 01).
    static let home = Capture(
        name: "home", mode: nil,
        showing: "the plain Home Screen, first page",
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
                                                  Other(1) label="THURSDAY, NOVEMBER 26" id=iconic-date-header-view {{19.3, 19.3}, {71.7, 53.3}}
                                                    StaticText(48) label="THURSDAY" {{19.3, 19.3}, {71.7, 15.7}}
                                                    StaticText(48) label="26" {{19.3, 32.0}, {41, 40.7}}
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
              Icon(44) label="Calendar" value="Thursday, November 26" id=Calendar {{32.7, 309.3}, {72.3, 94.7}}
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
          StaticText(48) label="10:03 AM" {{59.7, 22}, {48, 21.7}}
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

    /// The Today view (widgets with news headlines) (run 146, P15 capture 03).
    static let today = Capture(
        name: "today", mode: nil,
        showing: "the Today view (widgets with news headlines)",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{-30, -65}, {500.0, 1086.4}}
      Other(1) {{-30, -65}, {500.0, 1086.4}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 100}}
      Other(1) id=Home screen icons {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                ScrollView(46) id=left-of-home-scroll-view {{0, 0}, {440, 956}}
                  Other(1) {{0, 0}, {440, 1526.7}}
                    Other(1) {{0, 0}, {440, 1526.7}}
                      Other(1) {{0, 0}, {440, 1526.7}}
                        Other(1) {{0, 0}, {440, 1526.7}}
                          Other(1) {{0, 0}, {440, 1526.7}}
                            Other(1) {{0, 0}, {440, 1526.7}}
                              Other(1) {{0, 0}, {440, 1526.7}}
                                Other(1) {{0, 0}, {440, 1526.7}}
                            Other(1) {{0, 0}, {440, 1526.7}}
                              Other(1) {{0, 0}, {440, 1526.7}}
                            Other(1) {{0, 0}, {440, 1526.7}}
                              Other(1) {{0, 0}, {440, 1526.7}}
                      Icon(44) label="Calendar" value="Widget" id=Calendar {{29, 92.0}, {180.7, 203.3}}
                        Other(1) {{31, 94.0}, {176.7, 176.7}}
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
                                                Other(1) label="Calendar" {{0, 0}, {176.7, 176.7}}
                                                  Other(1) {{0, 0}, {176.7, 176.7}}
                                                    Other(1) {{0, 0}, {176.7, 176.7}}
                                                      Other(1) {{0, 0}, {176.7, 176.7}}
                                                        Other(1) {{0, 0}, {0, 0}}
                                                        Other(1) {{19.3, 19.3}, {127.3, 106}}
                                                          Other(1) label="THURSDAY, NOVEMBER 26" id=iconic-date-header-view {{19.3, 19.3}, {71.7, 53.3}}
                                                            StaticText(48) label="THURSDAY" {{19.3, 19.3}, {71.7, 15.7}}
                                                            StaticText(48) label="26" {{19.3, 32.0}, {41, 40.7}}
                                                          StaticText(48) label="No Events Today" {{19.3, 105.0}, {127.3, 20.3}}
                                                  Other(1) label="Calendar" {{0, 0}, {176.7, 176.7}}
                                        Other(1) {{31, 94.0}, {176.7, 176.7}}
                                          Other(1) {{31, 94.0}, {176.7, 176.7}}
                                            Other(1) {{31, 94.0}, {176.7, 176.7}}
                                  Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 237.7}, {176.7, 30}}
                                    Other(1) {{34, 264.7}, {167.7, 3}}
                                  Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 237.7}, {176.7, 30}}
                                    Other(1) {{34, 264.7}, {167.7, 3}}
                      Icon(44) label="Batteries" value="Widget" id=Batteries {{230, 92.0}, {180.7, 203.3}}
                        Other(1) {{232, 94.0}, {176.7, 176.7}}
                          Other(1) {{232, 94.0}, {176.7, 176.7}}
                            Other(1) {{232, 94.0}, {176.7, 176.7}}
                              Other(1) {{232, 94.0}, {176.7, 176.7}}
                                ScrollView(46) {{232, 94.0}, {176.7, 176.7}}
                                  Other(1) {{232, 94.0}, {176.7, 176.7}}
                                    Other(1) {{232, 94.0}, {176.7, 176.7}}
                                    Other(1) {{232, 94.0}, {176.7, 176.7}}
                                      Other(1) {{232, 94.0}, {176.7, 176.7}}
                                        Other(1) {{232, 94.0}, {176.7, 176.7}}
                                          Other(1) {{232, 94.0}, {176.7, 176.7}}
                                            Other(1) {{232, 94.0}, {176.7, 176.7}}
                                        Other(1) {{232, 94.0}, {176.7, 176.7}}
                                          Other(1) {{232, 94.0}, {176.7, 176.7}}
                                            Other(1) {{232, 94.0}, {176.7, 176.7}}
                                              Other(1) {{0, 0}, {176.7, 176.7}}
                                                Other(1) label="Batteries" {{0, 0}, {176.7, 176.7}}
                                                  Other(1) {{0, 0}, {176.7, 176.7}}
                                                    Other(1) {{0, 0}, {176.7, 176.7}}
                                                      Other(1) {{0, 0}, {176.7, 176.7}}
                                                        Other(1) {{-0.0, -0.0}, {176.7, 176.7}}
                                                          Other(1) label="iPhone 17 Pro Max, 100% Charged" {{0, 0}, {176.7, 176.7}}
                                                            Other(1) label="iPhone 17 Pro Max, 100% Charged" {{12, 12}, {71, 71}}
                                                              Other(1) id=battery-widget-background-ring {{12, 12}, {71, 71}}
                                                              Other(1) id=battery-widget-colored-charge-ring {{12, 12}, {71, 71}}
                                                              Other(1) {{75.4, 44.5}, {7.6, 16.1}}
                                                              Other(1) {{75.4, 34.4}, {7.6, 16.1}}
                                                              Image(43) {{47.3, 47.3}, {0, 0}}
                                                            Image(43) label="batterywidget.bolt" {{47.5, 15.2}, {0, 0}}
                                                            Image(43) label="batterywidget.bolt.mask" {{47.5, 15.2}, {0, 0}}
                                                            Other(1) label=", 0% Charged" id=battery-widget-background-ring {{94, 12.2}, {70.7, 70.7}}
                                                            Image(43) label="batterywidget.bolt" {{126.5, 50.2}, {0, 0}}
                                                            Image(43) label="batterywidget.bolt.mask" {{126.5, 50.2}, {0, 0}}
                                                            StaticText(48) label="100%" {{12, 116.3}, {115, 48.3}}
                                                            Other(1) label=", 0% Charged" id=battery-widget-background-ring {{12.2, 94}, {70.7, 70.7}}
                                                            Image(43) label="batterywidget.bolt" {{50.2, 126.5}, {0, 0}}
                                                            Image(43) label="batterywidget.bolt.mask" {{50.2, 126.5}, {0, 0}}
                                                            Other(1) label=", 0% Charged" id=battery-widget-background-ring {{94, 94}, {70.7, 70.7}}
                                                            Image(43) label="batterywidget.bolt" {{126.5, 126.5}, {0, 0}}
                                                            Image(43) label="batterywidget.bolt.mask" {{126.5, 126.5}, {0, 0}}
                                                  Other(1) label="Batteries" {{0, 0}, {176.7, 176.7}}
                                        Other(1) {{232, 94.0}, {176.7, 176.7}}
                                          Other(1) {{232, 94.0}, {176.7, 176.7}}
                                            Other(1) {{232, 94.0}, {176.7, 176.7}}
                                  Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{232, 237.7}, {176.7, 30}}
                                    Other(1) {{235, 264.7}, {167.7, 3}}
                                  Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{232, 237.7}, {176.7, 30}}
                                    Other(1) {{235, 264.7}, {167.7, 3}}
                      Icon(44) label="News" value="Widget" id=News {{29, 293.0}, {382, 638}}
                        Other(1) {{31, 295.0}, {378, 611.3}}
                          Other(1) {{31, 295.0}, {378, 611.3}}
                            Other(1) {{31, 295.0}, {378, 611.3}}
                              Other(1) {{31, 295.0}, {378, 611.3}}
                                ScrollView(46) {{31, 295.0}, {378, 611.3}}
                                  Other(1) {{31, 295.0}, {378, 611.3}}
                                    Other(1) {{31, 295.0}, {378, 611.3}}
                                      Other(1) {{31, 295.0}, {378, 611.3}}
                                        Other(1) {{31, 295.0}, {378, 611.3}}
                                          Other(1) {{31, 295.0}, {378, 611.3}}
                                            Other(1) {{31, 295.0}, {378, 611.3}}
                                              Other(1) {{0, 0}, {378, 611.3}}
                                                Other(1) label="News" {{0, 0}, {378, 611.3}}
                                                  Other(1) {{0, 0}, {378, 611.3}}
                                                    Other(1) {{0, 0}, {378, 611.3}}
                                                      Other(1) {{0, 0}, {378, 611.3}}
                                                        Other(1) {{0, -19.3}, {378, 650}}
                                                          Button(9) {{0, -19.3}, {378, 650}}
                                                        Other(1) {{19.3, 0}, {339.3, 601.3}}
                                                          Button(9) label="Top Stories" {{19.3, 0}, {339.3, 34}}
                                                            StaticText(48) label="Top Stories" {{19.3, 14.7}, {87.7, 19.3}}
                                                          Button(9) label="A “silent war” grips Iran as its economy strains under U.S. blockade, The Wall Street Journal" {{19.3, 34.0}, {339.3, 80}}
                                                            Image(43) label="The Wall Street Journal, A “silent war” grips Iran as its economy strains under U.S. blockade" {{19.3, 40.0}, {97.3, 8.3}}
                                                            StaticText(48) label="The Wall Street Journal, A “silent war” grips Iran as its economy strains under U.S. blockade" {{19.3, 53.0}, {256, 33.7}}
                                                            Image(43) label="The Wall Street Journal, A “silent war” grips Iran as its economy strains under U.S. blockade" {{273.7, 40.0}, {102, 68}}
                                                          Button(9) label="Trending" {{19.3, 124.0}, {339.3, 34}}
                                                          Button(9) label="Aaron Rodgers not returning to Steelers, says Mike McCarthy falls asleep in meetings and is 'unwell', Fox News" {{19.3, 158.0}, {339.3, 79.7}}
                                                            Image(43) label="Fox News, Aaron Rodgers not returning to Steelers, says Mike McCarthy falls asleep in meetings and is 'unwell'" {{19.3, 164.0}, {46.3, 8.7}}
                                                            StaticText(48) label="Fox News, Aaron Rodgers not returning to Steelers, says Mike McCarthy falls asleep in meetings and is 'unwell'" {{19.3, 177.0}, {262.3, 49}}
                                                            Image(43) label="Fox News, Aaron Rodgers not returning to Steelers, says Mike McCarthy falls asleep in meetings and is 'unwell'" {{264.3, 164.0}, {120.7, 67.7}}
                                                          Button(9) label="Hayden Panettiere’s Daughter Kaya, 11, Is Set to Inherit Her Entire Estate—Here’s How Much It’s Worth, InStyle" {{19.3, 237.7}, {339.3, 80}}
                                                            Image(43) label="InStyle, Hayden Panettiere’s Daughter Kaya, 11, Is Set to Inherit Her Entire Estate—Here’s How Much It’s Worth" {{19.3, 243.7}, {45.7, 10}}
                                                            StaticText(48) label="InStyle, Hayden Panettiere’s Daughter Kaya, 11, Is Set to Inherit Her Entire Estate—Here’s How Much It’s Worth" {{19.3, 256.7}, {263.3, 49}}
                                                            Image(43) label="InStyle, Hayden Panettiere’s Daughter Kaya, 11, Is Set to Inherit Her Entire Estate—Here’s How Much It’s Worth" {{279.3, 243.7}, {90.7, 68}}
                                                          Button(9) label="How Did Any Child Survive the 1970s?, The Atlantic" {{19.3, 317.7}, {339.3, 80}}
                                                            Image(43) label="The Atlantic, How Did Any Child Survive the 1970s?" {{19.3, 323.7}, {70.7, 11}}
                                                            StaticText(48) label="The Atlantic, How Did Any Child Survive the 1970s?" {{19.3, 336.7}, {240.7, 18}}
                                                            Image(43) label="The Atlantic, How Did Any Child Survive the 1970s?" {{264.3, 323.7}, {120.7, 68}}
                                                          Button(9) label="58 Dumb, Dumb Dummies Who Made Me Question The Intelligence Of The Human Species, BuzzFeed" {{19.3, 397.7}, {339.3, 79.7}}
                                                            Image(43) label="BuzzFeed, 58 Dumb, Dumb Dummies Who Made Me Question The Intelligence Of The Human Species" {{19.3, 403.7}, {60, 10}}
                                                            StaticText(48) label="BuzzFeed, 58 Dumb, Dumb Dummies Who Made Me Question The Intelligence Of The Human Species" {{19.3, 416.7}, {263.3, 49}}
                                                            Image(43) label="BuzzFeed, 58 Dumb, Dumb Dummies Who Made Me Question The Intelligence Of The Human Species" {{273.7, 403.7}, {102, 67.7}}
                                                          Button(9) label="Featured" {{19.3, 487.3}, {339.3, 34}}
                                                            StaticText(48) label="Featured" {{19.3, 502.0}, {69.7, 19.3}}
                                                          Button(9) label="Five ways to catch a liar, according to science, BBC Science Focus Magazine" {{19.3, 521.3}, {339.3, 80}}
                                                            Image(43) label="BBC Science Focus Magazine, Five ways to catch a liar, according to science" {{19.3, 527.3}, {194.7, 10}}
                                                            StaticText(48) label="BBC Science Focus Magazine, Five ways to catch a liar, according to science" {{19.3, 540.3}, {226.3, 33.7}}
                                                            Image(43) label="BBC Science Focus Magazine, Five ways to catch a liar, according to science" {{290.7, 527.3}, {68, 68}}
                                                  Other(1) label="News" {{0, 0}, {378, 611.3}}
                                        Other(1) {{31, 295.0}, {378, 611.3}}
                                          Other(1) {{31, 295.0}, {378, 611.3}}
                                            Other(1) {{31, 295.0}, {378, 611.3}}
                                  Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 873.3}, {378, 30}}
                                    Other(1) {{34, 900.3}, {369, 3}}
                                  Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 873.3}, {378, 30}}
                                    Other(1) {{34, 900.3}, {369, 3}}
                      Icon(44) label="Maps" value="Widget" id=Maps {{29, 929}, {382, 203.3}}
                        Other(1) {{31, 931}, {378, 176.7}}
                          Other(1) {{31, 931}, {378, 176.7}}
                            Other(1) {{31, 931}, {378, 176.7}}
                              Other(1) {{31, 931}, {378, 176.7}}
                                ScrollView(46) {{31, 931}, {378, 176.7}}
                                  Other(1) {{31, 931}, {378, 176.7}}
                                    Other(1) {{31, 931}, {378, 176.7}}
                                      Other(1) {{31, 931}, {378, 176.7}}
                                        Other(1) {{31, 931}, {378, 176.7}}
                                          Other(1) {{31, 931}, {378, 176.7}}
                                            Other(1) {{31, 931}, {378, 176.7}}
                                              Other(1) {{0, 0}, {378, 176.7}}
                                                Other(1) label="Maps" {{0, 0}, {378, 176.7}}
                                                  Other(1) {{0, 0}, {378, 176.7}}
                                                    Other(1) {{0, 0}, {378, 176.7}}
                                                      Other(1) {{0, 0}, {378, 176.7}}
                                                        Other(1) {{0, -0.0}, {378, 176.7}}
                                                          Image(43) label="Map" id=NearbyWidgetMapImage {{0, 0}, {378, 176.7}}
                                                          Image(43) label="Search" id=NearbyButton {{24, 102.7}, {50, 50}}
                                                          Button(9) label="Search" {{24, 102.7}, {50, 50}}
                                                          Other(1) id=PlaceholderNearbyIcon {{89, 102.7}, {50, 50}}
                                                  Other(1) label="Maps" {{0, 0}, {378, 176.7}}
                                        Other(1) {{31, 931}, {378, 176.7}}
                                          Other(1) {{31, 931}, {378, 176.7}}
                                            Other(1) {{31, 931}, {378, 176.7}}
                                  Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 1040.7}, {378, 30}}
                                    Other(1) {{34, 1067.7}, {369, 3}}
                                  Other(1) label="Horizontal scroll bar, 1 page" value="0%" {{31, 1040.7}, {378, 30}}
                                    Other(1) {{34, 1067.7}, {369, 3}}
                Other(1) {{0, 0}, {440, 956}}
            Other(1) {{1320, 0}, {440, 956}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Maps" id=Maps {{0, 0}, {0, 0}}
              Icon(44) label="Calendar" id=Calendar {{0, 0}, {0, 0}}
              Icon(44) label="Calendar" value="Thursday, November 26" id=Calendar {{0, 0}, {0, 0}}
              Icon(44) label="Photos" id=Photos {{0, 0}, {0, 0}}
              Icon(44) label="Reminders" id=Reminders {{0, 0}, {0, 0}}
              Icon(44) label="News" id=News {{0, 0}, {0, 0}}
              Icon(44) label="Maps" id=Maps {{0, 0}, {0, 0}}
              Icon(44) label="Health" id=Health {{0, 0}, {0, 0}}
              Icon(44) label="Wallet" id=Wallet {{0, 0}, {0, 0}}
              Icon(44) label="Settings" id=Settings {{0, 0}, {0, 0}}
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
            Other(1) label="Search" value="Page 0 of 2" id=spotlight-pill {{621, 763.7}, {78, 30}}
              PageIndicator(34) value="Page 0 of 2" id=Page control {{621, 763.7}, {78, 35}}
              Other(1) id=spotlight-pill {{621, 763.7}, {78, 30}}
                Other(1) {{621, 763.7}, {78, 30}}
                  Image(43) label="Search" id=magnifyingglass {{632.7, 773}, {11.3, 10.7}}
                  StaticText(48) label="Search" {{647, 771.3}, {40.3, 14.3}}
            Other(1) {{440, 821.7}, {440, 134.3}}
              Other(1) {{457, 821.7}, {406, 117.3}}
                Other(1) label="Dock" {{440, 821.7}, {440, 117.3}}
                  Other(1) {{440, 821.7}, {440, 117.3}}
                  Icon(44) label="Safari" value="" id=Safari {{576, 844.3}, {72, 72}}
                    Other(1) {{578, 846.3}, {68, 68}}
                      Other(1) {{578, 846.3}, {68, 68}}
                        Other(1) {{578, 846.3}, {68, 68}}
                          Other(1) {{578, 846.3}, {68, 68}}
                  Icon(44) label="Messages" value="" id=Messages {{672, 844.3}, {72, 72}}
                    Other(1) {{674, 846.3}, {68, 68}}
                      Other(1) {{674, 846.3}, {68, 68}}
                        Other(1) {{674, 846.3}, {68, 68}}
                          Other(1) {{674, 846.3}, {68, 68}}
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
          StaticText(48) label="10:04 AM" {{59.3, 22}, {48.3, 21.7}}
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

    /// The App Library (run 146, P15 capture 05).
    static let appLibrary = Capture(
        name: "appLibrary", mode: "appLibrary",
        showing: "the App Library",
        text: #"""
Application(2) label=" " {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{-30, -65}, {500.0, 1086.4}}
      Other(1) {{-30, -65}, {500.0, 1086.4}}
    Other(1) {{0, 0}, {440, 956}}
      Other(1) {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 100}}
      Other(1) id=Home screen icons {{0, 0}, {440, 956}}
        Other(1) {{0, 0}, {440, 956}}
          Other(1) {{0, 0}, {440, 956}}
            Other(1) {{0, 0}, {440, 956}}
              Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
                  Other(1) id=dewey-search-bar {{0, 0}, {440, 166}}
                    Other(1) {{0, 0}, {440, 166}}
                      Other(1) {{0, 0}, {440, 166}}
                        Other(1) {{0, 0}, {440, 166}}
                          Other(1) {{0, 0}, {440, 166}}
                            Other(1) {{0, 0}, {440, 166}}
                              Other(1) {{0, 0}, {440, 166}}
                          Other(1) {{0, 0}, {440, 166}}
                            Other(1) {{0, 0}, {378, 48}}
                          Other(1) {{0, 0}, {440, 166}}
                            Other(1) {{0, 0}, {440, 166}}
                    SearchField(45) placeholder="App Library" id=dewey-search-field {{31, 94}, {378, 48}}
                      Image(43) label="Search" id=magnifyingglass {{152.3, 107}, {22, 22}}
                  Other(1) {{0, 0}, {440, 956}}
                    Other(1) {{0, 0}, {440, 956}}
                      Other(1) id=dewey-pod-view {{0, 0}, {440, 956}}
                        Other(1) id=dewey-pod-expanded {{0, 0}, {440, 956}}
                          Icon(44) label="Other" value="" id=Other {{29, 172}, {180.7, 203.3}}
                            Other(1) {{31, 174}, {176.7, 190.7}}
                              Other(1) {{31, 174}, {176.7, 176.7}}
                                Other(1) {{31, 174}, {176.7, 176.7}}
                                  Other(1) {{31, 174}, {176.7, 176.7}}
                                    Icon(44) label="Jarvis" value="" id=Jarvis {{43, 186}, {72, 72}}
                                      Other(1) {{45, 188}, {68, 68}}
                                        Other(1) {{45, 188}, {68, 68}}
                                          Other(1) {{45, 188}, {68, 68}}
                                            Other(1) {{45, 188}, {68, 68}}
                                    Icon(44) label="JarvisUITests-Runner" value="" id=JarvisUITests-Runner {{123.3, 186}, {72, 72}}
                                      Other(1) {{125.3, 188}, {68, 68}}
                                        Other(1) {{125.3, 188}, {68, 68}}
                                          Other(1) {{125.3, 188}, {68, 68}}
                                            Other(1) {{125.3, 188}, {68, 68}}
                              Other(1) id=label-view {{94.7, 353.7}, {49, 19.7}}
                                Other(1) {{94.7, 353.7}, {49, 19.7}}
                                  Image(43) {{78.1, 335.1}, {83, 55.7}}
                                  Image(43) {{94.7, 353.7}, {49, 19.7}}
                          Icon(44) label=" System" value="" id= System {{230.3, 172}, {180.7, 203.3}}
                            Other(1) {{232.3, 174}, {176.7, 190.7}}
                              Other(1) {{232.3, 174}, {176.7, 176.7}}
                                Other(1) {{232.3, 174}, {176.7, 176.7}}
                                  Other(1) {{232.3, 174}, {176.7, 176.7}}
                                    Icon(44) label="Watch" value="" id=Watch {{244.3, 186}, {72, 72}}
                                      Other(1) {{246.3, 188}, {68, 68}}
                                        Other(1) {{246.3, 188}, {68, 68}}
                                          Other(1) {{246.3, 188}, {68, 68}}
                                            Other(1) {{246.3, 188}, {68, 68}}
                                    Icon(44) label="Files" value="" id=Files {{324.7, 186}, {72, 72}}
                                      Other(1) {{326.7, 188}, {68, 68}}
                                        Other(1) {{326.7, 188}, {68, 68}}
                                          Other(1) {{326.7, 188}, {68, 68}}
                                            Other(1) {{326.7, 188}, {68, 68}}
                                    Icon(44) label="Fitness" value="" id=Fitness {{244.3, 266.7}, {72, 72}}
                                      Other(1) {{246.3, 268.7}, {68, 68}}
                                        Other(1) {{246.3, 268.7}, {68, 68}}
                                          Other(1) {{246.3, 268.7}, {68, 68}}
                                            Other(1) {{246.3, 268.7}, {68, 68}}
                                    Icon(44) label=" System folder" value="17 apps" id= System {{324.7, 266.7}, {72, 72}}
                                      Icon(44) label="Reminders" id=Reminders {{0, 0}, {0, 0}}
                                      Icon(44) label="Maps" id=Maps {{0, 0}, {0, 0}}
                                      Icon(44) label="News" id=News {{0, 0}, {0, 0}}
                                      Icon(44) label="Messages" id=Messages {{0, 0}, {0, 0}}
                                      Icon(44) label="Preview" id=Preview {{0, 0}, {0, 0}}
                                      Icon(44) label="Calendar" value="Thursday, November 26" id=Calendar {{0, 0}, {0, 0}}
                                      Icon(44) label="Contacts" id=Contacts {{0, 0}, {0, 0}}
                                      Icon(44) label="Passwords" id=Passwords {{0, 0}, {0, 0}}
                                      Icon(44) label="Photos" id=Photos {{0, 0}, {0, 0}}
                                      Icon(44) label="Shortcuts" id=Shortcuts {{0, 0}, {0, 0}}
                                      Icon(44) label="Settings" id=Settings {{0, 0}, {0, 0}}
                                      Icon(44) label="Safari" id=Safari {{0, 0}, {0, 0}}
                                      Icon(44) label="Health" id=Health {{0, 0}, {0, 0}}
                                      Icon(44) label="Wallet" id=Wallet {{0, 0}, {0, 0}}
                              Other(1) id=label-view {{285, 353.7}, {70.7, 19.7}}
                                Other(1) {{285, 353.7}, {70.7, 19.7}}
                                  Image(43) {{268.4, 335.1}, {104.7, 55.7}}
                                  Image(43) {{285, 353.7}, {70.7, 19.7}}
                        Other(1) {{0, 0}, {440, 956}}
                Other(1) {{0, 0}, {440, 956}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Maps" id=Maps {{0, 0}, {0, 0}}
              Icon(44) label="Calendar" id=Calendar {{0, 0}, {0, 0}}
              Icon(44) label="Calendar" value="Thursday, November 26" id=Calendar {{0, 0}, {0, 0}}
              Icon(44) label="Photos" id=Photos {{0, 0}, {0, 0}}
              Icon(44) label="Reminders" id=Reminders {{0, 0}, {0, 0}}
              Icon(44) label="News" id=News {{0, 0}, {0, 0}}
              Icon(44) label="Maps" id=Maps {{0, 0}, {0, 0}}
              Icon(44) label="Health" id=Health {{0, 0}, {0, 0}}
              Icon(44) label="Wallet" id=Wallet {{0, 0}, {0, 0}}
              Icon(44) label="Settings" id=Settings {{0, 0}, {0, 0}}
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
            Other(1) label="Search" value="Page 3 of 2" id=spotlight-pill {{-259, 763.7}, {78, 30}}
              PageIndicator(34) value="Page 3 of 2" id=Page control {{-259, 763.7}, {78, 35}}
              Other(1) id=spotlight-pill {{-259, 763.7}, {78, 30}}
                Other(1) {{-259, 763.7}, {78, 30}}
                  Image(43) label="Search" id=magnifyingglass {{-247.3, 773}, {11.3, 10.7}}
                  StaticText(48) label="Search" {{-233, 771.3}, {40.3, 14.3}}
            Other(1) {{-440, 821.7}, {440, 134.3}}
              Other(1) {{-423, 821.7}, {406, 117.3}}
                Other(1) label="Dock" {{-440, 821.7}, {440, 117.3}}
                  Other(1) {{-440, 821.7}, {440, 117.3}}
                  Icon(44) label="Safari" value="" id=Safari {{-304, 844.3}, {72, 72}}
                    Other(1) {{-302, 846.3}, {68, 68}}
                      Other(1) {{-302, 846.3}, {68, 68}}
                        Other(1) {{-302, 846.3}, {68, 68}}
                          Other(1) {{-302, 846.3}, {68, 68}}
                  Icon(44) label="Messages" value="" id=Messages {{-208, 844.3}, {72, 72}}
                    Other(1) {{-206, 846.3}, {68, 68}}
                      Other(1) {{-206, 846.3}, {68, 68}}
                        Other(1) {{-206, 846.3}, {68, 68}}
                          Other(1) {{-206, 846.3}, {68, 68}}
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
          StaticText(48) label="10:04 AM" {{59.3, 22}, {48.3, 21.7}}
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

    /// An open folder (Utilities) (run 146, P15 capture 07).
    static let folder = Capture(
        name: "folder", mode: "openFolder",
        showing: "an open folder (Utilities)",
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
              TextField(49) value="Utilities" id=Utilities, Folder name {{45.7, 204.3}, {348.7, 63.3}}
              Icon(44) {{0, 0}, {0, 0}}
                Icon(44) label="Shortcuts" value="" id=Shortcuts {{80.7, 319.7}, {76.7, 94.7}}
                  Other(1) {{85.3, 321.7}, {68, 82}}
                    Other(1) {{85.3, 321.7}, {68, 68}}
                      Other(1) {{85.3, 321.7}, {68, 68}}
                        Other(1) {{85.3, 321.7}, {68, 68}}
                    Other(1) id=label-view {{82.7, 392.7}, {72.7, 19.7}}
                      Other(1) {{82.7, 392.7}, {72.7, 19.7}}
                        Image(43) {{66.1, 374.1}, {106.7, 55.7}}
                        Image(43) {{82.7, 392.7}, {72.7, 19.7}}
                Icon(44) label="Passwords" value="" id=Passwords {{178.7, 319.7}, {82.3, 94.7}}
                  Other(1) {{186, 321.7}, {68, 82}}
                    Other(1) {{186, 321.7}, {68, 68}}
                      Other(1) {{186, 321.7}, {68, 68}}
                        Other(1) {{186, 321.7}, {68, 68}}
                    Other(1) id=label-view {{180.7, 392.7}, {78.3, 19.7}}
                      Other(1) {{180.7, 392.7}, {78.3, 19.7}}
                        Image(43) {{164.1, 374.1}, {112.3, 55.7}}
                        Image(43) {{180.7, 392.7}, {78.3, 19.7}}
              PageIndicator(34) value="Page 1 of 1" id=Page control {{201.2, 636.2}, {37.7, 30.7}}
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
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// The Home Screen's edit mode after a long press on the wallpaper (run 146, P15 capture 09).
    static let jiggle = Capture(
        name: "jiggle", mode: "homeEditMode",
        showing: "the Home Screen's edit mode after a long press on the wallpaper",
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
            Button(9) label="Edit" {{50, 19}, {68, 28}}
            Button(9) label="Done" {{322, 19}, {68, 28}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Maps" id=Maps {{0, 0}, {0, 0}}
              Icon(44) label="Calendar" id=Calendar {{0, 0}, {0, 0}}
              Icon(44) label="Calendar" value="Thursday, November 26" id=Calendar {{0, 0}, {0, 0}}
              Icon(44) label="Photos" id=Photos {{0, 0}, {0, 0}}
              Icon(44) label="Reminders" id=Reminders {{0, 0}, {0, 0}}
              Icon(44) label="News" id=News {{0, 0}, {0, 0}}
              Icon(44) label="Maps" id=Maps {{0, 0}, {0, 0}}
              Icon(44) label="Health" id=Health {{0, 0}, {0, 0}}
              Icon(44) label="Wallet" id=Wallet {{0, 0}, {0, 0}}
              Icon(44) label="Settings" id=Settings {{0, 0}, {0, 0}}
            Icon(44) {{0, 0}, {0, 0}}
              Icon(44) label="Fitness" value="" id=Fitness {{33, 92}, {72, 94.7}}
                Button(9) id=DeleteButton {{25.3, 84.3}, {26, 26}}
              Icon(44) label="Watch" value="" id=Watch {{133.7, 92}, {72, 94.7}}
                Button(9) id=DeleteButton {{126.0, 84.3}, {26, 26}}
              Icon(44) label="Contacts" value="" id=Contacts {{234, 92}, {72.3, 94.7}}
                Button(9) id=DeleteButton {{226.6, 84.3}, {26, 26}}
              Icon(44) label="Files" value="" id=Files {{335, 92}, {72, 94.7}}
                Button(9) id=DeleteButton {{327.3, 84.3}, {26, 26}}
              Icon(44) label="Preview" value="" id=Preview {{33, 200.7}, {72, 94.7}}
                Button(9) id=DeleteButton {{25.3, 193.0}, {26, 26}}
              Icon(44) label="Utilities folder" value="2 apps" id=Utilities {{133.7, 200.7}, {72, 94.7}}
                Icon(44) label="Passwords" id=Passwords {{0, 0}, {0, 0}}
                Icon(44) label="Shortcuts" id=Shortcuts {{0, 0}, {0, 0}}
              Icon(44) label="JarvisUITests-Runner" value="" id=JarvisUITests-Runner {{216, 200.7}, {109, 98.0}}
                Button(9) id=DeleteButton {{226.6, 193.0}, {26, 26}}
              Icon(44) label="Jarvis" value="" id=Jarvis {{335, 200.7}, {72, 94.7}}
                Button(9) id=DeleteButton {{327.3, 193.0}, {26, 26}}
            Icon(44) {{0, 0}, {0, 0}}
            Other(1) value="Page 2 of 3" id=spotlight-pill {{181.8, 764.0}, {76.4, 29.4}}
              PageIndicator(34) value="Page 2 of 3" id=Page control {{181, 763.7}, {78, 35}}
              Other(1) id=spotlight-pill {{181.8, 764.0}, {76.4, 29.4}}
                Other(1) {{181.8, 764.0}, {76.4, 29.4}}
                  Image(43) label="Search" id=magnifyingglass {{193.2, 773.1}, {11.1, 10.5}}
                  StaticText(48) label="Search" {{207.3, 771.5}, {39.5, 14.0}}
            Other(1) {{0, 821.7}, {440, 134.3}}
              Other(1) {{17, 821.7}, {406, 117.3}}
                Other(1) label="Dock" {{0, 821.7}, {440, 117.3}}
                  Other(1) {{0, 821.7}, {440, 117.3}}
                  Icon(44) label="Safari" value="" id=Safari {{136, 844.3}, {72, 72}}
                    Button(9) id=DeleteButton {{128.3, 836.6}, {26, 26}}
                  Icon(44) label="Messages" value="" id=Messages {{232, 844.3}, {72, 72}}
                    Button(9) id=DeleteButton {{224.3, 836.6}, {26, 26}}
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
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
  Window(4) {{0, 0}, {440, 956}}
    Other(1) {{0, 0}, {440, 956}}
"""#
    )

    /// The plain Home Screen after Home left the edit mode (run 146, P15 capture 11).
    static let homeAfter = Capture(
        name: "homeAfter", mode: nil,
        showing: "the plain Home Screen after Home left the edit mode",
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
                                                  Other(1) label="THURSDAY, NOVEMBER 26" id=iconic-date-header-view {{19.3, 19.3}, {71.7, 53.3}}
                                                    StaticText(48) label="THURSDAY" {{19.3, 19.3}, {71.7, 15.7}}
                                                    StaticText(48) label="26" {{19.3, 32.0}, {41, 40.7}}
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
              Icon(44) label="Calendar" value="Thursday, November 26" id=Calendar {{32.7, 309.3}, {72.3, 94.7}}
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
          StaticText(48) label="10:04 AM" {{59.3, 22}, {48.3, 21.7}}
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

    static let all: [Capture] = [home, today, appLibrary, folder, jiggle, homeAfter]
}
