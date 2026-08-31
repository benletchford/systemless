/*
 * Toolbox Showcase
 *
 * A deliberately traditional System 7 application whose one source file is
 * compiled into classic 68K and native PowerPC slices. The event-loop shape,
 * update handling, and control tracking follow Macintosh Toolbox Essentials
 * (1992), pp. 2-24, 4-49, 5-30--5-37, and 5-53--5-59.
 */

#include <Types.h>
#include <Resources.h>
#include <Quickdraw.h>
#include <Fonts.h>
#include <Events.h>
#include <Windows.h>
#include <Menus.h>
#include <Controls.h>
#include <ControlDefinitions.h>
#include <TextEdit.h>
#include <Dialogs.h>
#include <ToolUtils.h>
#include <Memory.h>
#include <OSUtils.h>
#include <SegLoad.h>
#include <Scrap.h>

#ifndef TARGET_POWERPC
#define TARGET_POWERPC 0
#endif

#define kMenuBar 128
#define kAppleMenu 128
#define kFileMenu 129
#define kPagesMenu 130
#define kDemoMenu 131
#define kEditMenu 132
#define kPaletteMenu 200
#define kMainWindow 128
#define kAboutAlert 128
#define kPageDescriptions 128

#define kPageOverview 0
#define kPageQuickDraw 1
#define kPageControls 2
#define kPageTextEdit 3
#define kPageWindows 4
#define kPageResources 5
#define kPageCount 6

#define kFileNewCompanion 1
#define kFileClose 2
#define kFileQuit 4

#define kEditUndo 1
#define kEditCut 3
#define kEditCopy 4
#define kEditPaste 5
#define kEditClear 6

#define kDemoAbout 1
#define kDemoToggleCheckbox 2
#define kDemoReset 3
#define kDemoCheckboxState 5
#define kDemoScrollState 6
#define kDemoTextState 7
#define kDemoPalette 8

#define kPaletteRed 1
#define kPaletteGreen 2
#define kPaletteBlue 3

#define kMinWindowWidth 480
#define kMinWindowHeight 300
#define kMaxWindowWidth 620
#define kMaxWindowHeight 430

#define HiWord(value) ((short)(((unsigned long)(value) >> 16) & 0xFFFF))
#define LoWord(value) ((short)((unsigned long)(value) & 0xFFFF))

QDGlobals qd;

static Boolean gDone = false;
static Boolean gCheckboxSelected = false;
static Boolean gScrollMoved = false;
static Boolean gTextEdited = false;
static short gPage = kPageOverview;
static short gPageScrollValue = 0;
static short gPaletteChoice = kPaletteRed;

static WindowPtr gMainWindow = NULL;
static WindowPtr gCompanionWindow = NULL;
static MenuHandle gAppleMenu = NULL;
static MenuHandle gFileMenu = NULL;
static MenuHandle gEditMenu = NULL;
static MenuHandle gPagesMenu = NULL;
static MenuHandle gDemoMenu = NULL;
static MenuHandle gPaletteMenu = NULL;

static ControlHandle gPreviousControl = NULL;
static ControlHandle gNextControl = NULL;
static ControlHandle gPageScrollControl = NULL;
static ControlHandle gCheckboxControl = NULL;
static ControlHandle gRadioOneControl = NULL;
static ControlHandle gRadioTwoControl = NULL;
static ControlHandle gSliderControl = NULL;
static ControlHandle gCompanionControl = NULL;

static TEHandle gTextEdit = NULL;
static Rect gTextView;

static unsigned char *gPageTitles[kPageCount] = {
    (unsigned char *)"\pOverview",
    (unsigned char *)"\pQuickDraw",
    (unsigned char *)"\pControls",
    (unsigned char *)"\pTextEdit",
    (unsigned char *)"\pWindows",
    (unsigned char *)"\pResources & Events"
};

static void InitializeApplication(void);
static void InitializeMenus(void);
static void InitializeMainWindow(void);
static void InitializeControls(void);
static void InitializeTextEdit(void);
static void EventLoop(void);
static void DispatchEvent(EventRecord *event);
static void HandleMouseDown(EventRecord *event);
static void HandleContentClick(WindowPtr window, EventRecord *event);
static void HandleKeyDown(EventRecord *event);
static void HandleUpdate(WindowPtr window);
static void HandleActivate(WindowPtr window, Boolean active);
static void HandleMenuCommand(long choice);
static void HandleEditCommand(short item);
static void UpdateMenus(void);
static void SetPage(short page);
static void UpdateControlVisibility(void);
static void LayoutControls(void);
static void DrawMainWindow(void);
static void DrawCompanionWindow(void);
static void DrawPageHeader(void);
static void DrawOverviewPage(short offset);
static void DrawQuickDrawPage(short offset);
static void DrawControlsPage(short offset);
static void DrawTextEditPage(short offset);
static void DrawWindowsPage(short offset);
static void DrawResourcesPage(short offset);
static void DrawPageMarker(void);
static void DrawLabel(short h, short v, ConstStr255Param text);
static void DrawNumber(short h, short v, long value);
static void DrawFeatureRow(short h, short v, ConstStr255Param manager,
                           ConstStr255Param details);
