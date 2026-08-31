/*
 * One Toolbox application built unchanged for 68K and native PowerPC.
 * Its menu checkmarks expose deterministic state to automated runners while
 * the window remains useful for human inspection on a classic Macintosh.
 */

#include <Controls.h>
#include <ControlDefinitions.h>
#include <Dialogs.h>
#include <Events.h>
#include <Fonts.h>
#include <Memory.h>
#include <Menus.h>
#include <OSUtils.h>
#include <Quickdraw.h>
#include <Resources.h>
#include <ToolUtils.h>
#include <Windows.h>

#define rMenuBar 128
#define rMainWindow 128

#define mApple 128
#define mPages 129
#define mState 130
#define mFile 131

#define iGraphics 1
#define iControls 2
#define iWindows 3

#define iButtonState 1
#define iCheckboxState 2
#define iScrollbarState 3
#define iWindowState 4

#define iQuit 1

#define pageGraphics 1
#define pageControls 2
#define pageWindows 3

static QDGlobals qd;
static WindowPtr gMainWindow;
static WindowPtr gAuxWindow;
static ControlHandle gButton;
static ControlHandle gCheckbox;
static ControlHandle gScrollbar;
static short gPage = pageGraphics;
static Boolean gQuit = false;
static Boolean gButtonActivated = false;

static MenuHandle StateMenu(void)
{
    return GetMenuHandle(mState);
}

static void DrawHeading(ConstStr255Param heading)
{
    TextFont(systemFont);
    TextSize(12);
    TextFace(bold);
    MoveTo(24, 34);
    DrawString(heading);
    TextFace(0);
}

static void DrawGraphicsPage(void)
{
    Rect r;
    Rect clip;
    RgnHandle savedClip;
    RGBColor red;
    RGBColor blue;
    RGBColor black;

    DrawHeading("\pGraphics, patterns, clipping, color, and text");

    SetRect(&r, 24, 55, 174, 135);
    PenPat(&qd.gray);
    PaintRect(&r);
    PenNormal();
    FrameRect(&r);

    red.red = 0xffff;
    red.green = 0x2222;
    red.blue = 0x2222;
    blue.red = 0x2222;
    blue.green = 0x4444;
    blue.blue = 0xffff;
    black.red = black.green = black.blue = 0;

    RGBForeColor(&red);
    SetRect(&r, 205, 55, 325, 135);
    PaintOval(&r);

    savedClip = NewRgn();
    GetClip(savedClip);
    SetRect(&clip, 360, 55, 455, 135);
    ClipRect(&clip);
    RGBForeColor(&blue);
    SetRect(&r, 330, 35, 485, 155);
    FrameRoundRect(&r, 24, 24);
    SetClip(savedClip);
    DisposeRgn(savedClip);

    RGBForeColor(&black);
    MoveTo(24, 175);
    LineTo(505, 175);
    TextFont(3);
    TextSize(10);
    MoveTo(24, 205);
    DrawString("\pThe same source and resources drive both CPU slices.");
}

static void DrawControlsPage(void)
{
    DrawHeading("\pControls and scroll bars");
    MoveTo(24, 70);
    DrawString("\pClick each control; the State menu records the result.");
    DrawControls(gMainWindow);
}

static void DrawWindowsPage(void)
{
    DrawHeading("\pWindow lifecycle and update events");
    MoveTo(24, 70);
    DrawString("\pThis page opens a second document window.");
    MoveTo(24, 92);
    DrawString("\pChoose another page to dispose it again.");
}

static void DrawMainWindow(void)
{
    SetPort(gMainWindow);
    EraseRect(&gMainWindow->portRect);
    if (gPage == pageGraphics) {
        DrawGraphicsPage();
    } else if (gPage == pageControls) {
        DrawControlsPage();
    } else {
        DrawWindowsPage();
    }
}

static void DrawAuxWindow(void)
{
    if (gAuxWindow == nil) {
        return;
    }
    SetPort(gAuxWindow);
    EraseRect(&gAuxWindow->portRect);
    DrawHeading("\pAuxiliary window");
    MoveTo(24, 68);
    DrawString("\pCreated and destroyed through ordinary Window Manager calls.");
}

static void ShowPageControls(Boolean visible)
{
    if (visible) {
        ShowControl(gButton);
        ShowControl(gCheckbox);
        ShowControl(gScrollbar);
    } else {
        HideControl(gButton);
        HideControl(gCheckbox);
        HideControl(gScrollbar);
    }
}

static void SetPage(short page)
{
    MenuHandle pages;
    Rect bounds;

    gPage = page;
    pages = GetMenuHandle(mPages);
    CheckItem(pages, iGraphics, page == pageGraphics);
    CheckItem(pages, iControls, page == pageControls);
    CheckItem(pages, iWindows, page == pageWindows);
    ShowPageControls(page == pageControls);

    if (page == pageWindows && gAuxWindow == nil) {
        SetRect(&bounds, 180, 155, 570, 300);
        gAuxWindow = NewCWindow(nil, &bounds, "\pAuxiliary Window", true,
                                documentProc, (WindowPtr)-1, true, 0);
        CheckItem(StateMenu(), iWindowState, gAuxWindow != nil);
        DrawAuxWindow();
    } else if (page != pageWindows && gAuxWindow != nil) {
        DisposeWindow(gAuxWindow);
        gAuxWindow = nil;
        CheckItem(StateMenu(), iWindowState, false);
    }

    DrawMainWindow();
}