static void SetDemoColor(void);
static void SetControlVisible(ControlHandle control, Boolean visible);
static void InvalidateMainWindow(void);
static void OpenCompanionWindow(void);
static void CloseCompanionWindow(void);
static void ToggleCheckbox(void);
static void ResetInteractiveState(void);
static void UpdateScrollState(short value);
static void TrackPageScroll(short part, Point mouse);
static void TrackSlider(short part, Point mouse);
static void AdjustCursor(Point globalMouse);
static void CleanUpAndQuit(void);

void main(void) {
    MaxApplZone();
    MoreMasters();
    MoreMasters();
    InitializeApplication();
    EventLoop();
    CleanUpAndQuit();
}

static void InitializeApplication(void) {
    InitGraf(&qd.thePort);
    InitFonts();
    InitWindows();
    InitMenus();
    TEInit();
    InitDialogs(NULL);
    InitCursor();
    FlushEvents(everyEvent, 0);

    InitializeMenus();
    InitializeMainWindow();
    InitializeControls();
    InitializeTextEdit();
    SetPage(kPageOverview);
    ShowWindow(gMainWindow);
    SelectWindow(gMainWindow);
}

static void InitializeMenus(void) {
    Handle menuBar;

    menuBar = GetNewMBar(kMenuBar);
    if (menuBar == NULL) {
        ExitToShell();
    }
    SetMenuBar(menuBar);
    DisposeHandle(menuBar);

    gAppleMenu = GetMenuHandle(kAppleMenu);
    gFileMenu = GetMenuHandle(kFileMenu);
    gEditMenu = GetMenuHandle(kEditMenu);
    gPagesMenu = GetMenuHandle(kPagesMenu);
    gDemoMenu = GetMenuHandle(kDemoMenu);
    gPaletteMenu = GetMenu(kPaletteMenu);

    if (gAppleMenu != NULL) {
        AppendResMenu(gAppleMenu, 'DRVR');
    }
    if (gPaletteMenu != NULL) {
        InsertMenu(gPaletteMenu, -1);
    }
    if (gDemoMenu != NULL) {
        SetItemCmd(gDemoMenu, kDemoPalette, hMenuCmd);
        SetItemMark(gDemoMenu, kDemoPalette, kPaletteMenu);
        DisableItem(gDemoMenu, kDemoCheckboxState);
        DisableItem(gDemoMenu, kDemoScrollState);
        DisableItem(gDemoMenu, kDemoTextState);
    }
    DrawMenuBar();
}

static void InitializeMainWindow(void) {
    gMainWindow = GetNewCWindow(kMainWindow, NULL, (WindowPtr)-1L);
    if (gMainWindow == NULL) {
        ExitToShell();
    }
    SetPort(gMainWindow);
}

static void InitializeControls(void) {
    Rect bounds;

    SetPort(gMainWindow);

    SetRect(&bounds, 18, 360, 108, 382);
    gPreviousControl = NewControl(gMainWindow, &bounds, "\pPrevious", true,
                                  0, 0, 1, pushButProc, 0);
    SetRect(&bounds, 116, 360, 206, 382);
    gNextControl = NewControl(gMainWindow, &bounds, "\pNext", true,
                              0, 0, 1, pushButProc, 0);
    SetRect(&bounds, 570, 54, 586, 345);
    gPageScrollControl = NewControl(gMainWindow, &bounds, "\p", true,
                                    0, 0, 100, scrollBarProc, 0);

    SetRect(&bounds, 76, 120, 270, 140);
    gCheckboxControl = NewControl(gMainWindow, &bounds,
                                  "\pEnable fragile paths", false,
                                  0, 0, 1, checkBoxProc, 0);
    SetRect(&bounds, 76, 158, 270, 178);
    gRadioOneControl = NewControl(gMainWindow, &bounds,
                                  "\pClassic rendering", false,
                                  1, 0, 1, radioButProc, 0);
    SetRect(&bounds, 76, 184, 270, 204);
    gRadioTwoControl = NewControl(gMainWindow, &bounds,
                                  "\pAlternate rendering", false,
                                  0, 0, 1, radioButProc, 0);
    SetRect(&bounds, 76, 232, 420, 248);
    gSliderControl = NewControl(gMainWindow, &bounds, "\p", false,
                                35, 0, 100, scrollBarProc, 0);
    SetRect(&bounds, 76, 152, 264, 176);
    gCompanionControl = NewControl(gMainWindow, &bounds,
                                   "\pOpen Companion Window", false,
                                   0, 0, 1, pushButProc, 0);

    if (gPreviousControl == NULL || gNextControl == NULL ||
        gPageScrollControl == NULL || gCheckboxControl == NULL ||
        gRadioOneControl == NULL || gRadioTwoControl == NULL ||
        gSliderControl == NULL || gCompanionControl == NULL) {
        ExitToShell();
    }
    LayoutControls();
}