static void Initialize(void)
{
    Handle menuBar;
    Rect r;

    InitGraf(&qd.thePort);
    InitFonts();
    InitWindows();
    InitMenus();
    TEInit();
    InitDialogs(nil);
    InitCursor();

    menuBar = GetNewMBar(rMenuBar);
    if (menuBar == nil) {
        ExitToShell();
    }
    SetMenuBar(menuBar);
    DisposeHandle(menuBar);
    DrawMenuBar();

    gMainWindow = GetNewCWindow(rMainWindow, nil, (WindowPtr)-1);
    if (gMainWindow == nil) {
        ExitToShell();
    }
    SetPort(gMainWindow);
    ShowWindow(gMainWindow);

    SetRect(&r, 40, 255, 150, 279);
    gButton = NewControl(gMainWindow, &r, "\pActivate", false, 0, 0, 1,
                         pushButProc, 0);
    SetRect(&r, 185, 255, 315, 279);
    gCheckbox = NewControl(gMainWindow, &r, "\pCheckbox", false, 0, 0, 1,
                           checkBoxProc, 0);
    SetRect(&r, 40, 310, 500, 326);
    gScrollbar = NewControl(gMainWindow, &r, "\p", false, 0, 0, 10,
                            scrollBarProc, 0);
    SetPage(pageGraphics);
}

static void DoMenuChoice(long choice)
{
    short menuID;
    short item;

    menuID = HiWord(choice);
    item = LoWord(choice);
    if (menuID == mPages && item >= iGraphics && item <= iWindows) {
        SetPage(item);
    } else if (menuID == mFile && item == iQuit) {
        gQuit = true;
    }
    HiliteMenu(0);
}

static void DoContentClick(WindowPtr window, Point where)
{
    ControlHandle control;
    short part;
    short trackedPart;
    short value;

    if (window != gMainWindow || gPage != pageControls) {
        return;
    }
    SetPort(window);
    GlobalToLocal(&where);
    part = FindControl(where, window, &control);
    if (part == 0) {
        return;
    }
    trackedPart = TrackControl(control, where, nil);
    if (trackedPart == 0) {
        return;
    }
    part = trackedPart;
    if (control == gButton) {
        gButtonActivated = !gButtonActivated;
        CheckItem(StateMenu(), iButtonState, gButtonActivated);
    } else if (control == gCheckbox) {
        value = GetControlValue(gCheckbox) == 0 ? 1 : 0;
        SetControlValue(gCheckbox, value);
        CheckItem(StateMenu(), iCheckboxState, value != 0);
    } else if (control == gScrollbar) {
        value = GetControlValue(gScrollbar);
        if (part == kControlUpButtonPart || part == kControlPageUpPart) {
            value -= part == kControlPageUpPart ? 3 : 1;
        } else if (part == kControlDownButtonPart || part == kControlPageDownPart) {
            value += part == kControlPageDownPart ? 3 : 1;
        }
        if (value < 0) value = 0;
        if (value > 10) value = 10;
        SetControlValue(gScrollbar, value);
        CheckItem(StateMenu(), iScrollbarState,
                  GetControlValue(gScrollbar) != 0);
    }
    DrawMainWindow();
}

static void DoEvent(EventRecord *event)
{
    WindowPtr window;
    short part;
    char key;

    switch (event->what) {
        case mouseDown:
            part = FindWindow(event->where, &window);
            if (part == inMenuBar) {
                DoMenuChoice(MenuSelect(event->where));
            } else if (part == inContent) {
                if (window != FrontWindow()) {
                    SelectWindow(window);
                } else {
                    DoContentClick(window, event->where);
                }
            } else if (part == inDrag) {
                DragWindow(window, event->where, &qd.screenBits.bounds);
            } else if (part == inGoAway && TrackGoAway(window, event->where)) {
                if (window == gAuxWindow) {
                    DisposeWindow(gAuxWindow);
                    gAuxWindow = nil;
                    CheckItem(StateMenu(), iWindowState, false);
                } else {
                    gQuit = true;
                }
            }
            break;

        case keyDown:
        case autoKey:
            key = (char)(event->message & charCodeMask);
            if ((event->modifiers & cmdKey) != 0) {
                DoMenuChoice(MenuKey(key));
            }
            break;

        case updateEvt:
            window = (WindowPtr)event->message;
            BeginUpdate(window);
            if (window == gMainWindow) {
                DrawMainWindow();
            } else if (window == gAuxWindow) {
                DrawAuxWindow();
            }
            EndUpdate(window);
            break;
    }
}

void main(void)
{
    EventRecord event;

    Initialize();
    while (!gQuit) {
        if (WaitNextEvent(everyEvent, &event, 1, nil)) {
            DoEvent(&event);
        }
    }
    ExitToShell();
}