static void InitializeTextEdit(void) {
    static char initialText[] =
        "This is a live TextEdit record.\r"
        "Click here and type on either CPU architecture.\r"
        "Cut, Copy, Paste, Clear, selection, caret idling, and redraw all use the classic Toolbox.";
    Rect destination;

    SetPort(gMainWindow);
    SetRect(&gTextView, 62, 110, 530, 300);
    destination = gTextView;
    InsetRect(&destination, 4, 4);
    gTextEdit = TENew(&destination, &gTextView);
    if (gTextEdit == NULL) {
        ExitToShell();
    }
    TESetText(initialText, sizeof(initialText) - 1, gTextEdit);
    TEAutoView(true, gTextEdit);
}

static void EventLoop(void) {
    EventRecord event;
    Boolean gotEvent;

    while (!gDone) {
        gotEvent = WaitNextEvent(everyEvent, &event, 4, NULL);
        AdjustCursor(event.where);
        if (gotEvent) {
            DispatchEvent(&event);
        } else if (gPage == kPageTextEdit && FrontWindow() == gMainWindow) {
            TEIdle(gTextEdit);
        }
    }
}

static void DispatchEvent(EventRecord *event) {
    switch (event->what) {
        case mouseDown:
            HandleMouseDown(event);
            break;
        case keyDown:
        case autoKey:
            HandleKeyDown(event);
            break;
        case updateEvt:
            HandleUpdate((WindowPtr)event->message);
            break;
        case activateEvt:
            HandleActivate((WindowPtr)event->message,
                           (event->modifiers & activeFlag) != 0);
            break;
        default:
            break;
    }
}

static void HandleMouseDown(EventRecord *event) {
    WindowPtr window;
    short part;
    long growResult;
    Rect limits;
    Boolean tracked;

    part = FindWindow(event->where, &window);
    switch (part) {
        case inMenuBar:
            UpdateMenus();
            HandleMenuCommand(MenuSelect(event->where));
            break;
        case inContent:
            if (window != FrontWindow()) {
                SelectWindow(window);
            } else {
                HandleContentClick(window, event);
            }
            break;
        case inDrag:
            DragWindow(window, event->where, &qd.screenBits.bounds);
            break;
        case inGrow:
            SetRect(&limits, kMinWindowWidth, kMinWindowHeight,
                    kMaxWindowWidth, kMaxWindowHeight);
            growResult = GrowWindow(window, event->where, &limits);
            if (growResult != 0) {
                SizeWindow(window, LoWord(growResult), HiWord(growResult), true);
                if (window == gMainWindow) {
                    LayoutControls();
                    InvalidateMainWindow();
                }
            }
            break;
        case inGoAway:
            tracked = TrackGoAway(window, event->where);
            if (tracked) {
                if (window == gCompanionWindow) {
                    CloseCompanionWindow();
                } else if (window == gMainWindow) {
                    gDone = true;
                }
            }
            break;
        case inZoomIn:
        case inZoomOut:
            tracked = TrackBox(window, event->where, part);
            if (tracked) {
                ZoomWindow(window, part, true);
                if (window == gMainWindow) {
                    LayoutControls();
                    InvalidateMainWindow();
                }
            }
            break;
        default:
            break;
    }
}

static void HandleContentClick(WindowPtr window, EventRecord *event) {
    Point mouse;
    ControlHandle control;
    short part;
    short trackedPart;

    if (window != gMainWindow) {
        return;
    }

    SetPort(window);
    mouse = event->where;
    GlobalToLocal(&mouse);
    part = FindControl(mouse, window, &control);
    if (part != 0 && control != NULL) {
        if (control == gPageScrollControl) {
            TrackPageScroll(part, mouse);
            return;
        }
        if (control == gSliderControl) {
            TrackSlider(part, mouse);
            return;
        }

        trackedPart = TrackControl(control, mouse, NULL);
        if (trackedPart == 0) {
            return;
        }
        if (control == gPreviousControl) {
            SetPage((gPage + kPageCount - 1) % kPageCount);
        } else if (control == gNextControl) {
            SetPage((gPage + 1) % kPageCount);
        } else if (control == gCheckboxControl) {
            ToggleCheckbox();
        } else if (control == gRadioOneControl) {
            SetControlValue(gRadioOneControl, 1);
            SetControlValue(gRadioTwoControl, 0);
            InvalidateMainWindow();
        } else if (control == gRadioTwoControl) {
            SetControlValue(gRadioOneControl, 0);
            SetControlValue(gRadioTwoControl, 1);
            InvalidateMainWindow();
        } else if (control == gCompanionControl) {
            OpenCompanionWindow();
        }
        return;
    }

    if (gPage == kPageTextEdit && PtInRect(mouse, &gTextView)) {
        TEClick(mouse, (event->modifiers & shiftKey) != 0, gTextEdit);
    }
}

static void HandleKeyDown(EventRecord *event) {
    char key;
    short page;

    key = (char)(event->message & charCodeMask);
    if ((event->modifiers & cmdKey) != 0) {
        UpdateMenus();
        HandleMenuCommand(MenuKey(key));
        return;
    }

    if (key >= '1' && key <= '6') {
        page = (short)(key - '1');
        SetPage(page);
        return;
    }
    if (key == 0x1C) {
        SetPage((gPage + kPageCount - 1) % kPageCount);
        return;
    }
    if (key == 0x1D) {
        SetPage((gPage + 1) % kPageCount);
        return;
    }
    if (key == 0x1E) {
        UpdateScrollState(gPageScrollValue - 10);
        return;
    }
    if (key == 0x1F) {
        UpdateScrollState(gPageScrollValue + 10);
        return;
    }

    if (gPage == kPageTextEdit && key >= 0x20 && key != 0x7F) {
        TEKey(key, gTextEdit);
        gTextEdited = true;
        UpdateMenus();
    }
}

static void HandleUpdate(WindowPtr window) {
    BeginUpdate(window);
    if (window == gMainWindow) {
        DrawMainWindow();
    } else if (window == gCompanionWindow) {
        DrawCompanionWindow();
    }
    EndUpdate(window);
}

static void HandleActivate(WindowPtr window, Boolean active) {
    ControlHandle controls[8];
    short index;

    if (window != gMainWindow) {
        return;
    }
    controls[0] = gPreviousControl;
    controls[1] = gNextControl;
    controls[2] = gPageScrollControl;
    controls[3] = gCheckboxControl;
    controls[4] = gRadioOneControl;
    controls[5] = gRadioTwoControl;
    controls[6] = gSliderControl;
    controls[7] = gCompanionControl;

    for (index = 0; index < 8; index++) {
        HiliteControl(controls[index], active ? kControlNoPart :
                                           kControlInactivePart);
    }
    if (gPage == kPageTextEdit) {
        if (active) {
            TEActivate(gTextEdit);
        } else {
            TEDeactivate(gTextEdit);
        }
    }
}

static void HandleMenuCommand(long choice) {
    short menuID;
    short item;

    menuID = HiWord(choice);
    item = LoWord(choice);
    if (menuID == 0 || item == 0) {
        HiliteMenu(0);
        return;
    }

    switch (menuID) {
        case kAppleMenu:
            if (item == 1) {
                Alert(kAboutAlert, NULL);
                InvalidateMainWindow();
            }
            break;
        case kFileMenu:
            if (item == kFileNewCompanion) {
                OpenCompanionWindow();
            } else if (item == kFileClose) {
                if (FrontWindow() == gCompanionWindow) {
                    CloseCompanionWindow();
                }
            } else if (item == kFileQuit) {
                gDone = true;
            }
            break;
        case kEditMenu:
            HandleEditCommand(item);
            break;
        case kPagesMenu:
            if (item >= 1 && item <= kPageCount) {
                SetPage(item - 1);
            }
            break;
        case kDemoMenu:
            if (item == kDemoAbout) {
                Alert(kAboutAlert, NULL);
                InvalidateMainWindow();
            } else if (item == kDemoToggleCheckbox) {
                ToggleCheckbox();
            } else if (item == kDemoReset) {
                ResetInteractiveState();
            }
            break;
        case kPaletteMenu:
            if (item >= kPaletteRed && item <= kPaletteBlue) {
                gPaletteChoice = item;
                SetPage(kPageQuickDraw);
            }
            break;
        default:
            break;
    }
    UpdateMenus();
    HiliteMenu(0);
}

static void HandleEditCommand(short item) {
    if (gPage != kPageTextEdit) {
        return;
    }
    switch (item) {
        case kEditCut:
            ZeroScrap();
            TECut(gTextEdit);
            TEToScrap();
            gTextEdited = true;
            break;
        case kEditCopy:
            ZeroScrap();
            TECopy(gTextEdit);
            TEToScrap();
            break;
        case kEditPaste:
            TEFromScrap();
            TEPaste(gTextEdit);
            gTextEdited = true;
            break;
        case kEditClear:
            TEDelete(gTextEdit);
            gTextEdited = true;
            break;
        default:
            break;
    }
    InvalidateMainWindow();
}

static void UpdateMenus(void) {
    short item;
    Boolean textActive;

    if (gPagesMenu != NULL) {
        for (item = 1; item <= kPageCount; item++) {
            CheckItem(gPagesMenu, item, item == gPage + 1);
        }
    }
    if (gPaletteMenu != NULL) {
        for (item = kPaletteRed; item <= kPaletteBlue; item++) {
            CheckItem(gPaletteMenu, item, item == gPaletteChoice);
        }
    }
    if (gDemoMenu != NULL) {
        CheckItem(gDemoMenu, kDemoCheckboxState, gCheckboxSelected);
        CheckItem(gDemoMenu, kDemoScrollState, gScrollMoved);
        CheckItem(gDemoMenu, kDemoTextState, gTextEdited);
    }
    if (gFileMenu != NULL) {
        if (gCompanionWindow != NULL && FrontWindow() == gCompanionWindow) {
            EnableItem(gFileMenu, kFileClose);
        } else {
            DisableItem(gFileMenu, kFileClose);
        }
    }

    textActive = gPage == kPageTextEdit && FrontWindow() == gMainWindow;
    if (gEditMenu != NULL) {
        DisableItem(gEditMenu, kEditUndo);
        if (textActive) {
            EnableItem(gEditMenu, kEditCut);
            EnableItem(gEditMenu, kEditCopy);
            EnableItem(gEditMenu, kEditPaste);
            EnableItem(gEditMenu, kEditClear);
        } else {
            DisableItem(gEditMenu, kEditCut);
            DisableItem(gEditMenu, kEditCopy);
            DisableItem(gEditMenu, kEditPaste);
            DisableItem(gEditMenu, kEditClear);
        }
    }
}

static void SetPage(short page) {
    if (page < 0 || page >= kPageCount) {
        return;
    }
    if (gPage == kPageTextEdit && page != kPageTextEdit) {
        TEDeactivate(gTextEdit);
    }
    gPage = page;
    gPageScrollValue = 0;
    SetControlValue(gPageScrollControl, 0);
    UpdateControlVisibility();
    if (gPage == kPageTextEdit && FrontWindow() == gMainWindow) {
        TEActivate(gTextEdit);
    }
    UpdateMenus();
    InvalidateMainWindow();
}

static void UpdateControlVisibility(void) {
    Boolean controlsPage;
    Boolean windowsPage;

    controlsPage = gPage == kPageControls;
    windowsPage = gPage == kPageWindows;
    SetControlVisible(gCheckboxControl, controlsPage);
    SetControlVisible(gRadioOneControl, controlsPage);
    SetControlVisible(gRadioTwoControl, controlsPage);
    SetControlVisible(gSliderControl, controlsPage);
    SetControlVisible(gCompanionControl, windowsPage);
}

static void LayoutControls(void) {
    Rect portRect;
    short bottom;
    short right;

    if (gMainWindow == NULL) {
        return;
    }
    SetPort(gMainWindow);
    portRect = gMainWindow->portRect;
    bottom = portRect.bottom;
    right = portRect.right;

    MoveControl(gPreviousControl, 18, bottom - 40);
    MoveControl(gNextControl, 116, bottom - 40);
    MoveControl(gPageScrollControl, right - 30, 54);
    SizeControl(gPageScrollControl, 16, bottom - 109);

    SetRect(&gTextView, 62, 110, right - 70, bottom - 98);
    if (gTextEdit != NULL) {
        (**gTextEdit).viewRect = gTextView;
        (**gTextEdit).destRect = gTextView;
        InsetRect(&(**gTextEdit).destRect, 4, 4);
        TECalText(gTextEdit);
    }
}

static void DrawMainWindow(void) {
    Rect portRect;
    short offset;

    SetPort(gMainWindow);
    portRect = gMainWindow->portRect;
    EraseRect(&portRect);
    PenNormal();
    ForeColor(blackColor);
    BackColor(whiteColor);
    TextFont(0);
    TextSize(12);
    TextFace(0);
    TextMode(srcOr);

    DrawPageHeader();
    offset = gPageScrollValue / 2;
    switch (gPage) {
        case kPageOverview:
            DrawOverviewPage(offset);
            break;
        case kPageQuickDraw:
            DrawQuickDrawPage(offset);
            break;
        case kPageControls:
            DrawControlsPage(offset);
            break;
        case kPageTextEdit:
            DrawTextEditPage(offset);
            break;
        case kPageWindows:
            DrawWindowsPage(offset);
            break;
        case kPageResources:
            DrawResourcesPage(offset);
            break;
    }
    DrawControls(gMainWindow);
}

static void DrawCompanionWindow(void) {
    Rect portRect;
    Rect card;
    short index;

    SetPort(gCompanionWindow);
    portRect = gCompanionWindow->portRect;
    EraseRect(&portRect);
    PenNormal();
    DrawLabel(20, 30, "\pCompanion document window");
    DrawLabel(20, 50, "\pMove, activate, zoom, grow, and close me.");

    SetRect(&card, 24, 76, 76, 128);
    for (index = 0; index < 4; index++) {
        if ((index & 1) == 0) {
            PenPat(&qd.gray);
            PaintRoundRect(&card, 12, 12);
        } else {
            PenNormal();
            FrameOval(&card);
        }
        OffsetRect(&card, 62, 0);
    }
    PenNormal();
}

static void DrawPageHeader(void) {
    Str255 description;
    Rect rule;

    TextFace(bold);
    TextSize(18);
    MoveTo(22, 28);
    DrawString(gPageTitles[gPage]);
    TextFace(0);
    TextSize(10);

#if TARGET_POWERPC
    DrawLabel(22, 47, "\pNative PowerPC slice");
#else
    DrawLabel(22, 47, "\pClassic 68K slice");
#endif

    GetIndString(description, kPageDescriptions, gPage + 1);
    MoveTo(22, 68);
    DrawString(description);

    SetRect(&rule, 20, 76, gMainWindow->portRect.right - 46, 78);
    PaintRect(&rule);
    DrawPageMarker();
}

static void DrawPageMarker(void) {
    Rect marker;
    short index;
    short left;

    left = gMainWindow->portRect.right - 166;
    SetRect(&marker, left, 16, left + 16, 32);
    for (index = 0; index < kPageCount; index++) {
        if (index == gPage) {
            PaintRect(&marker);
        } else {
            FrameRect(&marker);
        }
        OffsetRect(&marker, 22, 0);
    }
}

static void DrawOverviewPage(short offset) {
    Rect card;
    short y;

    y = 101 - offset;
    DrawFeatureRow(34, y, "\pMenu Manager",
                   "\pmenus, commands, state, hierarchy");
    DrawFeatureRow(34, y + 34, "\pWindow Manager",
                   "\pupdate, activate, drag, grow, zoom");
    DrawFeatureRow(34, y + 68, "\pControl Manager",
                   "\pbuttons, checks, radio, scroll bars");
    DrawFeatureRow(34, y + 102, "\pQuickDraw + TextEdit",
                   "\pports, regions, color, text, selection");
    DrawFeatureRow(34, y + 136, "\pResource + Event",
                   "\ploaded UI, WaitNextEvent dispatch");

    SetRect(&card, 34, y + 178, 532, y + 230);
    PenPat(&qd.gray);
    FrameRoundRect(&card, 16, 16);
    PenNormal();
    DrawLabel(52, y + 199,
              "\pOne source, one fat APPL, one StuffIt archive.");
    DrawLabel(52, y + 218,
              "\pUse Pages, Previous/Next, or number keys 1-6.");
}

static void DrawQuickDrawPage(short offset) {
    Rect r;
    Rect source;
    Rect destination;
    RgnHandle oldClip;
    RgnHandle rectangleRegion;
    RgnHandle ovalRegion;
    PolyHandle polygon;
    short y;

    y = 98 - offset;
    SetDemoColor();
    SetRect(&r, 34, y, 100, y + 42);
    PaintRect(&r);
    ForeColor(blackColor);
    FrameRect(&r);
    OffsetRect(&r, 82, 0);
    PaintRoundRect(&r, 18, 18);
    ForeColor(blackColor);
    FrameRoundRect(&r, 18, 18);
    OffsetRect(&r, 82, 0);
    SetDemoColor();
    PaintOval(&r);
    ForeColor(blackColor);
    FrameOval(&r);
    OffsetRect(&r, 82, 0);
    FrameArc(&r, 25, 285);
    PaintArc(&r, 315, 80);

    DrawLabel(34, y + 62, "\pPen modes, patterns, lines, and polygon:");
    MoveTo(48, y + 83);
    PenSize(2, 2);
    LineTo(170, y + 112);
    PenPat(&qd.gray);
    PenMode(patXor);
    MoveTo(48, y + 112);
    LineTo(170, y + 83);
    PenNormal();

    polygon = OpenPoly();
    MoveTo(210, y + 112);
    LineTo(250, y + 78);
    LineTo(292, y + 112);
    LineTo(210, y + 112);
    ClosePoly();
    if (polygon != NULL) {
        SetDemoColor();
        PaintPoly(polygon);
        ForeColor(blackColor);
        FramePoly(polygon);
        KillPoly(polygon);
    }

    oldClip = NewRgn();
    rectangleRegion = NewRgn();
    ovalRegion = NewRgn();
    if (oldClip != NULL && rectangleRegion != NULL && ovalRegion != NULL) {
        GetClip(oldClip);
        SetRect(&r, 330, y + 72, 520, y + 126);
        RectRgn(rectangleRegion, &r);
        InsetRect(&r, 28, -10);
        OpenRgn();
        FrameOval(&r);
        CloseRgn(ovalRegion);
        SectRgn(rectangleRegion, ovalRegion, rectangleRegion);
        SetClip(rectangleRegion);
        PenPat(&qd.gray);
        PaintRect(&gMainWindow->portRect);
        PenNormal();
        SetClip(oldClip);
        FrameRgn(rectangleRegion);
    }
    if (oldClip != NULL) DisposeRgn(oldClip);
    if (rectangleRegion != NULL) DisposeRgn(rectangleRegion);
    if (ovalRegion != NULL) DisposeRgn(ovalRegion);

    DrawLabel(34, y + 151, "\pCopyBits overlap copy:");
    SetRect(&source, 34, y + 160, 116, y + 200);
    PenPat(&qd.gray);
    PaintRect(&source);
    PenNormal();
    FrameRect(&source);
    destination = source;
    OffsetRect(&destination, 100, 14);
    CopyBits(&gMainWindow->portBits, &gMainWindow->portBits,
             &source, &destination, srcCopy, NULL);
    FrameRect(&destination);

    ForeColor(blackColor);
    TextFace(bold | italic);
    TextSize(14);
    DrawLabel(280, y + 183, "\pQuickDraw text styles");
    TextFace(underline);
    TextSize(12);
    DrawLabel(280, y + 203, "\pclipped, copied, and colored");
    TextFace(0);
    TextSize(12);
}

static void DrawControlsPage(short offset) {
    short y;

    y = 96 - offset;
    DrawLabel(34, y, "\pControl Manager creates and tracks every live control below.");
    DrawLabel(34, y + 24, "\pThe Demo menu mirrors state for deterministic inspection.");
    DrawLabel(76, y + 120, "\pHorizontal scroll value:");
    DrawNumber(248, y + 120, GetControlValue(gSliderControl));
    DrawLabel(76, y + 176,
              "\pThe right-side document scroll bar moves every showcase page.");
    DrawLabel(76, y + 196,
              "\pArrow keys also drive it through the same application state.");
}

static void DrawTextEditPage(short offset) {
    Rect frame;

    (void)offset;
    DrawLabel(34, 96, "\pClick the framed edit record and type. The Edit menu is live.");
    frame = gTextView;
    FrameRect(&frame);
    InsetRect(&frame, 1, 1);
    FrameRect(&frame);
    TEUpdate(&gTextView, gTextEdit);
}

static void DrawWindowsPage(short offset) {
    Rect back;
    Rect front;
    short y;

    y = 98 - offset;
    DrawLabel(34, y, "\pThe button opens a real second document window.");
    DrawLabel(34, y + 22,
              "\pUse its title bar, zoom box, grow box, close box, and activation.");

    SetRect(&back, 312, y + 42, 512, y + 158);
    SetRect(&front, 268, y + 78, 468, y + 194);
    PenPat(&qd.gray);
    PaintRect(&back);
    PenNormal();
    FrameRect(&back);
    EraseRect(&front);
    FrameRect(&front);
    MoveTo(front.left, front.top + 18);
    LineTo(front.right, front.top + 18);
    DrawLabel(front.left + 12, front.top + 14, "\pWindow layering");

    DrawLabel(34, y + 225, "\pWindow count:");
    DrawNumber(132, y + 225, gCompanionWindow == NULL ? 1 : 2);
}

static void DrawResourcesPage(short offset) {
    short y;

    y = 100 - offset;
    DrawFeatureRow(34, y, "\p'MBAR' / 'MENU'",
                   "\pGetNewMBar, MenuSelect, MenuKey");
    DrawFeatureRow(34, y + 34, "\p'WIND'",
                   "\pGetNewCWindow and window events");
    DrawFeatureRow(34, y + 68, "\p'ALRT' / 'DITL'",
                   "\pAlert and dialog item tracking");
    DrawFeatureRow(34, y + 102, "\p'STR#' / 'vers'",
                   "\pGetIndString and version metadata");
    DrawFeatureRow(34, y + 136, "\p'CODE' / 'cfrg'",
                   "\p68K segments and PowerPC PEF selection");

    DrawLabel(34, y + 188,
              "\pWaitNextEvent yields, then dispatches mouse, key, update,");
    DrawLabel(34, y + 207,
              "\pactivate, menu, dialog, control, and TextEdit behavior.");
}

static void DrawLabel(short h, short v, ConstStr255Param text) {
    MoveTo(h, v);
    DrawString(text);
}

static void DrawNumber(short h, short v, long value) {
    Str255 text;

    NumToString(value, text);
    MoveTo(h, v);
    DrawString(text);
}

static void DrawFeatureRow(short h, short v, ConstStr255Param manager,
                           ConstStr255Param details) {
    Rect badge;

    SetRect(&badge, h, v - 16, h + 176, v + 8);
    PenPat(&qd.gray);
    PaintRoundRect(&badge, 10, 10);
    PenNormal();
    FrameRoundRect(&badge, 10, 10);
    TextFace(bold);
    DrawLabel(h + 10, v, manager);
    TextFace(0);
    DrawLabel(h + 190, v, details);
}

static void SetDemoColor(void) {
    RGBColor color;

    color.red = 0;
    color.green = 0;
    color.blue = 0;
    if (gPaletteChoice == kPaletteRed) {
        color.red = 0xFFFF;
        color.green = 0x3333;
        color.blue = 0x2222;
    } else if (gPaletteChoice == kPaletteGreen) {
        color.red = 0x2222;
        color.green = 0xBBBB;
        color.blue = 0x4444;
    } else {
        color.red = 0x2222;
        color.green = 0x5555;
        color.blue = 0xFFFF;
    }
    RGBForeColor(&color);
}

static void SetControlVisible(ControlHandle control, Boolean visible) {
    if (control == NULL) {
        return;
    }
    if (visible) {
        ShowControl(control);
    } else {
        HideControl(control);
    }
}

static void InvalidateMainWindow(void) {
    if (gMainWindow != NULL) {
        SetPort(gMainWindow);
        InvalRect(&gMainWindow->portRect);
    }
}

static void OpenCompanionWindow(void) {
    Rect bounds;

    if (gCompanionWindow != NULL) {
        SelectWindow(gCompanionWindow);
        return;
    }
    SetRect(&bounds, 150, 118, 550, 330);
    gCompanionWindow = NewCWindow(NULL, &bounds, "\pCompanion Window", true,
                                  zoomDocProc, (WindowPtr)-1L, true, 0);
    if (gCompanionWindow != NULL) {
        SelectWindow(gCompanionWindow);
        InvalRect(&gCompanionWindow->portRect);
    }
    UpdateMenus();
}

static void CloseCompanionWindow(void) {
    if (gCompanionWindow != NULL) {
        DisposeWindow(gCompanionWindow);
        gCompanionWindow = NULL;
        SelectWindow(gMainWindow);
        UpdateMenus();
        InvalidateMainWindow();
    }
}

static void ToggleCheckbox(void) {
    gCheckboxSelected = !gCheckboxSelected;
    SetControlValue(gCheckboxControl, gCheckboxSelected ? 1 : 0);
    UpdateMenus();
    InvalidateMainWindow();
}

static void ResetInteractiveState(void) {
    gCheckboxSelected = false;
    gScrollMoved = false;
    gTextEdited = false;
    gPageScrollValue = 0;
    SetControlValue(gCheckboxControl, 0);
    SetControlValue(gRadioOneControl, 1);
    SetControlValue(gRadioTwoControl, 0);
    SetControlValue(gSliderControl, 35);
    SetControlValue(gPageScrollControl, 0);
    UpdateMenus();
    InvalidateMainWindow();
}

static void UpdateScrollState(short value) {
    if (value < 0) value = 0;
    if (value > 100) value = 100;
    gPageScrollValue = value;
    SetControlValue(gPageScrollControl, value);
    if (value != 0) {
        gScrollMoved = true;
    }
    UpdateMenus();
    InvalidateMainWindow();
}

static void TrackPageScroll(short part, Point mouse) {
    short oldValue;
    short trackedPart;
    short value;

    oldValue = GetControlValue(gPageScrollControl);
    trackedPart = TrackControl(gPageScrollControl, mouse, NULL);
    if (trackedPart == 0) {
        return;
    }
    value = GetControlValue(gPageScrollControl);
    if (part == kControlUpButtonPart) value = oldValue - 5;
    if (part == kControlDownButtonPart) value = oldValue + 5;
    if (part == kControlPageUpPart) value = oldValue - 20;
    if (part == kControlPageDownPart) value = oldValue + 20;
    UpdateScrollState(value);
}

static void TrackSlider(short part, Point mouse) {
    short oldValue;
    short trackedPart;
    short value;

    oldValue = GetControlValue(gSliderControl);
    trackedPart = TrackControl(gSliderControl, mouse, NULL);
    if (trackedPart == 0) {
        return;
    }
    value = GetControlValue(gSliderControl);
    if (part == kControlUpButtonPart) value = oldValue - 5;
    if (part == kControlDownButtonPart) value = oldValue + 5;
    if (part == kControlPageUpPart) value = oldValue - 20;
    if (part == kControlPageDownPart) value = oldValue + 20;
    if (value < 0) value = 0;
    if (value > 100) value = 100;
    SetControlValue(gSliderControl, value);
    InvalidateMainWindow();
}

static void AdjustCursor(Point globalMouse) {
    Point localMouse;
    CursHandle cursor;

    if (gPage == kPageTextEdit && FrontWindow() == gMainWindow) {
        SetPort(gMainWindow);
        localMouse = globalMouse;
        GlobalToLocal(&localMouse);
        if (PtInRect(localMouse, &gTextView)) {
            cursor = GetCursor(iBeamCursor);
            if (cursor != NULL) {
                SetCursor(*cursor);
                return;
            }
        }
    }
    SetCursor(&qd.arrow);
}

static void CleanUpAndQuit(void) {
    if (gCompanionWindow != NULL) {
        DisposeWindow(gCompanionWindow);
        gCompanionWindow = NULL;
    }
    if (gTextEdit != NULL) {
        TEDispose(gTextEdit);
        gTextEdit = NULL;
    }
    if (gMainWindow != NULL) {
        DisposeWindow(gMainWindow);
        gMainWindow = NULL;
    }
    ExitToShell();
}
