/*
 * Toolbox Showcase: Classic Macintosh Fat-App Fixture.
 * Compiled unchanged for 68K and native PowerPC.
 *
 * Exercises standard Macintosh Toolbox subsystems:
 * - Window Manager & Event Manager (Macintosh Toolbox Essentials ch 2, 4)
 * - Menu Manager & Hierarchical Submenus (Macintosh Toolbox Essentials ch 3)
 * - List Manager, default text lists, selection, scrolling, and resizing
 *   (More Macintosh Toolbox ch 4)
 * - Resource Manager enumeration, named lookup, deferred loading, release,
 *   and reload behavior
 *   (Inside Macintosh: More Macintosh Toolbox ch 1)
 * - Styled TextEdit runs, Font Manager lookup, and text measurement
 *   (Inside Macintosh: Text ch 2, 3, and 4)
 * - Standard File Open/Save dialogs, file-type filtering, navigation,
 *   cancellation, editable names, and returned FSSpec/SFReply records
 *   (Inside Macintosh: Files ch 3)
 * - Control Manager & Standard CDEFs (Macintosh Toolbox Essentials ch 5)
 * - Dialog Manager & Alerts (Macintosh Toolbox Essentials ch 6)
 * - Sound Manager channels, sampled playback & command queues
 *   (Inside Macintosh: Sound ch 2)
 * - Palette Manager activation, indexed drawing & animation
 *   (Inside Macintosh Volume VI ch 20)
 * - QuickDraw Geometry, Arcs, Polygons, Regions, PICT, Icons & 3D Bevels
 *   (Imaging With QuickDraw ch 3, 4, 7, 8)
 */

#include <Controls.h>
#include <ControlDefinitions.h>
#include <Dialogs.h>
#include <Events.h>
#include <Files.h>
#include <Fonts.h>
#include <Memory.h>
#include <Menus.h>
#include <Lists.h>
#include <OSUtils.h>
#include <Palettes.h>
#include <QDOffscreen.h>
#include <Quickdraw.h>
#include <Resources.h>
#include <Sound.h>
#include <StandardFile.h>
#include <TextEdit.h>
#include <ToolUtils.h>
#include <Windows.h>

#if defined(__POWERPC__) || defined(powerc) || defined(PowerPC) || defined(__powerc)
#define SHOWCASE_TARGET_PPC 1
#endif

#ifdef SHOWCASE_TARGET_PPC
#include <QD3D.h>
#include <QD3DCamera.h>
#include <QD3DDrawContext.h>
#include <QD3DGeometry.h>
#include <QD3DGroup.h>
#include <QD3DLight.h>
#include <QD3DRenderer.h>
#include <QD3DShader.h>
#include <QD3DStyle.h>
#include <QD3DTransform.h>
#include <QD3DSet.h>
#endif

#define rMenuBar 128
#define rMainWindow 128
#define rPrefDialog 129
#define rAboutAlert 130
#define rShowcaseIcon 128
#define rShowcasePalette 150
#define rShowcaseSound 151

#define mApple 128
#define mPages 129
#define mState 130
#define mFile 131
#define mOptions 132

#define mDifficulty 140
#define mSoundMenu 141
#define mRendererMenu 142

/* Pages menu items */
#define iGraphics 1
#define iControls 2
#define iWindows 3
#define iDrawing 4
#define iPreferences 5
#define iDialogs 6
#define iTextEdit 7
#define iPalettes 8
#define iLists 9
#define iSound 10
#define iStyledText 11
#define iStandardFile 12
#define iResources 13
#define iSprites 14
#define iEventsCursors 15

/* State menu items */
#define iButtonState 1
#define iCheckboxState 2
#define iScrollbarState 3
#define iWindowState 4
#define iSoundState 5

/* Options menu items */
#define iOptDifficulty 1
#define iOptSound 2
#define iOptRenderer 3
#define iOptResetPrefs 5
#define iOptLaunchDialog 6

/* Difficulty submenu items */
#define iDiffEasy 1
#define iDiffNormal 2
#define iDiffHard 3

/* Sound submenu items */
#define iSndMute 1
#define iSndFXOnly 2
#define iSndMusicOnly 3
#define iSndFull 4

/* Renderer submenu items */
#define iRendFlat 1
#define iRendBevel 2
#define iRendContrast 3

/* File menu items */
#define iFilePrefs 1
#define iQuit 4

/* Apple menu items */
#define iAbout 1

/* Page indices */
#define pageGraphics 1
#define pageControls 2
#define pageWindows 3
#define pageDrawing 4
#define pagePreferences 5
#define pageDialogs 6
#define pageTextEdit 7
#define pagePalettes 8
#define pageLists 9
#define pageSound 10
#define pageStyledText 11
#define pageStandardFile 12
#define pageResources 13
#define pageSprites 14
#define pageEventsCursors 15

#define kResourceBrowserRows 3
#define resourceStatusEnumerated 1
#define resourceStatusLoaded 2
#define resourceStatusReleased 3
#define resourceStatusError 4

#define kInventoryRowCount 12
#define listStatusNone 0
#define listStatusSelected 1
#define listStatusMutated 2
#define listStatusScrolled 3
#define listStatusResized 4
#define listStatusInactive 5
#define listStatusActivated 6

static QDGlobals qd;
static WindowPtr gMainWindow;
static WindowPtr gAuxWindow;
static WindowPtr gStackWindow;

/* Page 2: Controls */
static ControlHandle gButton;
static ControlHandle gCheckbox;
static ControlHandle gScrollbar;

/* Page 5: Game Preferences Controls */
static ControlHandle gPrefSndFX;
static ControlHandle gPrefMusic;
static ControlHandle gPrefVolume;
static ControlHandle gPrefDiffEasy;
static ControlHandle gPrefDiffNormal;
static ControlHandle gPrefDiffHard;
static ControlHandle gPrefRendFlat;
static ControlHandle gPrefRendBevel;
static ControlHandle gPrefRendContrast;
static ControlHandle gPrefBtnApply;
static ControlHandle gPrefBtnReset;
static ControlHandle gPrefBtnModal;

/* Page 6: Dialogs Controls */
static ControlHandle gDlgBtnOpenPrefs;
static ControlHandle gDlgBtnOpenAlert;

/* Page 7: Palette Manager */
static ControlHandle gPaletteAnimate;
static PaletteHandle gShowcasePalette;
static PaletteHandle gOriginalPalette;
static Boolean gPaletteAnimated = false;

/* Page 8: TextEdit */
static TEHandle gTE;
static Rect gTERect;
static short gTEJust = teJustLeft;
static ControlHandle gTEJustLeft;
static ControlHandle gTEJustCenter;
static ControlHandle gTEJustRight;
static ControlHandle gTEBtnCut;
static ControlHandle gTEBtnCopy;
static ControlHandle gTEBtnPaste;
static ControlHandle gTEBtnReset;

/* Page 9: Lists & Inventory */
static ListHandle gInventoryList;
static Rect gInventoryView;
static ControlHandle gListInspect;
static ControlHandle gListMutate;
static ControlHandle gListScroll;
static ControlHandle gListResize;
static ControlHandle gListActivate;
static short gListStatus;
static short gListSelectedRow = -1;
static short gListSelectedLength;
static Boolean gListActive = true;
static Boolean gListResized = false;

/* Page 15: Event Manager and cursor probes */
static Rect gEventProbeRect;
static Rect gEventCrossRect;
static Rect gEventWatchRect;
static Rect gEventArrowRect;
static Rect gEventHideRect;
static Rect gEventShowRect;
static short gEventLastWhat = nullEvent;
static long gEventLastMessage;
static unsigned long gEventLastWhen;
static Point gEventLastWhere;
static short gEventLastModifiers;
static short gEventMouseV;
static short gEventMouseH;
static Boolean gEventButtonDown;
static Boolean gEventStillDown;
static Boolean gEventWaitMouseUp;
static Boolean gEventKeysDown;
static Boolean gEventUpdateSeen;
static Boolean gEventActivateSeen;
static Boolean gEventActive;
static short gEventMouseDownCount;
static short gEventMouseUpCount;
static short gEventKeyDownCount;
static short gEventCursorMode;
static Boolean gEventCursorHidden;
static OSErr gEventPostResult;
static Boolean gEventPeeked;
static Boolean gEventOSEventPeeked;
static Boolean gEventTaken;
static short gEventPeekWhat;
static short gEventOSEventPeekWhat;
static short gEventTakenWhat;

/* Page 10: Sound Manager */
static SndChannelPtr gShowcaseSoundChannel;
static Handle gShowcaseSound;
static SndCallBackUPP gShowcaseSoundCallbackUPP;
static SCStatus gShowcaseSoundStatus;
static ControlHandle gSoundBtnBeep;
static ControlHandle gSoundBtnPlay;
static ControlHandle gSoundBtnQueue;
static ControlHandle gSoundBtnFlush;
static ControlHandle gSoundBtnQuiet;
static ControlHandle gSoundBtnComplete;
static ControlHandle gSoundBtnDispose;
static Boolean gSoundBeeped = false;
static Boolean gSoundStarted = false;
static Boolean gSoundQueued = false;
static Boolean gSoundFlushed = false;
static Boolean gSoundQuieted = false;
static volatile Boolean gSoundCompleted = false;
static Boolean gSoundCompletionPresented = false;
static Boolean gSoundDisposed = true;
static Boolean gSoundResourceLocked = false;
static OSErr gSoundLastError = noErr;
static OSErr gSoundStatusError = noErr;

static void SyncMenuState(void);
static MenuHandle StateMenu(void);
static void DrawMainWindow(void);

/*
 * Sound Manager callbacks run at interrupt time. Store the completion flag
 * address in the channel's application-owned userInfo field so the callback
 * does not need to dereference the application's A5 world. Sound (1994),
 * pp. 2-46--2-48 and 2-151--2-152.
 */
static pascal void ShowcaseSoundCallback(SndChannelPtr chan, SndCommand *cmd)
{
    (void)cmd;
    if (chan != nil && chan->userInfo != 0) {
        *((volatile Boolean *)chan->userInfo) = true;
    }
}

static void MakeShowcaseSoundCommand(SndCommand *cmd, short command, long param2)
{
    cmd->cmd = command;
    cmd->param1 = 0;
    cmd->param2 = param2;
}

static void EnsureShowcaseSoundChannel(void)
{
    if (gShowcaseSoundChannel != nil) return;
    if (gShowcaseSound == nil) {
        gShowcaseSound = GetResource('snd ', rShowcaseSound);
    }
    if (gShowcaseSound == nil) {
        gSoundLastError = resNotFound;
        return;
    }

    if (gShowcaseSoundCallbackUPP == nil) {
        gShowcaseSoundCallbackUPP = NewSndCallBackUPP(ShowcaseSoundCallback);
    }
    gSoundLastError = SndNewChannel(&gShowcaseSoundChannel, sampledSynth,
                                    initMono, gShowcaseSoundCallbackUPP);
    if (gSoundLastError == noErr && gShowcaseSoundChannel != nil) {
        gShowcaseSoundChannel->userInfo = (long)&gSoundCompleted;
        gSoundDisposed = false;
    } else if (gShowcaseSoundCallbackUPP != nil) {
        DisposeSndCallBackUPP(gShowcaseSoundCallbackUPP);
        gShowcaseSoundCallbackUPP = nil;
    }
}

static void ResetShowcaseSoundChannel(void)
{
    SndCommand cmd;

    if (gShowcaseSoundChannel == nil) return;
    MakeShowcaseSoundCommand(&cmd, quietCmd, 0);
    SndDoImmediate(gShowcaseSoundChannel, &cmd);
    MakeShowcaseSoundCommand(&cmd, flushCmd, 0);
    SndDoImmediate(gShowcaseSoundChannel, &cmd);
}

static void PlayShowcaseSound(Boolean queueCompletion)
{
    SndCommand cmd;
    Boolean lockedHere;

    EnsureShowcaseSoundChannel();
    if (gShowcaseSoundChannel == nil || gShowcaseSound == nil) return;

    ResetShowcaseSoundChannel();
    gSoundStarted = false;
    gSoundQueued = false;
    gSoundFlushed = false;
    gSoundQuieted = false;
    gSoundCompleted = false;
    gSoundCompletionPresented = false;
    gSoundDisposed = false;
    SyncMenuState();

    lockedHere = false;
    if (!gSoundResourceLocked) {
        HLock(gShowcaseSound);
        gSoundResourceLocked = true;
        lockedHere = true;
    }
    gSoundLastError = SndPlay(gShowcaseSoundChannel,
                               (SndListHandle)gShowcaseSound, true);
    if (gSoundLastError != noErr) {
        if (lockedHere) {
            HUnlock(gShowcaseSound);
            gSoundResourceLocked = false;
        }
        return;
    }
    gSoundStarted = true;

    if (queueCompletion) {
        MakeShowcaseSoundCommand(&cmd, volumeCmd, 0x00c000c0);
        gSoundLastError = SndDoCommand(gShowcaseSoundChannel, &cmd, true);
        if (gSoundLastError == noErr) {
            MakeShowcaseSoundCommand(&cmd, callBackCmd, 0);
            gSoundLastError = SndDoCommand(gShowcaseSoundChannel, &cmd, true);
            gSoundQueued = gSoundLastError == noErr;
        }
    }
}

static void QueueShowcaseSoundCommands(void)
{
    SndCommand cmd;

    if (gShowcaseSoundChannel == nil) return;
    MakeShowcaseSoundCommand(&cmd, volumeCmd, 0x00800080);
    gSoundLastError = SndDoCommand(gShowcaseSoundChannel, &cmd, true);
    if (gSoundLastError == noErr) {
        MakeShowcaseSoundCommand(&cmd, callBackCmd, 0);
        gSoundLastError = SndDoCommand(gShowcaseSoundChannel, &cmd, true);
        gSoundQueued = gSoundLastError == noErr;
    }
}

static void FlushShowcaseSound(void)
{
    SndCommand cmd;

    if (gShowcaseSoundChannel == nil) return;
    MakeShowcaseSoundCommand(&cmd, flushCmd, 0);
    gSoundLastError = SndDoImmediate(gShowcaseSoundChannel, &cmd);
    gSoundFlushed = gSoundLastError == noErr;
}

static void QuietShowcaseSound(void)
{
    SndCommand cmd;

    if (gShowcaseSoundChannel == nil) return;
    MakeShowcaseSoundCommand(&cmd, quietCmd, 0);
    gSoundLastError = SndDoImmediate(gShowcaseSoundChannel, &cmd);
    gSoundQuieted = gSoundLastError == noErr;
}

static void DisposeShowcaseSoundChannel(void)
{
    if (gShowcaseSoundChannel == nil) {
        gSoundDisposed = true;
        if (gShowcaseSoundCallbackUPP != nil) {
            DisposeSndCallBackUPP(gShowcaseSoundCallbackUPP);
            gShowcaseSoundCallbackUPP = nil;
        }
        if (gSoundResourceLocked && gShowcaseSound != nil) {
            HUnlock(gShowcaseSound);
            gSoundResourceLocked = false;
        }
        SyncMenuState();
        return;
    }

    ResetShowcaseSoundChannel();
    gSoundLastError = SndDisposeChannel(gShowcaseSoundChannel, true);
    gShowcaseSoundChannel = nil;
    gSoundDisposed = true;
    if (gShowcaseSoundCallbackUPP != nil) {
        DisposeSndCallBackUPP(gShowcaseSoundCallbackUPP);
        gShowcaseSoundCallbackUPP = nil;
    }
    if (gSoundResourceLocked && gShowcaseSound != nil) {
        HUnlock(gShowcaseSound);
        gSoundResourceLocked = false;
    }
    SyncMenuState();
}

static void RefreshShowcaseSoundStatus(void)
{
    if (gShowcaseSoundChannel == nil) {
        gSoundStatusError = noErr;
        gShowcaseSoundStatus.scChannelBusy = false;
        gShowcaseSoundStatus.scChannelPaused = false;
        return;
    }
    gSoundStatusError = SndChannelStatus(gShowcaseSoundChannel,
                                         sizeof(SCStatus),
                                         &gShowcaseSoundStatus);
}

static void PollShowcaseSound(void)
{
    RefreshShowcaseSoundStatus();
    CheckItem(StateMenu(), iSoundState, gSoundCompleted);
    if (gSoundCompleted && !gSoundCompletionPresented) {
        gSoundCompletionPresented = true;
        DrawMainWindow();
    }
}

/* Page 11: Styled TextEdit & Font Manager */
static TEHandle gStyledTE;
static Rect gStyledTERect;
static short gStyledGenevaFont;
static short gStyledMonacoFont;
static Boolean gStyledGenevaReal;
static Boolean gStyledMonacoReal;
static short gStyledInspectMode;
static Boolean gStyledInspectResult;
static TextStyle gStyledInspectStyle;
static short gStyledRunMode;
static Boolean gStyledRunResult;
static TextStyle gStyledRunStyle;
static short gStyledCharWidth;
static short gStyledTextWidth;
static short gStyledMeasureWidth;

/* Page 12: Standard File */
#define fileStatusNone 0
#define fileStatusAccepted 1
#define fileStatusCancelled 2
#define fileStatusError 3

static ControlHandle gFileOpen;
static ControlHandle gFileLegacyOpen;
static ControlHandle gFileSave;
static ControlHandle gFileLegacySave;
static StandardFileReply gFileOpenReply;
static StandardFileReply gFileSaveReply;
static SFReply gFileLegacyOpenReply;
static SFReply gFileLegacySaveReply;
static short gFileOpenStatus;
static short gFileLegacyOpenStatus;
static short gFileSaveStatus;
static short gFileLegacySaveStatus;
static SFTypeList gFileTypeList;

/* Page 13: Resource Browser */
typedef struct {
    Handle handle;
    short id;
    ResType type;
    Str255 name;
    short attrs;
    long size;
    Boolean loaded;
    Boolean present;
} ResourceBrowserEntry;

static ControlHandle gResourceRefresh;
static ControlHandle gResourceLoad;
static ControlHandle gResourceRelease;
static ResourceBrowserEntry gResourceEntries[kResourceBrowserRows];
static short gResourceCount;
static short gResourceMenuCount;
static short gResourceWindowCount;
static Handle gResourceEditHandle;
static short gResourceStatus;
static short gResourceError;
static Boolean gResourceBrowserReady = false;

/* Page 14: Sprites, Masks & Scrolling */
#define kSpriteWorldWidth 320
#define kSpriteWorldHeight 128
#define kSpriteSize 48
static GWorldPtr gSpriteWorld;
static GWorldPtr gSpriteSource;
static GWorldPtr gSpriteMask;
static GWorldPtr gSpriteDeepMask;
static RgnHandle gSpriteRegion;
static RgnHandle gSpriteUpdateRegion;
static ControlHandle gSpriteAnimate;
static ControlHandle gSpriteScroll;
static ControlHandle gSpriteReset;
static Boolean gSpriteReady = false;
static Boolean gSpriteAnimated = false;
static Boolean gSpriteScrolled = false;
static Boolean gSpritePixelVerified = false;
static Boolean gSpriteRegionVerified = false;
static Boolean gSpriteUpdateRegionVerified = false;
static OSErr gSpriteRegionError = noErr;
static short gSpriteScrollDelta;

static const char kTESampleText[] =
    "TextEdit manages styled and plain text formatting, automatic word wrapping, "
    "selection highlighting, and clipboard scrap operations.\r\r"
    "Click to move the insertion point or drag across characters to select text.";

static const char kTECalloutText[] =
    "TETextBox renders transient wrapped paragraphs with specified justification.";

static const char kStyledSampleText[] =
    "Plain  Bold  Color  Size  Font";

static const char *kInventoryItems[kInventoryRowCount] = {
    "Phase Shifter       01  equipped",
    "Medkit              03  ready",
    "Plasma Cartridge    12  ready",
    "Shield Cell         04  reserve",
    "Navigation Chip     01  installed",
    "Repair Nanobots     06  reserve",
    "Fuel Rod            02  reserve",
    "Signal Beacon       01  ready",
    "Star Map             02  archive",
    "Alien Artifact      01  sealed",
    "Quantum Key         01  secured",
    "Mission Log          04  archive"
};

/*
 * Record an indexed PixMap into a PICT, replay it through a canonical
 * offscreen GWorld, then copy those pixels into the active screen palette.
 * The picture's 64 populated colors deliberately do not share device indexes
 * with the destination. DrawPicture must color-match them instead of treating
 * the PICT's raw indexes as screen indexes. Imaging With QuickDraw (1994),
 * pp. 4-13 and 7-14; CopyBits palette translation, pp. 7-24..7-28.
 */
static void DrawIndexedPictureTransfer(void)
{
    GWorldPtr sourceWorld;
    GWorldPtr replayWorld;
    GWorldPtr savedWorld;
    GDHandle savedDevice;
    PixMapHandle sourcePixels;
    PixMapHandle replayPixels;
    PixMapHandle windowPixels;
    CTabHandle sourceColors;
    PicHandle picture;
    Rect localRect;
    Rect displayRect;
    Ptr base;
    long rowBytes;
    short x;
    short y;
    short i;
    sourceWorld = nil;
    replayWorld = nil;
    picture = nil;
    sourceColors = nil;
    SetRect(&localRect, 0, 0, 192, 18);

    GetGWorld(&savedWorld, &savedDevice);
    if (NewGWorld(&sourceWorld, 8, &localRect, nil, nil, 0) != noErr) goto cleanup;
    if (NewGWorld(&replayWorld, 8, &localRect, nil, nil, 0) != noErr) goto cleanup;

    sourcePixels = GetGWorldPixMap(sourceWorld);
    replayPixels = GetGWorldPixMap(replayWorld);
    windowPixels = GetGWorldPixMap((GWorldPtr)gMainWindow);
    if (windowPixels == nil) goto cleanup;
    sourceColors = (**sourcePixels).pmTable;
    if (sourceColors == nil) goto cleanup;
    for (i = 0; i < 64; i++) {
        (**sourceColors).ctTable[i].value = i;
        (**sourceColors).ctTable[i].rgb.red = (unsigned short)(0x2200 + i * 0x02c0);
        (**sourceColors).ctTable[i].rgb.green = (unsigned short)(0xee00 - i * 0x0240);
        (**sourceColors).ctTable[i].rgb.blue = (unsigned short)(0x3300 + i * 0x0180);
    }
    CTabChanged(sourceColors);
    if (!LockPixels(sourcePixels) || !LockPixels(replayPixels)) goto cleanup;

    base = GetPixBaseAddr(sourcePixels);
    rowBytes = (**sourcePixels).rowBytes & 0x3fff;
    for (y = 0; y < 18; y++) {
        for (x = 0; x < 192; x++) {
            base[y * rowBytes + x] = (char)(1 + (x / 3));
        }
    }

    SetGWorld(replayWorld, nil);
    picture = OpenPicture(&localRect);
    CopyBits((BitMap *)*sourcePixels, (BitMap *)*replayPixels,
             &localRect, &localRect, srcCopy, nil);
    ClosePicture();

    DrawPicture(picture, &localRect);

    SetGWorld((GWorldPtr)gMainWindow, savedDevice);
    SetRect(&displayRect, 330, 272, 522, 290);
    CopyBits((BitMap *)*replayPixels, (BitMap *)*windowPixels,
             &localRect, &displayRect, srcCopy, nil);
    FrameRect(&displayRect);

cleanup:
    SetGWorld(savedWorld, savedDevice);
    if (picture != nil) KillPicture(picture);
    if (sourceWorld != nil) DisposeGWorld(sourceWorld);
    if (replayWorld != nil) DisposeGWorld(replayWorld);
}

/*
 * Copy already-authored indexes from an offscreen GDevice whose ColorTable
 * identifies the destination device, but whose RGB entries have been
 * transiently cleared without changing the table seed. Device ColorTables
 * assign colors by position, so CopyBits must preserve these indexes instead
 * of matching their temporary black RGB values into the screen table. Inside
 * Macintosh Volume V (1986), pp. V-57..V-58 and V-138..V-145; Imaging With
 * QuickDraw (1994), pp. 6-16..6-21 and 7-24..7-28.
 */
static void DrawSameDeviceIndexedTransfer(void)
{
    GWorldPtr sourceWorld;
    GDHandle screenDevice;
    PixMapHandle sourcePixels;
    PixMapHandle windowPixels;
    CTabHandle screenColors;
    CTabHandle sourceColors;
    Rect localRect;
    Rect displayRect;
    Ptr base;
    long rowBytes;
    RGBColor color;
    short x;
    short y;

    sourceWorld = nil;
    screenDevice = GetGDevice();
    if (screenDevice == nil) return;
    if ((**(**screenDevice).gdPMap).pixelSize != 8) {
        for (x = 0; x < 3; x++) {
            if (x == 0) {
                color.red = 0xffff; color.green = 0xffff; color.blue = 0;
            } else if (x == 1) {
                color.red = 0xffff; color.green = 0x6666; color.blue = 0;
            } else {
                color.red = 0; color.green = 0xcccc; color.blue = 0xaaaa;
            }
            RGBForeColor(&color);
            SetRect(&displayRect, 330 + x * 64, 302,
                    394 + x * 64, 320);
            PaintRect(&displayRect);
        }
        color.red = color.green = color.blue = 0;
        RGBForeColor(&color);
        SetRect(&displayRect, 330, 302, 522, 320);
        FrameRect(&displayRect);
        return;
    }
    screenColors = (**(**screenDevice).gdPMap).pmTable;
    if (screenColors == nil) return;

    SetRect(&localRect, 0, 0, 192, 18);
    if (NewGWorld(&sourceWorld, 8, &localRect, nil, nil, 0) != noErr) return;

    sourcePixels = GetGWorldPixMap(sourceWorld);
    windowPixels = GetGWorldPixMap((GWorldPtr)gMainWindow);
    if (sourcePixels == nil || windowPixels == nil) goto cleanup;
    sourceColors = (**sourcePixels).pmTable;
    if (sourceColors == nil || !LockPixels(sourcePixels)) goto cleanup;

    base = GetPixBaseAddr(sourcePixels);
    rowBytes = (**sourcePixels).rowBytes & 0x3fff;
    for (y = 0; y < 18; y++) {
        for (x = 0; x < 192; x++) {
            base[y * rowBytes + x] = (char)(2 + (x / 64));
        }
    }

    (**sourceColors).ctSeed = (**screenColors).ctSeed;
    (**sourceColors).ctFlags |= 0x8000;
    for (x = 2; x <= 4; x++) {
        (**sourceColors).ctTable[x].rgb.red = 0;
        (**sourceColors).ctTable[x].rgb.green = 0;
        (**sourceColors).ctTable[x].rgb.blue = 0;
    }

    SetRect(&displayRect, 330, 302, 522, 320);
    CopyBits((BitMap *)*sourcePixels, (BitMap *)*windowPixels,
             &localRect, &displayRect, srcCopy, nil);
    FrameRect(&displayRect);

cleanup:
    if (sourceWorld != nil) DisposeGWorld(sourceWorld);
}

/* State variables */
static short gPage = pageGraphics;
static Boolean gQuit = false;
static Boolean gButtonActivated = false;

/* Preferences state */
static short gDifficulty = iDiffNormal;
static Boolean gSoundFX = true;
static Boolean gMusic = true;
static short gVolume = 75;
static short gRenderer = iRendBevel;
static Boolean gModalDialogCompleted = false;

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

/*
 * QuickDraw 3D-style Beveled Treatment (Emulated QuickDraw)
 * Renders raised 3D panels and sunken wells using dual-tone light/shadow borders.
 */
static void DrawBeveledBox(const Rect *r, Boolean sunken)
{
    RGBColor white;
    RGBColor lightGray;
    RGBColor midGray;
    RGBColor darkGray;
    RGBColor black;
    Rect inner;

    white.red = 0xffff; white.green = 0xffff; white.blue = 0xffff;
    lightGray.red = 0xeeee; lightGray.green = 0xeeee; lightGray.blue = 0xeeee;
    midGray.red = 0xcccc; midGray.green = 0xcccc; midGray.blue = 0xcccc;
    darkGray.red = 0x6666; darkGray.green = 0x6666; darkGray.blue = 0x6666;
    black.red = 0x0000; black.green = 0x0000; black.blue = 0x0000;

    RGBForeColor(&black);
    FrameRect(r);

    inner = *r;
    InsetRect(&inner, 1, 1);

    if (!sunken) {
        /* Raised 3D Panel: White top/left, Dark bottom/right */
        RGBForeColor(&white);
        MoveTo(inner.left, inner.bottom - 1);
        LineTo(inner.left, inner.top);
        LineTo(inner.right - 1, inner.top);

        RGBForeColor(&darkGray);
        MoveTo(inner.right - 1, inner.top + 1);
        LineTo(inner.right - 1, inner.bottom - 1);
        LineTo(inner.left + 1, inner.bottom - 1);

        InsetRect(&inner, 1, 1);
        RGBForeColor(&lightGray);
        PaintRect(&inner);
    } else {
        /* Sunken 3D Well: Dark top/left, White bottom/right */
        RGBForeColor(&darkGray);
        MoveTo(inner.left, inner.bottom - 1);
        LineTo(inner.left, inner.top);
        LineTo(inner.right - 1, inner.top);

        RGBForeColor(&white);
        MoveTo(inner.right - 1, inner.top + 1);
        LineTo(inner.right - 1, inner.bottom - 1);
        LineTo(inner.left + 1, inner.bottom - 1);

        InsetRect(&inner, 1, 1);
        RGBForeColor(&midGray);
        PaintRect(&inner);
    }
    RGBForeColor(&black);
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
    SetRect(&clip, 405, 35, 486, 156);
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
    DrawHeading("\pWindow stacking and update events");
    MoveTo(24, 70);
    DrawString("\pThree overlapping documents exercise z-order and repaint.");
    MoveTo(24, 92);
    DrawString("\pMove, resize, activate, and close the front window.");
}

#ifdef SHOWCASE_TARGET_PPC
/*
 * Native PowerPC QuickDraw 3D Rendering Pipeline.
 * Exercises real QuickDraw3DLib lifecycle:
 * - Initialization & Teardown (Q3Initialize, Q3Exit)
 * - Macintosh Draw Context with Pane Viewport (Q3MacDrawContext_New)
 * - View Angle Aspect Perspective Camera (Q3ViewAngleAspectCamera_New)
 * - Interactive Renderer (Q3Renderer_NewFromType)
 * - Multi-source Lighting: Ambient & Directional (Q3LightGroup_New, Q3AmbientLight_New, Q3DirectionalLight_New)
 * - Material Attribute Set with Diffuse Color (Q3AttributeSet_New, Q3AttributeSet_Add)
 * - 3D Geometry: TriMesh model with vertex points & triangle indexing (Q3TriMesh_New)
 * - Phong Illumination Shader (Q3PhongIllumination_New)
 * - Rendering traversal loop (Q3View_StartRendering, Q3InterpolationStyle_Submit, Q3BackfacingStyle_Submit, Q3FillStyle_Submit, Q3Shader_Submit, Q3TriMesh_Submit, Q3View_EndRendering)
 * - Comprehensive resource disposal (Q3Object_Dispose)
 */
static void RenderQD3DScene(WindowPtr window)
{
    TQ3DrawContextObject drawContext = nil;
    TQ3CameraObject camera = nil;
    TQ3RendererObject renderer = nil;
    TQ3GroupObject lightGroup = nil;
    TQ3LightObject ambientLight = nil;
    TQ3LightObject dirLight = nil;
    TQ3ViewObject view = nil;
    TQ3ShaderObject shader = nil;
    TQ3AttributeSet attrSet = nil;
    TQ3GeometryObject geom = nil;
    TQ3MacDrawContextData macDCData;
    TQ3ViewAngleAspectCameraData cameraData;
    TQ3LightData ambData;
    TQ3DirectionalLightData dirData;
    TQ3ColorRGB diffColor;
    TQ3TriMeshData triMeshData;
    TQ3Point3D points[4];
    TQ3TriMeshTriangleData triangles[4];
    TQ3ViewStatus viewStatus;

    if (Q3Initialize() != kQ3Success) {
        return;
    }

    /* Bounded 3D Viewport Pane within Section 1 of Drawing Page: local (30, 80, 125, 130) */
    macDCData.drawContextData.clearImageMethod = kQ3ClearMethodWithColor;
    macDCData.drawContextData.clearImageColor.a = 1.0f;
    macDCData.drawContextData.clearImageColor.r = 0.12f;
    macDCData.drawContextData.clearImageColor.g = 0.16f;
    macDCData.drawContextData.clearImageColor.b = 0.28f;
    macDCData.drawContextData.pane.min.x = 30.0f;
    macDCData.drawContextData.pane.min.y = 80.0f;
    macDCData.drawContextData.pane.max.x = 125.0f;
    macDCData.drawContextData.pane.max.y = 130.0f;
    macDCData.drawContextData.paneState = kQ3True;
    macDCData.drawContextData.mask.image = nil;
    macDCData.drawContextData.mask.width = 0;
    macDCData.drawContextData.mask.height = 0;
    macDCData.drawContextData.mask.rowBytes = 0;
    macDCData.drawContextData.mask.bitOrder = kQ3EndianBig;
    macDCData.drawContextData.maskState = kQ3False;
    macDCData.drawContextData.doubleBufferState = kQ3False;
    macDCData.window = (CWindowPtr)window;
    macDCData.library = kQ3Mac2DLibraryNone;
    macDCData.viewPort = nil;
    macDCData.grafPort = (CGrafPtr)window;

    drawContext = Q3MacDrawContext_New(&macDCData);

    cameraData.cameraData.placement.cameraLocation.x = 0.0f;
    cameraData.cameraData.placement.cameraLocation.y = 0.35f;
    cameraData.cameraData.placement.cameraLocation.z = 2.4f;
    cameraData.cameraData.placement.pointOfInterest.x = 0.0f;
    cameraData.cameraData.placement.pointOfInterest.y = 0.0f;
    cameraData.cameraData.placement.pointOfInterest.z = 0.0f;
    cameraData.cameraData.placement.upVector.x = 0.0f;
    cameraData.cameraData.placement.upVector.y = 1.0f;
    cameraData.cameraData.placement.upVector.z = 0.0f;
    cameraData.cameraData.range.hither = 0.1f;
    cameraData.cameraData.range.yon = 100.0f;
    cameraData.cameraData.viewPort.origin.x = -1.0f;
    cameraData.cameraData.viewPort.origin.y = 1.0f;
    cameraData.cameraData.viewPort.width = 2.0f;
    cameraData.cameraData.viewPort.height = 2.0f;
    cameraData.fov = 0.85f;
    cameraData.aspectRatioXToY = 1.0f;

    camera = Q3ViewAngleAspectCamera_New(&cameraData);

    renderer = Q3Renderer_NewFromType(kQ3RendererTypeInteractive);

    lightGroup = Q3LightGroup_New();
    if (lightGroup != nil) {
        ambData.isOn = kQ3True;
        ambData.brightness = 0.35f;
        ambData.color.r = 1.0f;
        ambData.color.g = 1.0f;
        ambData.color.b = 1.0f;
        ambientLight = Q3AmbientLight_New(&ambData);
        if (ambientLight != nil) {
            Q3Group_AddObject(lightGroup, ambientLight);
            Q3Object_Dispose(ambientLight);
        }

        dirData.lightData.isOn = kQ3True;
        dirData.lightData.brightness = 0.85f;
        dirData.lightData.color.r = 1.0f;
        dirData.lightData.color.g = 1.0f;
        dirData.lightData.color.b = 1.0f;
        dirData.castsShadows = kQ3False;
        dirData.direction.x = 1.0f;
        dirData.direction.y = -1.0f;
        dirData.direction.z = -1.0f;
        dirLight = Q3DirectionalLight_New(&dirData);
        if (dirLight != nil) {
            Q3Group_AddObject(lightGroup, dirLight);
            Q3Object_Dispose(dirLight);
        }
    }

    view = Q3View_New();
    if (view != nil) {
        if (drawContext != nil) Q3View_SetDrawContext(view, drawContext);
        if (camera != nil) Q3View_SetCamera(view, camera);
        if (renderer != nil) Q3View_SetRenderer(view, renderer);
        if (lightGroup != nil) Q3View_SetLightGroup(view, lightGroup);
    }

    /* 3D Tetrahedron / Pyramid Model */
    points[0].x =  0.0f; points[0].y =  0.55f; points[0].z =  0.0f;
    points[1].x = -0.5f; points[1].y = -0.35f; points[1].z =  0.5f;
    points[2].x =  0.5f; points[2].y = -0.35f; points[2].z =  0.5f;
    points[3].x =  0.0f; points[3].y = -0.35f; points[3].z = -0.5f;

    triangles[0].pointIndices[0] = 0;
    triangles[0].pointIndices[1] = 1;
    triangles[0].pointIndices[2] = 2;

    triangles[1].pointIndices[0] = 0;
    triangles[1].pointIndices[1] = 2;
    triangles[1].pointIndices[2] = 3;

    triangles[2].pointIndices[0] = 0;
    triangles[2].pointIndices[1] = 3;
    triangles[2].pointIndices[2] = 1;

    triangles[3].pointIndices[0] = 1;
    triangles[3].pointIndices[1] = 3;
    triangles[3].pointIndices[2] = 2;

    attrSet = Q3AttributeSet_New();
    if (attrSet != nil) {
        diffColor.r = 0.2f;
        diffColor.g = 0.65f;
        diffColor.b = 0.95f;
        Q3AttributeSet_Add(attrSet, kQ3AttributeTypeDiffuseColor, &diffColor);
    }

    triMeshData.triMeshAttributeSet = attrSet;
    triMeshData.numTriangles = 4;
    triMeshData.triangles = triangles;
    triMeshData.numTriangleAttributeTypes = 0;
    triMeshData.triangleAttributeTypes = nil;
    triMeshData.numEdges = 0;
    triMeshData.edges = nil;
    triMeshData.numEdgeAttributeTypes = 0;
    triMeshData.edgeAttributeTypes = nil;
    triMeshData.numPoints = 4;
    triMeshData.points = points;
    triMeshData.numVertexAttributeTypes = 0;
    triMeshData.vertexAttributeTypes = nil;
    triMeshData.bBox.isEmpty = kQ3False;
    triMeshData.bBox.min.x = -0.5f;
    triMeshData.bBox.min.y = -0.35f;
    triMeshData.bBox.min.z = -0.5f;
    triMeshData.bBox.max.x =  0.5f;
    triMeshData.bBox.max.y =  0.55f;
    triMeshData.bBox.max.z =  0.5f;

    geom = Q3TriMesh_New(&triMeshData);
    shader = Q3PhongIllumination_New();

    if (view != nil && Q3View_StartRendering(view) == kQ3Success) {
        do {
            Q3InterpolationStyle_Submit(kQ3InterpolationStyleVertex, view);
            Q3BackfacingStyle_Submit(kQ3BackfacingStyleBoth, view);
            Q3FillStyle_Submit(kQ3FillStyleFilled, view);
            if (shader != nil) {
                Q3Shader_Submit(shader, view);
            }
            Q3TriMesh_Submit(&triMeshData, view);
            viewStatus = Q3View_EndRendering(view);
        } while (viewStatus == kQ3ViewStatusRetraverse);
    }

    if (geom != nil) Q3Object_Dispose(geom);
    if (shader != nil) Q3Object_Dispose(shader);
    if (attrSet != nil) Q3Object_Dispose(attrSet);
    if (view != nil) Q3Object_Dispose(view);
    if (lightGroup != nil) Q3Object_Dispose(lightGroup);
    if (renderer != nil) Q3Object_Dispose(renderer);
    if (camera != nil) Q3Object_Dispose(camera);
    if (drawContext != nil) Q3Object_Dispose(drawContext);

    Q3Exit();
}
#endif

/*
 * Page 4: Broader Drawing, QuickDraw 3D, Polygons, Arcs, Regions, PICT & Text
 */
static void DrawDrawingPage(void)
{
    Rect r;
    Rect arcRect;
    Rect subRect;
    Rect picFrame;
    Rect dstRect;
    PolyHandle poly;
    RgnHandle rgnA;
    RgnHandle rgnB;
    RgnHandle rgnCombined;
    PicHandle pic;
    Handle iconH;
    RGBColor color;
    RGBColor white;
    RGBColor darkGray;
    RGBColor black;
    FontInfo fInfo;
    short strW;
    Str255 strBuf;

    white.red = white.green = white.blue = 0xffff;
    darkGray.red = darkGray.green = darkGray.blue = 0x5555;
    black.red = black.green = black.blue = 0x0000;

    /* Section 1: architecture-neutral result after the native QD3D submission. */
    SetRect(&r, 20, 48, 270, 165);
    DrawBeveledBox(&r, false);

#ifdef SHOWCASE_TARGET_PPC
    /* Exercise the native pipeline before painting the shared visible result. */
    RenderQD3DScene(gMainWindow);
    DrawBeveledBox(&r, false);
#endif

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 62);
    DrawString("\pQuickDraw 3D-style Beveled Treatment");
    TextFace(0);
    MoveTo(28, 74);
    DrawString("\p(68K 2D Fallback Representation)");

    /* Metallic horizontal ridges */
    RGBForeColor(&white);
    MoveTo(28, 80); LineTo(262, 80);
    MoveTo(28, 84); LineTo(262, 84);
    RGBForeColor(&darkGray);
    MoveTo(28, 81); LineTo(262, 81);
    MoveTo(28, 85); LineTo(262, 85);

    /* Raised 3D Button */
    SetRect(&subRect, 30, 95, 125, 125);
    DrawBeveledBox(&subRect, false);
    TextFont(applFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(44, 113);
    DrawString("\pEmbossed");
    TextFace(0);

    /* Sunken 3D Gauge Well */
    SetRect(&subRect, 135, 95, 260, 125);
    DrawBeveledBox(&subRect, true);
    color.red = 0x2222; color.green = 0x8888; color.blue = 0x3333;
    RGBForeColor(&color);
    SetRect(&arcRect, 138, 98, 215, 122);
    PaintRect(&arcRect);
    RGBForeColor(&black);
    TextFont(applFont);
    TextSize(9);
    MoveTo(145, 113);
    DrawString("\pGauge: 65%");

    /* Inset chamfer status bar */
    SetRect(&subRect, 30, 134, 260, 155);
    DrawBeveledBox(&subRect, true);
    TextFont(3);
    TextSize(9);
    MoveTo(38, 148);
    DrawString("\pBeveled Facet Lighting (White / Gray)");

    /* Section 2: Polygons & Arcs */
    SetRect(&r, 280, 48, 535, 165);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(288, 62);
    DrawString("\pPolygons and Arcs (QuickDraw Geometry)");
    TextFace(0);

    /* Star Polygon */
    poly = OpenPoly();
    MoveTo(345, 75);
    LineTo(356, 102); LineTo(385, 102); LineTo(362, 118);
    LineTo(371, 148); LineTo(345, 130); LineTo(319, 148);
    LineTo(328, 118); LineTo(305, 102); LineTo(334, 102);
    LineTo(345, 75);
    ClosePoly();

    color.red = 0xeeee; color.green = 0x9999; color.blue = 0x1111;
    RGBForeColor(&color);
    PaintPoly(poly);
    RGBForeColor(&black);
    FramePoly(poly);
    KillPoly(poly);

    /* 3-Sector Pie Chart with Arcs */
    SetRect(&arcRect, 415, 78, 495, 158);
    color.red = 0xdddd; color.green = 0x2222; color.blue = 0x3333;
    RGBForeColor(&color);
    PaintArc(&arcRect, 0, 120);

    color.red = 0x2222; color.green = 0x9999; color.blue = 0x4444;
    RGBForeColor(&color);
    PaintArc(&arcRect, 120, 110);

    color.red = 0x2222; color.green = 0x4444; color.blue = 0xdddd;
    RGBForeColor(&color);
    PaintArc(&arcRect, 230, 130);

    RGBForeColor(&black);
    FrameOval(&arcRect);

    /* Section 3: QuickDraw Regions */
    SetRect(&r, 20, 175, 185, 320);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 190);
    DrawString("\pQuickDraw Regions");
    TextFace(0);

    rgnA = NewRgn();
    rgnB = NewRgn();
    rgnCombined = NewRgn();

    SetRect(&subRect, 30, 205, 105, 275);
    OpenRgn();
    FrameOval(&subRect);
    CloseRgn(rgnA);

    SetRect(&subRect, 65, 225, 140, 290);
    OpenRgn();
    FrameRoundRect(&subRect, 16, 16);
    CloseRgn(rgnB);

    XorRgn(rgnA, rgnB, rgnCombined);
    color.red = 0x3333; color.green = 0x6666; color.blue = 0xbbbb;
    RGBForeColor(&color);
    PaintRgn(rgnCombined);
    RGBForeColor(&black);
    FrameRgn(rgnCombined);

    DisposeRgn(rgnA);
    DisposeRgn(rgnB);
    DisposeRgn(rgnCombined);

    TextFont(3);
    TextSize(9);
    MoveTo(28, 308);
    DrawString("\pXorRgn Oval + RoundRect");

    /* Section 4: Icons & Picture (PICT) Recording */
    SetRect(&r, 195, 175, 360, 320);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(203, 190);
    DrawString("\pIcons & Pictures (PICT)");
    TextFace(0);

    /* Plot standard icon resource */
    SetRect(&subRect, 205, 205, 237, 237);
    iconH = GetIcon(rShowcaseIcon);
    if (iconH != nil) {
        PlotIcon(&subRect, iconH);
        ReleaseResource(iconH);
    }

    /* Dynamic PICT Recording & Playback */
    SetRect(&picFrame, 0, 0, 40, 40);
    pic = OpenPicture(&picFrame);
    color.red = 0xeeee; color.green = 0x7777; color.blue = 0x1111;
    RGBForeColor(&color);
    PaintRoundRect(&picFrame, 10, 10);
    RGBForeColor(&black);
    FrameRoundRect(&picFrame, 10, 10);
    TextFont(applFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(6, 25);
    DrawString("\pPICT");
    TextFace(0);
    ClosePicture();

    /* Draw picture 1:1 and scaled */
    SetRect(&dstRect, 250, 205, 290, 245);
    DrawPicture(pic, &dstRect);
    SetRect(&dstRect, 300, 205, 350, 255);
    DrawPicture(pic, &dstRect);
    KillPicture(pic);

    TextFont(3);
    TextSize(9);
    MoveTo(203, 280);
    DrawString("\pPlotIcon (32x32 bitmap)");
    MoveTo(203, 295);
    DrawString("\pOpenPicture/DrawPicture");

    /* Section 5: Typography & Measurements */
    SetRect(&r, 370, 175, 535, 320);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(378, 190);
    DrawString("\pTypography & Metrics");
    TextFace(0);

    TextFont(systemFont);
    TextSize(12);
    TextFace(bold);
    MoveTo(378, 212);
    DrawString("\pSystem 12pt Bold");

    TextFont(applFont);
    TextSize(9);
    TextFace(0);
    MoveTo(378, 228);
    DrawString("\pGeneva 9pt Plain Text");

    TextFont(4); /* Monaco */
    TextSize(9);
    MoveTo(378, 244);
    DrawString("\pMonaco 9pt Code Font");

    TextFont(applFont);
    TextSize(9);
    TextFace(italic | underline | shadow);
    MoveTo(378, 262);
    DrawString("\pItalic Underline Shadow");
    TextFace(0);

    /* Text measurement */
    GetFontInfo(&fInfo);
    strBuf[0] = 0;
    MoveTo(378, 282);
    TextFont(3);
    TextSize(9);
    DrawString("\pMeasureText: ");
    strW = StringWidth("\pToolbox Showcase");
    NumToString(strW, strBuf);
    DrawString(strBuf);
    DrawString("\ppx");

    /* Ruler line */
    MoveTo(378, 295);
    LineTo(378 + (strW > 140 ? 140 : strW), 295);

    /* Footer note */
    MoveTo(20, 345);
    TextFont(applFont);
    TextSize(9);
    DrawString("\pAll operations executed through standard QuickDraw traps on 68K and PowerPC.");

    /* Draw this last because QuickDraw 3D flushes its pane asynchronously. */
    TextFont(systemFont);
    TextSize(12);
    TextFace(bold);
    MoveTo(70, 34);
    DrawString("\pDrawing: QuickDraw 3D, Polygons, Arcs, Regions, Pictures & Text");
    TextFace(0);
}

/*
 * Page 5: Game Preferences & Settings Panel
 */
static void DrawPreferencesPage(void)
{
    Rect r;
    Rect well;
    Str255 volStr;

    DrawHeading("\pGame Preferences & Configuration Panel");

    /* Audio Settings Group Box */
    SetRect(&r, 20, 48, 230, 160);
    DrawBeveledBox(&r, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 62);
    DrawString("\pAudio & Sound FX");
    TextFace(0);

    /* Difficulty Group Box */
    SetRect(&r, 240, 48, 535, 160);
    DrawBeveledBox(&r, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(248, 62);
    DrawString("\pGameplay Difficulty");
    TextFace(0);

    /* Volume Slider Box */
    SetRect(&r, 20, 170, 230, 255);
    DrawBeveledBox(&r, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 184);
    DrawString("\pMaster Volume: ");
    NumToString(gVolume, volStr);
    DrawString(volStr);
    DrawString("\p%");
    TextFace(0);

    /* Renderer Group Box */
    SetRect(&r, 240, 170, 535, 255);
    DrawBeveledBox(&r, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(248, 184);
    DrawString("\pGraphics Pipeline / Shading Style");
    TextFace(0);

    /* Status Readout Sunken Well */
    SetRect(&well, 20, 265, 535, 305);
    DrawBeveledBox(&well, true);
    TextFont(applFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 282);
    DrawString("\pActive Profile: ");
    TextFace(0);
    if (gDifficulty == iDiffEasy) {
        DrawString("\pRecruit (Easy)");
    } else if (gDifficulty == iDiffHard) {
        DrawString("\pNightmare (Hard)");
    } else {
        DrawString("\pVeteran (Normal)");
    }
    DrawString("\p | Audio: ");
    if (!gSoundFX && !gMusic) {
        DrawString("\pMuted");
    } else if (gSoundFX && !gMusic) {
        DrawString("\pFX Only");
    } else if (!gSoundFX && gMusic) {
        DrawString("\pMusic Only");
    } else {
        DrawString("\pFull (FX+Music)");
    }
    DrawString("\p | Volume: ");
    DrawString(volStr);
    DrawString("\p% | Pipeline: ");
    if (gRenderer == iRendFlat) {
        DrawString("\pFlat 2D");
    } else if (gRenderer == iRendContrast) {
        DrawString("\pHigh Contrast");
    } else {
        DrawString("\pQD3D Bevels");
    }

    MoveTo(28, 297);
    TextFont(3);
    TextSize(9);
    DrawString("\pSettings synchronize bidirectionally with the Options hierarchical menus.");

    DrawControls(gMainWindow);
}

/*
 * Page 6: Dialog Manager, Modal Dialogs & System Alerts
 */
static void DrawDialogsPage(void)
{
    Rect r;
    Rect btnRect;
    Rect editRect;
    RGBColor black;

    black.red = black.green = black.blue = 0x0000;

    DrawHeading("\pDialog Manager: Modal Dialogs & System Alerts");

    TextFont(applFont);
    TextSize(9);
    MoveTo(24, 52);
    DrawString("\pThe Dialog Manager provides standardized modal and modeless user interaction.");
    MoveTo(24, 66);
    DrawString("\pClick the action buttons below or use the Options menu to trigger live sessions.");

    /* Embedded Dialog Simulation Preview */
    SetRect(&r, 20, 80, 535, 290);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(12);
    TextFace(bold);
    MoveTo(35, 105);
    DrawString("\pSimulated Modal Dialog Structure (DLOG / DITL)");
    TextFace(0);

    /* Default Button 3px Bold Outline Ring */
    SetRect(&btnRect, 410, 235, 510, 265);
    PenSize(3, 3);
    FrameRoundRect(&btnRect, 16, 16);
    PenNormal();
    TextFont(systemFont);
    TextSize(12);
    MoveTo(445, 255);
    DrawString("\pOK");

    /* Cancel Button */
    SetRect(&btnRect, 295, 238, 385, 262);
    FrameRoundRect(&btnRect, 8, 8);
    MoveTo(320, 255);
    DrawString("\pCancel");

    /* Dialog Item Types showcase */
    TextFont(applFont);
    TextSize(9);
    MoveTo(35, 130);
    DrawString("\p* Item Type 1 (btnCtrl): Standard & Default Action Push Buttons (3px ring)");
    MoveTo(35, 150);
    DrawString("\p* Item Type 2 (chkCtrl / radCtrl): Checkboxes & Mutual Exclusion Radio Buttons");
    MoveTo(35, 170);
    DrawString("\p* Item Type 3 (statText): Non-editable Information & Prompt Strings");
    MoveTo(35, 190);
    DrawString("\p* Item Type 4 (editText): TextEdit Buffer with Keyboard Focus & Selection:");

    /* Sample EditText Field */
    SetRect(&editRect, 170, 202, 380, 222);
    RGBForeColor(&black);
    FrameRect(&editRect);
    TextFont(4); /* Monaco */
    TextSize(9);
    MoveTo(176, 216);
    DrawString("\pAce Pilot |");

    /* Status of last modal session */
    TextFont(applFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(35, 282);
    DrawString("\pModal Dialog Status: ");
    TextFace(0);
    if (gModalDialogCompleted) {
        DrawString("\pLast session confirmed with OK.");
    } else {
        DrawString("\pNo modal dialog session completed yet.");
    }

    DrawControls(gMainWindow);
}

/*
 * Palette Manager page. Animated + explicit entries deliberately occupy
 * stable device indexes so AnimateEntry recolors already-drawn pixels without
 * touching the window bitmap. Inside Macintosh Volume VI (1991), pp. 20-10
 * through 20-15 and 20-19 through 20-22.
 */
static void DrawPalettesPage(void)
{
    Rect r;
    RGBColor black;
    RGBColor white;

    black.red = black.green = black.blue = 0;
    white.red = white.green = white.blue = 0xffff;

    RGBForeColor(&black);
    RGBBackColor(&white);
    DrawHeading("\pPalette activation, indexed drawing, and animation");
    MoveTo(24, 58);
    DrawString("\pThe swatches use PmForeColor; the lower well uses PmBackColor.");
    MoveTo(24, 76);
    DrawString("\pAnimated + explicit colors stay at known CLUT indexes.");

    PmForeColor(2);
    SetRect(&r, 30, 96, 180, 166);
    PaintRect(&r);
    PmForeColor(3);
    SetRect(&r, 205, 96, 355, 166);
    PaintOval(&r);
    PmForeColor(4);
    SetRect(&r, 380, 96, 530, 166);
    PaintRect(&r);

    RGBForeColor(&black);
    SetRect(&r, 30, 96, 180, 166); FrameRect(&r);
    SetRect(&r, 205, 96, 355, 166); FrameOval(&r);
    SetRect(&r, 380, 96, 530, 166); FrameRoundRect(&r, 18, 18);
    MoveTo(62, 186); DrawString("\pEntry 2");
    MoveTo(246, 186); DrawString("\pEntry 3");
    MoveTo(422, 186); DrawString("\pEntry 4");

    SetRect(&r, 30, 205, 530, 254);
    FrameRect(&r);
    InsetRect(&r, 2, 2);
    PmBackColor(5);
    EraseRect(&r);
    RGBBackColor(&white);
    RGBForeColor(&black);
    MoveTo(48, 235);
    DrawString("\pTolerant background entry allocated by the Palette Manager");

    RGBBackColor(&white);
    RGBForeColor(&black);
    MoveTo(30, 284);
    DrawString("\pIndexed PICT -> GWorld -> screen");
    MoveTo(30, 314);
    DrawString("\pDevice indexes survive black CLUT");
    DrawSameDeviceIndexedTransfer();
    DrawIndexedPictureTransfer();

    /*
     * A screen GDevice keeps the inverse table used for RGBForeColor
     * separate from the GDevice ColorTable used to build that lookup. Put an
     * exact requested color at hardware index 117 and a visibly different
     * color at the standard inverse-table result (213), then restore those
     * two logical ColorTable entries without touching the hardware. On an
     * 8-bit screen the band must display index 213, even though a direct
     * nearest-color scan would select 117. This is the small, app-independent
     * form of the loading-bar regression that exposed the distinction.
     * Inside Macintosh Volume V (1986), pp. V-137 through V-143.
     */
    {
        GDHandle screenDevice;
        CTabHandle screenColors;
        CTabHandle standardColors;
        ColorSpec colors[2];
        RGBColor requested;

        screenDevice = GetMainDevice();
        screenColors = (**(**screenDevice).gdPMap).pmTable;
        standardColors = GetCTable(8);
        colors[0].value = 117;
        colors[0].rgb.red = 0x1010;
        colors[0].rgb.green = 0x0a0a;
        colors[0].rgb.blue = 0x6969;
        colors[1].value = 213;
        colors[1].rgb.red = 0x7b7b;
        colors[1].rgb.green = 0x7373;
        colors[1].rgb.blue = 0x8484;
        SetEntries(-1, 1, colors);
        if (standardColors != nil) {
            (**screenColors).ctTable[117] = (**standardColors).ctTable[117];
            (**screenColors).ctTable[213] = (**standardColors).ctTable[213];
            CTabChanged(screenColors);
            DisposeCTable(standardColors);
        }

        requested.red = 0;
        requested.green = 0;
        requested.blue = 0x6666;
        RGBForeColor(&requested);
        SetRect(&r, 370, 322, 562, 340);
        PaintRect(&r);
        RGBForeColor(&black);
        FrameRect(&r);
    }
    MoveTo(30, 334);
    DrawString("\pRGBForeColor uses device inverse table");

    MoveTo(245, 361);
    DrawString(gPaletteAnimated ? "\pAnimated CLUT values" : "\pInitial CLUT values");
    DrawControls(gMainWindow);
}

static void AnimateShowcasePalette(void)
{
    RGBColor color;

    gPaletteAnimated = !gPaletteAnimated;
    color.red = 0xffff;
    color.green = gPaletteAnimated ? 0x1111 : 0xffff;
    color.blue = gPaletteAnimated ? 0x9999 : 0x0000;
    AnimateEntry(gMainWindow, 2, &color);

    color.red = gPaletteAnimated ? 0x1111 : 0xffff;
    color.green = gPaletteAnimated ? 0xdddd : 0x6666;
    color.blue = gPaletteAnimated ? 0x5555 : 0x0000;
    AnimateEntry(gMainWindow, 3, &color);

    color.red = gPaletteAnimated ? 0x2222 : 0x0000;
    color.green = gPaletteAnimated ? 0x5555 : 0xcccc;
    color.blue = gPaletteAnimated ? 0xffff : 0xaaaa;
    AnimateEntry(gMainWindow, 4, &color);
}

/*
 * Page 10: Sound Manager channels, sampled playback, command queues, and
 * callbacks. The controls deliberately separate queued commands from
 * immediate quiet/flush commands so their different lifecycles are visible.
 * Sound (1994), pp. 2-19--2-29, 2-92--2-101, and 2-151--2-152.
 */
static void DrawSoundPage(void)
{
    Rect r;
    Boolean busy;

    RefreshShowcaseSoundStatus();
    busy = gShowcaseSoundStatus.scChannelBusy;

    DrawHeading("\pSound Manager: Channels & Sampled Playback");
    MoveTo(24, 56);
    DrawString("\pSysBeep, SndPlay, FIFO commands, immediate control, and callbacks.");
    MoveTo(24, 70);
    DrawString("\pThe small resource-backed sample is mixed into the deterministic audio stream.");

    SetRect(&r, 20, 82, 290, 238);
    DrawBeveledBox(&r, false);
    SetRect(&r, 305, 82, 535, 238);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(10);
    TextFace(bold);
    MoveTo(32, 101);
    DrawString("\pChannel & status");
    MoveTo(317, 101);
    DrawString("\pCommand lifecycle");
    TextFace(0);

    TextFont(applFont);
    TextSize(9);
    MoveTo(32, 122);
    DrawString("\pSndNewChannel: ");
    DrawString(gShowcaseSoundChannel != nil ? "\pallocated" : "\pdisposed");
    MoveTo(32, 140);
    DrawString("\pSndChannelStatus: ");
    DrawString(busy ? "\pbusy" : "\pidle");
    MoveTo(32, 158);
    DrawString("\pSndPlay resource: ");
    DrawString(gSoundStarted ? "\pstarted" : "\pidle");
    MoveTo(32, 176);
    DrawString("\pSysBeep(30): ");
    DrawString(gSoundBeeped ? "\prequested" : "\pnot requested");
    MoveTo(32, 194);
    DrawString("\pCompletion: ");
    DrawString(gSoundCompleted ? "\preceived" : "\pwaiting");
    MoveTo(32, 212);
    DrawString("\pStatus error: ");
    DrawString(gSoundStatusError == noErr ? "\pnoErr" : "\perror");

    MoveTo(317, 122);
    DrawString("\pSndDoCommand FIFO: ");
    DrawString(gSoundQueued ? "\psent" : "\pidle");
    MoveTo(317, 140);
    DrawString("\pSndDoImmediate flush: ");
    DrawString(gSoundFlushed ? "\pissued" : "\pidle");
    MoveTo(317, 158);
    DrawString("\pSndDoImmediate quiet: ");
    DrawString(gSoundQuieted ? "\pissued" : "\pidle");
    MoveTo(317, 176);
    DrawString("\pCallback command: ");
    DrawString(gSoundQueued ? "\pqueued" : "\pnot queued");
    MoveTo(317, 194);
    DrawString("\pResource: ");
    DrawString(gShowcaseSound != nil ? "\ploaded" : "\pmissing");
    MoveTo(317, 212);
    DrawString("\pDispose: ");
    DrawString(gSoundDisposed ? "\pcomplete" : "\pavailable");

    DrawControls(gMainWindow);
}

/*
 * Page 8: TextEdit: Multiline Buffer, Justification, Scrap & Selection
 */
static void DrawTextEditPage(void)
{
    Rect r;
    Rect well;
    Rect calloutRect;
    Str255 numStr;
    RGBColor white;
    RGBColor black;

    white.red = white.green = white.blue = 0xffff;
    black.red = black.green = black.blue = 0x0000;

    DrawHeading("\pTextEdit: Multiline Buffer, Justification, Scrap & Selection");

    /* Section 1: Interactive TextEdit View */
    SetRect(&r, 20, 48, 340, 225);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 62);
    DrawString("\pInteractive TERec Buffer (Geneva 9pt)");
    TextFace(0);

    /* Sunken Well around live TextEdit field */
    SetRect(&well, 28, 70, 332, 217);
    DrawBeveledBox(&well, true);

    /* Erase TextEdit content rect and draw updated text */
    if (gTE != nil) {
        RGBBackColor(&white);
        RGBForeColor(&black);
        EraseRect(&gTERect);
        TEUpdate(&gTERect, gTE);
    }

    /* Section 2: Static TETextBox Callout */
    SetRect(&r, 350, 48, 535, 140);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(358, 62);
    DrawString("\pTETextBox Callout (Centered)");
    TextFace(0);

    SetRect(&well, 358, 70, 527, 132);
    DrawBeveledBox(&well, true);

    SetRect(&calloutRect, 362, 74, 523, 128);
    TextFont(applFont);
    TextSize(9);
    RGBBackColor(&white);
    RGBForeColor(&black);
    EraseRect(&calloutRect);
    TETextBox((const void *)kTECalloutText, sizeof(kTECalloutText) - 1, &calloutRect, teJustCenter);

    /* Section 3: Alignment Controls */
    SetRect(&r, 350, 146, 535, 225);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(358, 160);
    DrawString("\pParagraph Alignment (TESetJust)");
    TextFace(0);

    TextFont(applFont);
    TextSize(9);
    MoveTo(358, 206);
    DrawString("\pMode: ");
    if (gTEJust == teJustLeft) {
        DrawString("\pteJustLeft (0 - flush left)");
    } else if (gTEJust == teJustCenter) {
        DrawString("\pteJustCenter (1 - centered)");
    } else if (gTEJust == teJustRight) {
        DrawString("\pteJustRight (-1 - flush right)");
    }

    /* Section 4: Scrap Operations */
    SetRect(&r, 20, 233, 340, 310);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 247);
    DrawString("\pClipboard Scrap (TECut / TECopy / TEPaste)");
    TextFace(0);

    TextFont(3);
    TextSize(9);
    MoveTo(28, 300);
    DrawString("\pInteracts with internal TextEdit scrap buffer.");

    /* Section 5: TERec Live Metrics Inspector */
    SetRect(&r, 350, 233, 535, 310);
    DrawBeveledBox(&r, false);

    TextFont(applFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(358, 248);
    DrawString("\pTERec Inspector:");
    TextFace(0);

    if (gTE != nil) {
        MoveTo(358, 264);
        DrawString("\pLength: ");
        NumToString((**gTE).teLength, numStr);
        DrawString(numStr);
        DrawString("\p bytes | Lines: ");
        NumToString((**gTE).nLines, numStr);
        DrawString(numStr);

        MoveTo(358, 280);
        DrawString("\pSelection: [");
        NumToString((**gTE).selStart, numStr);
        DrawString(numStr);
        DrawString("\p..");
        NumToString((**gTE).selEnd, numStr);
        DrawString(numStr);
        DrawString("\p]");

        MoveTo(358, 296);
        DrawString("\pFont: Geneva | Size: 9pt | Active: Yes");
    }

    /* Footer Note */
    RGBBackColor(&white);
    RGBForeColor(&black);
    MoveTo(20, 335);
    TextFont(applFont);
    TextSize(9);
    DrawString("\pClick to position caret; drag to select. Type text or use buttons to edit.");

    DrawControls(gMainWindow);
}

/*
 * Page 9: Lists & Inventory.
 *
 * The default text list definition (theProc = 0) owns the cell drawing. The
 * page intentionally keeps the list in one column so every operation is
 * visible in the same deterministic row geometry on both CPU slices.
 * More Macintosh Toolbox (1993), pp. 4-70--4-76, 4-81--4-84, and 4-90--4-95.
 */
static short InventoryTextLength(const char *text)
{
    short length;

    length = 0;
    while (text[length] != 0) length++;
    return length;
}

static void UpdateInventoryList(void)
{
    RgnHandle updateRegion;

    if (gInventoryList == nil) return;
    updateRegion = NewRgn();
    if (updateRegion != nil) {
        RectRgn(updateRegion, &gInventoryView);
        LUpdate(updateRegion, gInventoryList);
        DisposeRgn(updateRegion);
    } else {
        LUpdate(nil, gInventoryList);
    }
}

static void PopulateInventoryList(void)
{
    Cell cell;
    short row;

    if (gInventoryList == nil) return;

    /* Batch structural and cell writes with automatic drawing disabled. */
    LSetDrawingMode(false, gInventoryList);
    LAddRow(kInventoryRowCount - 1, 1, gInventoryList);
    for (row = 0; row < kInventoryRowCount; row++) {
        cell.v = row;
        cell.h = 0;
        LSetCell((Ptr)kInventoryItems[row], InventoryTextLength(kInventoryItems[row]),
                 cell, gInventoryList);
    }
    LSetDrawingMode(true, gInventoryList);
    UpdateInventoryList();
}

static Boolean InspectInventorySelection(void)
{
    Cell cell;
    char cellData[96];
    short dataLength;

    cell.v = 0;
    cell.h = 0;
    gListSelectedRow = -1;
    gListSelectedLength = 0;
    if (gInventoryList == nil || !LGetSelect(true, &cell, gInventoryList)) {
        gListStatus = listStatusNone;
        return false;
    }

    /* LGetCell is the inspection path; the buffer is intentionally bounded. */
    dataLength = sizeof(cellData);
    LGetCell(cellData, &dataLength, cell, gInventoryList);
    gListSelectedRow = cell.v;
    gListSelectedLength = dataLength;
    gListStatus = listStatusSelected;
    return true;
}

static void MutateInventorySelection(void)
{
    Cell cell;
    char cellData[96];
    short dataLength;
    short suffixLength;
    short i;
    static const char suffix[] = "  * updated";

    cell.v = 0;
    cell.h = 0;
    if (gInventoryList == nil || !LGetSelect(true, &cell, gInventoryList)) {
        gListStatus = listStatusNone;
        gListSelectedRow = -1;
        gListSelectedLength = 0;
        return;
    }

    dataLength = sizeof(cellData);
    LGetCell(cellData, &dataLength, cell, gInventoryList);
    suffixLength = InventoryTextLength(suffix);
    if (dataLength > sizeof(cellData) - suffixLength) {
        dataLength = sizeof(cellData) - suffixLength;
    }
    for (i = 0; i < suffixLength; i++) {
        cellData[dataLength + i] = suffix[i];
    }
    dataLength += suffixLength;
    LSetCell(cellData, dataLength, cell, gInventoryList);
    gListSelectedRow = cell.v;
    gListSelectedLength = dataLength;
    gListStatus = listStatusMutated;
    UpdateInventoryList();
}

static void ScrollInventoryList(void)
{
    if (gInventoryList == nil) return;
    LScroll(0, 4, gInventoryList);
    gListStatus = listStatusScrolled;
    UpdateInventoryList();
}

static void ResizeInventoryList(void)
{
    if (gInventoryList == nil) return;
    if (gListResized) {
        LSize(504, 150, gInventoryList);
        SetRect(&gInventoryView, 24, 78, 528, 228);
        gListResized = false;
    } else {
        LSize(450, 114, gInventoryList);
        SetRect(&gInventoryView, 24, 78, 474, 192);
        gListResized = true;
    }
    gListStatus = listStatusResized;
    UpdateInventoryList();
}

static void ToggleInventoryActivation(void)
{
    if (gInventoryList == nil) return;
    gListActive = !gListActive;
    LActivate(gListActive, gInventoryList);
    gListStatus = gListActive ? listStatusActivated : listStatusInactive;
    UpdateInventoryList();
}

static void DrawListsPage(void)
{
    Rect frame;
    Str255 number;

    DrawHeading("\pLists & Inventory: List Manager operations");
    TextFont(applFont);
    TextSize(9);
    MoveTo(24, 54);
    DrawString("\pLNew + LAddRow + LSetCell populate a default text LDEF.");
    MoveTo(24, 66);
    DrawString("\pClick a row, inspect it, mutate it, scroll, resize, or toggle activation.");

    frame = gInventoryView;
    UpdateInventoryList();
    FrameRect(&frame);

    MoveTo(24, 316);
    TextFace(bold);
    DrawString("\pList Manager status: ");
    TextFace(0);
    if (gInventoryList == nil) {
        DrawString("\pLNew failed");
    } else if (gListStatus == listStatusNone) {
        DrawString("\pno cell selected");
    } else if (gListStatus == listStatusSelected) {
        DrawString("\pselected row ");
        NumToString(gListSelectedRow + 1, number);
        DrawString(number);
        DrawString("\p (LGetSelect/LGetCell)");
    } else if (gListStatus == listStatusMutated) {
        DrawString("\prow ");
        NumToString(gListSelectedRow + 1, number);
        DrawString(number);
        DrawString("\p updated with LSetCell");
    } else if (gListStatus == listStatusScrolled) {
        DrawString("\pscrolled with LScroll (four-row request)");
    } else if (gListStatus == listStatusResized) {
        DrawString(gListResized ? "\presized to compact view with LSize"
                               : "\prestored full view with LSize");
    } else if (gListStatus == listStatusInactive) {
        DrawString("\pinactive with LActivate(FALSE)");
    } else {
        DrawString("\pactive with LActivate(TRUE)");
    }

    MoveTo(24, 336);
    DrawString("\pSelected cell bytes: ");
    NumToString(gListSelectedLength, number);
    DrawString(number);
    DrawString(gListActive ? "\p | list active" : "\p | list inactive");

    DrawControls(gMainWindow);
}

/*
 * Page 13: Resource Browser. The map is inspected with loading disabled so
 * the table can show the difference between a resource reference and its
 * resident data. The selected named resource then follows the ordinary
 * SetResLoad(FALSE) -> GetNamedResource -> LoadResource -> ReleaseResource
 * path, including a subsequent reload of the same map reference.
 * Inside Macintosh: More Macintosh Toolbox (1993), pp. 1-75--1-82 and
 * 1-92--1-93; Inside Macintosh Volume I (1985), pp. I-118--I-125.
 */
static Boolean ResourceBrowserHandleLoaded(Handle handle)
{
    return handle != nil && *handle != nil;
}

static void ResourceBrowserTypeString(ResType type, Str255 string)
{
    string[0] = 4;
    string[1] = (char)(type >> 24);
    string[2] = (char)(type >> 16);
    string[3] = (char)(type >> 8);
    string[4] = (char)type;
}

static Boolean ResourceBrowserCheckError(void)
{
    gResourceError = ResError();
    if (gResourceError != noErr) {
        gResourceStatus = resourceStatusError;
        return false;
    }
    return true;
}

static void ResourceBrowserRestoreAutomaticLoading(void)
{
    short error;

    SetResLoad(true);
    error = ResError();
    if (gResourceError == noErr && error != noErr) gResourceError = error;
    if (gResourceError != noErr) gResourceStatus = resourceStatusError;
}

static void ResourceBrowserClearEntry(short index)
{
    gResourceEntries[index].handle = nil;
    gResourceEntries[index].id = 0;
    gResourceEntries[index].type = 0;
    gResourceEntries[index].name[0] = 0;
    gResourceEntries[index].attrs = 0;
    gResourceEntries[index].size = 0;
    gResourceEntries[index].loaded = false;
    gResourceEntries[index].present = false;
}

static Boolean ReadResourceBrowserEntries(void)
{
    short i;
    short visibleCount;
    Handle handle;
    ResType resourceType;
    long resourceSize;
    Boolean ok;
    Boolean resLoadDisabled;

    gResourceError = noErr;
    gResourceCount = 0;
    gResourceMenuCount = 0;
    gResourceWindowCount = 0;
    for (i = 0; i < kResourceBrowserRows; i++) ResourceBrowserClearEntry(i);

    ok = true;
    resLoadDisabled = false;
    /* Count1Resources is map-only, so it is safe while automatic loading is
     * disabled. Keep three counts visible to make the selected map scope
     * explicit and to compare the custom DATA records with existing UI data.
     */
    gResourceCount = Count1Resources('DATA');
    if (!ResourceBrowserCheckError()) ok = false;
    if (ok) {
        gResourceMenuCount = Count1Resources('MENU');
        if (!ResourceBrowserCheckError()) ok = false;
    }
    if (ok) {
        gResourceWindowCount = Count1Resources('WIND');
        if (!ResourceBrowserCheckError()) ok = false;
    }

    if (ok) {
        SetResLoad(false);
        resLoadDisabled = true;
        if (!ResourceBrowserCheckError()) ok = false;
    }
    visibleCount = ok ? gResourceCount : 0;
    if (visibleCount > kResourceBrowserRows) visibleCount = kResourceBrowserRows;
    for (i = 0; ok && i < visibleCount; i++) {
        handle = Get1IndResource('DATA', i + 1);
        if (!ResourceBrowserCheckError()) {
            ok = false;
            break;
        }
        if (handle == nil) {
            gResourceError = -192;
            gResourceStatus = resourceStatusError;
            ok = false;
            break;
        }

        gResourceEntries[i].handle = handle;
        GetResInfo(handle, &gResourceEntries[i].id, &resourceType,
                   gResourceEntries[i].name);
        if (!ResourceBrowserCheckError()) {
            ok = false;
            break;
        }
        gResourceEntries[i].type = resourceType;
        gResourceEntries[i].attrs = GetResAttrs(handle);
        if (!ResourceBrowserCheckError()) {
            ok = false;
            break;
        }
        gResourceEntries[i].loaded = ResourceBrowserHandleLoaded(handle);
        gResourceEntries[i].present = true;
        /* GetResourceSizeOnDisk (also exported as SizeResource) reports the
         * exact map-backed byte size without materializing an empty handle.
         * This preserves the deferred state while remaining valid on classic
         * Resource Manager implementations that invalidate released handles. */
        resourceSize = GetResourceSizeOnDisk(handle);
        if (!ResourceBrowserCheckError() || resourceSize < 0) {
            if (gResourceError == noErr) gResourceError = -192;
            gResourceStatus = resourceStatusError;
            ok = false;
            break;
        }
        gResourceEntries[i].size = resourceSize;
    }

    for (i = visibleCount; i < kResourceBrowserRows; i++) ResourceBrowserClearEntry(i);
    /* More Macintosh Toolbox warns that callers must restore automatic
     * loading promptly because other Toolbox managers rely on the default. */
    if (resLoadDisabled) ResourceBrowserRestoreAutomaticLoading();
    return ok && gResourceError == noErr;
}

static Boolean PrepareResourceBrowser(void)
{
    if (!gResourceBrowserReady) {
        if (!ReadResourceBrowserEntries()) return false;
        gResourceStatus = resourceStatusEnumerated;
        gResourceBrowserReady = true;
    }
    return true;
}

static void RefreshResourceBrowser(void)
{
    if (!PrepareResourceBrowser()) return;
    if (!ReadResourceBrowserEntries()) return;
    gResourceStatus = resourceStatusEnumerated;
}

static void LoadNamedResourceBrowserEntry(void)
{
    Handle handle;

    if (!PrepareResourceBrowser()) return;
    /* First obtain the named reference without reading its data, then restore
     * the normal policy and explicitly materialize it through LoadResource. */
    SetResLoad(false);
    if (!ResourceBrowserCheckError()) {
        ResourceBrowserRestoreAutomaticLoading();
        return;
    }
    handle = GetNamedResource('DATA', "\pMutable Record");
    if (!ResourceBrowserCheckError()) {
        ResourceBrowserRestoreAutomaticLoading();
        return;
    }
    ResourceBrowserRestoreAutomaticLoading();
    if (gResourceError != noErr) return;
    if (handle == nil) {
        gResourceError = -192;
        gResourceStatus = resourceStatusError;
        gResourceEditHandle = nil;
        return;
    }

    gResourceEditHandle = handle;
    LoadResource(gResourceEditHandle);
    if (!ResourceBrowserCheckError() || !ResourceBrowserHandleLoaded(gResourceEditHandle)) {
        if (gResourceError == noErr) gResourceError = -192;
        gResourceStatus = resourceStatusError;
        return;
    }
    if (!ReadResourceBrowserEntries()) return;
    gResourceStatus = resourceStatusLoaded;
}

static void ReleaseNamedResourceBrowserEntry(void)
{
    if (!PrepareResourceBrowser()) return;
    if (gResourceEditHandle == nil) {
        gResourceError = -192;
        gResourceStatus = resourceStatusError;
        return;
    }

    ReleaseResource(gResourceEditHandle);
    if (!ResourceBrowserCheckError()) {
        if (gResourceError == noErr) gResourceError = -192;
        gResourceStatus = resourceStatusError;
        return;
    }
    gResourceEditHandle = nil;
    if (!ReadResourceBrowserEntries()) return;
    gResourceStatus = resourceStatusReleased;
}

static void DrawResourceLifecycleStatus(void)
{
    Str255 number;

    if (gResourceStatus == resourceStatusLoaded) {
        DrawString("\ploaded via GetNamedResource + LoadResource");
    } else if (gResourceStatus == resourceStatusReleased) {
        DrawString("\preleased; map reference is empty");
    } else if (gResourceStatus == resourceStatusError) {
        DrawString("\pResource Manager error ");
        NumToString(gResourceError, number);
        DrawString(number);
    } else {
        DrawString("\penumerated with SetResLoad(FALSE)");
    }
}

static void DrawResourceBrowserPage(void)
{
    Rect table;
    Str255 number;
    Str255 typeName;
    short i;
    short rowTop;

    DrawHeading("\pResource Browser: Resource Manager map and lifecycle");
    TextFont(applFont);
    TextSize(9);
    MoveTo(24, 54);
    DrawString("\pCount1Resources: DATA ");
    NumToString(gResourceCount, number);
    DrawString(number);
    MoveTo(180, 54);
    DrawString("\pMENU ");
    NumToString(gResourceMenuCount, number);
    DrawString(number);
    MoveTo(310, 54);
    DrawString("\pWIND ");
    NumToString(gResourceWindowCount, number);
    DrawString(number);

    MoveTo(24, 68);
    DrawString("\pGet1IndResource + GetResInfo + GetResAttrs + GetResourceSizeOnDisk");
    table.top = 78;
    table.left = 20;
    table.bottom = 170;
    table.right = 540;
    DrawBeveledBox(&table, true);

    MoveTo(28, 91);
    TextFace(bold);
    DrawString("\pType");
    MoveTo(76, 91);
    DrawString("\pID");
    MoveTo(122, 91);
    DrawString("\pName");
    MoveTo(318, 91);
    DrawString("\pAttrs");
    MoveTo(374, 91);
    DrawString("\pSize");
    MoveTo(430, 91);
    DrawString("\pState");
    TextFace(0);

    for (i = 0; i < kResourceBrowserRows; i++) {
        rowTop = 108 + i * 19;
        MoveTo(28, rowTop);
        if (gResourceEntries[i].present) {
            ResourceBrowserTypeString(gResourceEntries[i].type, typeName);
            DrawString(typeName);
        } else {
            DrawString("\p--");
        }
        MoveTo(76, rowTop);
        if (gResourceEntries[i].present) {
            NumToString(gResourceEntries[i].id, number);
            DrawString(number);
        } else {
            DrawString("\p--");
        }
        MoveTo(122, rowTop);
        if (gResourceEntries[i].present) {
            DrawString(gResourceEntries[i].name);
        } else {
            DrawString("\p(no resource)");
        }
        MoveTo(318, rowTop);
        if ((gResourceEntries[i].attrs & 0x0002) != 0) {
            DrawString("\p02 changed");
        } else {
            DrawString("\p00 clean");
        }
        MoveTo(374, rowTop);
        NumToString((short)gResourceEntries[i].size, number);
        DrawString(number);
        MoveTo(430, rowTop);
        DrawString(gResourceEntries[i].loaded ? "\ploaded" : "\pempty");
    }

    MoveTo(24, 192);
    TextFace(bold);
    DrawString("\pSelected: ");
    TextFace(0);
    DrawString("\pDATA 203 \267 Mutable Record");
    MoveTo(24, 210);
    TextFace(bold);
    DrawString("\pLifecycle: ");
    TextFace(0);
    DrawResourceLifecycleStatus();
    MoveTo(24, 228);
    DrawString("\pThe table stays map-backed while resource data moves in and out of memory.");
    MoveTo(24, 338);
    DrawString("\pRefresh re-enumerates deferred references; Load and Release reuse one named record.");

    DrawControls(gMainWindow);
}

/*
 * Page 11: Styled TextEdit and Font Manager measurements.
 * The upper well is the rendered TextEdit record itself. The lower panels
 * report values returned by the same style and measurement APIs that created
 * that record; no parallel swatch model is used. Inside Macintosh: Text
 * (1993), pp. 2-78, 2-98..2-102, and 3-81..3-82.
 */
static void SetStyledStyle(TextStyle *style, short font, Style face,
                           short size, const RGBColor *color)
{
    style->tsFont = font;
    style->tsFace = face;
    style->tsSize = size;
    style->tsColor = *color;
}

static void ApplyStyledStyle(short start, short end, TextStyle *style)
{
    TESetSelect(start, end, gStyledTE);
    TESetStyle(doAll, style, true, gStyledTE);
}

static void InspectStyledText(void)
{
    short textLength;

    if (gStyledTE == nil) return;
    textLength = sizeof(kStyledSampleText) - 1;
    TESetSelect(0, textLength, gStyledTE);
    gStyledInspectMode = doAll;
    gStyledInspectResult = TEContinuousStyle(&gStyledInspectMode,
                                             &gStyledInspectStyle, gStyledTE);

    TESetSelect(7, 11, gStyledTE);
    gStyledRunMode = doAll;
    gStyledRunResult = TEContinuousStyle(&gStyledRunMode,
                                         &gStyledRunStyle, gStyledTE);
    TESetSelect(0, 0, gStyledTE);
}

static void MeasureStyledText(void)
{
    static const char measureText[] = "Styled";
    short charLocations[sizeof(measureText)];
    short count;

    count = sizeof(measureText) - 1;
    TextFont(gStyledGenevaFont);
    TextFace(0);
    TextSize(10);
    gStyledCharWidth = CharWidth('A');
    gStyledTextWidth = TextWidth((Ptr)measureText, 0, count);
    MeasureText(count, (Ptr)measureText, (Ptr)charLocations);
    gStyledMeasureWidth = charLocations[count];
}

static void InitializeStyledText(void)
{
    TextStyle style;
    RGBColor black;
    RGBColor blue;
    RGBColor red;
    RGBColor green;
    RGBColor purple;
    short textLength;

    black.red = black.green = black.blue = 0x0000;
    blue.red = 0x1111; blue.green = 0x2222; blue.blue = 0xdddd;
    red.red = 0xffff; red.green = 0x2222; red.blue = 0x2222;
    green.red = 0x1111; green.green = 0x9999; green.blue = 0x2222;
    purple.red = 0x8888; purple.green = 0x2222; purple.blue = 0x9999;

    /* Text 1993, pp. 4-52..4-53: resolve family names through Font Manager. */
    gStyledGenevaFont = 0;
    gStyledMonacoFont = 0;
    GetFNum("\pGeneva", &gStyledGenevaFont);
    GetFNum("\pMonaco", &gStyledMonacoFont);
    gStyledGenevaReal = gStyledGenevaFont != 0 && RealFont(gStyledGenevaFont, 9);
    gStyledMonacoReal = gStyledMonacoFont != 0 && RealFont(gStyledMonacoFont, 9);

    SetRect(&gStyledTERect, 34, 76, 521, 114);
    TextFont(gStyledGenevaFont);
    TextFace(0);
    TextSize(10);
    RGBForeColor(&black);
    gStyledTE = TEStyleNew(&gStyledTERect, &gStyledTERect);
    if (gStyledTE == nil) return;
    textLength = sizeof(kStyledSampleText) - 1;
    TESetText((const void *)kStyledSampleText, textLength, gStyledTE);

    SetStyledStyle(&style, gStyledGenevaFont, 0, 10, &black);
    ApplyStyledStyle(0, 5, &style);
    SetStyledStyle(&style, gStyledGenevaFont, bold, 12, &blue);
    ApplyStyledStyle(7, 11, &style);
    SetStyledStyle(&style, gStyledMonacoFont, 0, 10, &red);
    ApplyStyledStyle(13, 18, &style);
    SetStyledStyle(&style, gStyledMonacoFont, italic, 14, &green);
    ApplyStyledStyle(20, 24, &style);
    SetStyledStyle(&style, gStyledGenevaFont, underline, 10, &purple);
    ApplyStyledStyle(26, 30, &style);

    InspectStyledText();
    MeasureStyledText();
}

static void DrawStyledTextPage(void)
{
    Rect r;
    Rect well;
    Rect bar;
    Str255 numStr;
    RGBColor white;
    RGBColor black;
    RGBColor blue;
    RGBColor green;
    RGBColor purple;

    white.red = white.green = white.blue = 0xffff;
    black.red = black.green = black.blue = 0x0000;
    blue.red = 0x1111; blue.green = 0x2222; blue.blue = 0xdddd;
    green.red = 0x1111; green.green = 0x9999; green.blue = 0x2222;
    purple.red = 0x8888; purple.green = 0x2222; purple.blue = 0x9999;

    DrawHeading("\pStyled Text & Fonts");
    SetRect(&r, 20, 48, 535, 128);
    DrawBeveledBox(&r, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 62);
    DrawString("\pTEStyleNew live multistyled record");
    TextFace(0);
    SetRect(&well, 28, 70, 527, 120);
    DrawBeveledBox(&well, true);
    if (gStyledTE != nil) {
        RGBBackColor(&white);
        RGBForeColor(&black);
        EraseRect(&gStyledTERect);
        TEUpdate(&gStyledTERect, gStyledTE);
    }

    SetRect(&r, 20, 138, 300, 310);
    DrawBeveledBox(&r, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 153);
    DrawString("\pTESetStyle runs (doAll)");
    TextFace(0);
    TextFont(applFont);
    TextSize(9);
    MoveTo(28, 174); DrawString("\pPlain: Geneva 10pt / black");
    MoveTo(28, 189); DrawString("\pBold: Geneva 12pt / blue");
    MoveTo(28, 204); DrawString("\pColor: Monaco 10pt / red");
    MoveTo(28, 219); DrawString("\pSize: Monaco 14pt / italic green");
    MoveTo(28, 234); DrawString("\pFont: Geneva 10pt / underline purple");
    MoveTo(28, 260); DrawString("\pRun boundaries and colors are stored");
    MoveTo(28, 275); DrawString("\pin the live TEStyleHandle.");
    RGBForeColor(&purple);
    MoveTo(28, 294); DrawString("\pThe upper text is the assertion surface.");

    SetRect(&r, 310, 138, 535, 310);
    DrawBeveledBox(&r, false);
    RGBForeColor(&black);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(318, 153); DrawString("\pTEContinuousStyle + Font Manager");
    TextFace(0);
    TextFont(applFont);
    TextSize(9);
    MoveTo(318, 172); DrawString("\pAll runs: ");
    DrawString(gStyledInspectResult ? "\pcontinuous" : "\pmixed");
    MoveTo(318, 187); DrawString("\pMode mask: ");
    NumToString(gStyledInspectMode, numStr); DrawString(numStr);
    MoveTo(318, 202); DrawString("\pBold run: ");
    DrawString(gStyledRunResult ? "\pcontinuous" : "\pmixed");
    MoveTo(318, 217); DrawString("\pRun font/size: ");
    NumToString(gStyledRunStyle.tsFont, numStr); DrawString(numStr);
    DrawString("\p / ");
    NumToString(gStyledRunStyle.tsSize, numStr); DrawString(numStr);
    DrawString("\ppt");
    MoveTo(318, 232); DrawString("\pGetFNum: ");
    NumToString(gStyledGenevaFont, numStr); DrawString(numStr);
    DrawString("\p Geneva / ");
    NumToString(gStyledMonacoFont, numStr); DrawString(numStr);
    DrawString("\p Monaco");
    MoveTo(318, 247); DrawString("\pRealFont 9pt: ");
    DrawString(gStyledGenevaReal ? "\pGeneva yes / " : "\pGeneva no / ");
    DrawString(gStyledMonacoReal ? "\pMonaco yes" : "\pMonaco no");

    /* These bars are lengths returned by CharWidth, TextWidth, and MeasureText. */
    MoveTo(318, 263); DrawString("\pMeasured widths");
    SetRect(&r, 395, 257, 520, 305); FrameRect(&r);
    RGBForeColor(&blue);
    SetRect(&bar, 398, 263, 398 + gStyledCharWidth * 5, 270); PaintRect(&bar);
    RGBForeColor(&green);
    SetRect(&bar, 398, 278, 398 + gStyledTextWidth * 2, 285); PaintRect(&bar);
    RGBForeColor(&purple);
    SetRect(&bar, 398, 293, 398 + gStyledMeasureWidth * 2, 300); PaintRect(&bar);
    RGBForeColor(&black);
    MoveTo(318, 270); DrawString("\pChar");
    MoveTo(318, 285); DrawString("\pText");
    MoveTo(318, 300); DrawString("\pCumulative");

    RGBBackColor(&white);
    RGBForeColor(&black);
    MoveTo(20, 335);
    TextFont(applFont);
    TextSize(9);
    DrawString("\pActual TEStyle runs drive the rendered colors, fonts, faces, and measured widths.");
}

/*
 * Page 12: Standard File. Each button deliberately calls a different public
 * entry point. The Open path enters the fixture folder and accepts the sole
 * TEXT item; the legacy Open and Save paths are canceled; both Save paths
 * begin with an editable default name. Inside Macintosh: Files (1992),
 * pp. 3-42--3-54.
 */
static void DrawFileStatus(short status)
{
    if (status == fileStatusAccepted) {
        DrawString("\paccepted");
    } else if (status == fileStatusCancelled) {
        DrawString("\pcancelled");
    } else if (status == fileStatusError) {
        DrawString("\perror");
    } else {
        DrawString("\pnot exercised");
    }
}

static void DrawFileStatusLine(short top, ConstStr255Param label, short status)
{
    MoveTo(32, top);
    DrawString(label);
    DrawFileStatus(status);
}

static void DoStandardFileOpen(void)
{
    StandardGetFile(nil, 1, gFileTypeList, &gFileOpenReply);
    gFileOpenStatus = gFileOpenReply.sfGood ? fileStatusAccepted : fileStatusCancelled;
}

static void DoLegacyFileOpen(void)
{
    Point where;

    where.v = 0;
    where.h = 0;
    SFGetFile(where, "\pCancel this filtered legacy Open", nil, 1,
              gFileTypeList, nil, &gFileLegacyOpenReply);
    gFileLegacyOpenStatus = gFileLegacyOpenReply.good ? fileStatusAccepted : fileStatusCancelled;
}

static void DoStandardFileSave(void)
{
    OSErr err;

    StandardPutFile("\pSave a named fixture", "\pUntitled", &gFileSaveReply);
    if (!gFileSaveReply.sfGood) {
        gFileSaveStatus = fileStatusCancelled;
        return;
    }

    /* The returned FSSpec is consumed by File Manager, then removed so the
     * next Open dialog sees only the immutable fixture entries. */
    err = FSpCreate(&gFileSaveReply.sfFile, 'SHWC', 'TEXT', smSystemScript);
    if (err == noErr) {
        err = FSpDelete(&gFileSaveReply.sfFile);
    }
    gFileSaveStatus = err == noErr ? fileStatusAccepted : fileStatusError;
}

static void DoLegacyFileSave(void)
{
    Point where;

    where.v = 0;
    where.h = 0;
    SFPutFile(where, "\pCancel this legacy Save", "\pUntitled", nil,
              &gFileLegacySaveReply);
    gFileLegacySaveStatus = gFileLegacySaveReply.good ? fileStatusAccepted : fileStatusCancelled;
}

static void DrawStandardFilePage(void)
{
    Rect frame;

    DrawHeading("\pStandard File: Open, Save As, filtering & cancellation");
    TextFont(applFont);
    TextSize(9);
    MoveTo(24, 54);
    DrawString("\pThe fixture folder contains a TEXT document and a DATA file.");
    MoveTo(24, 68);
    DrawString("\pOpen TEXT navigates into it; the other controls exercise legacy APIs.");

    SetRect(&frame, 20, 84, 535, 216);
    DrawBeveledBox(&frame, false);
    TextFace(bold);
    MoveTo(32, 106);
    DrawString("\pReturned record checkpoints");
    TextFace(0);
    MoveTo(32, 126);
    DrawString("\pModern Open: sfGood + sfType + sfFile FSSpec");
    MoveTo(32, 142);
    DrawString("\pLegacy Open: good + fType + vRefNum + fName");
    MoveTo(32, 158);
    DrawString("\pModern Save: editable sfFile name, then FSpCreate/FSpDelete");
    MoveTo(32, 174);
    DrawString("\pLegacy Save: editable fName and SFReply cancellation");

    TextFace(bold);
    DrawFileStatusLine(236, "\pModern Open: ", gFileOpenStatus);
    DrawFileStatusLine(254, "\pLegacy Open: ", gFileLegacyOpenStatus);
    DrawFileStatusLine(272, "\pModern Save: ", gFileSaveStatus);
    DrawFileStatusLine(290, "\pLegacy Save: ", gFileLegacySaveStatus);
    TextFace(0);
    MoveTo(32, 316);
    DrawString("\pAccepted Open returns TEXT; Cancel leaves sfGood/good false.");
    MoveTo(32, 332);
    DrawString("\pNames are MacRoman PStrings and FSSpec directory IDs are checked.");
    DrawControls(gMainWindow);
}

/*
 * Page 14: Sprites, Masks & Scrolling.
 *
 * The scene is deliberately rendered into an 8-bit offscreen GWorld before
 * one CopyBits call presents it in the window. This keeps the source pixels
 * and the 1-bit matte stable while the destination exercises both the 8-bit
 * indexed 68K screen and the direct-color PowerPC screen. CopyMask is the
 * classic sprite operation; CopyDeepMask applies the same source through an
 * 8-bit deep mask and a BitMapToRegion-derived clip. ScrollRect then moves
 * the existing offscreen raster and reports its vacated strip through an
 * update region. Imaging With QuickDraw (1994), pp. 2-20--2-24,
 * 2-43--2-50, 3-119--3-122, and 6-22--6-46.
 */
static void BuildSpriteSource(void)
{
    GWorldPtr savedWorld;
    GDHandle savedDevice;
    Rect full;
    Rect body;
    Rect visor;
    RGBColor background;
    RGBColor bodyColor;
    RGBColor highlight;
    RGBColor black;
    RGBColor white;

    background.red = 0x1111; background.green = 0x2222; background.blue = 0x5555;
    bodyColor.red = gSpriteAnimated ? 0x22ff : 0xff22;
    bodyColor.green = gSpriteAnimated ? 0xdddd : 0x3333;
    bodyColor.blue = gSpriteAnimated ? 0xffff : 0x2222;
    highlight.red = gSpriteAnimated ? 0xffff : 0xffaa;
    highlight.green = gSpriteAnimated ? 0xffff : 0xaa22;
    highlight.blue = gSpriteAnimated ? 0x3333 : 0x1111;
    black.red = black.green = black.blue = 0;
    white.red = white.green = white.blue = 0xffff;

    GetGWorld(&savedWorld, &savedDevice);
    SetGWorld(gSpriteSource, nil);
    SetRect(&full, 0, 0, kSpriteSize, kSpriteSize);
    RGBBackColor(&background);
    EraseRect(&full);

    SetRect(&body, 4, 5, 44, 44);
    RGBForeColor(&bodyColor);
    PaintOval(&body);
    RGBForeColor(&black);
    FrameOval(&body);

    SetRect(&visor, 11, 14, 37, 25);
    RGBForeColor(&highlight);
    PaintRoundRect(&visor, 5, 5);
    RGBForeColor(&black);
    FrameRoundRect(&visor, 5, 5);

    /* A pair of white engine lights makes frame changes obvious. */
    SetRect(&body, 14, 31, 21, 37);
    RGBForeColor(&white);
    PaintOval(&body);
    SetRect(&body, 27, 31, 34, 37);
    PaintOval(&body);
    RGBForeColor(&black);
    SetGWorld(savedWorld, savedDevice);
}

static void BuildSpriteMask(void)
{
    GWorldPtr savedWorld;
    GDHandle savedDevice;
    Rect full;
    Rect body;
    RGBColor black;
    RGBColor white;

    black.red = black.green = black.blue = 0;
    white.red = white.green = white.blue = 0xffff;

    GetGWorld(&savedWorld, &savedDevice);
    SetGWorld(gSpriteMask, nil);
    SetRect(&full, 0, 0, kSpriteSize, kSpriteSize);
    RGBBackColor(&white);
    EraseRect(&full);
    SetRect(&body, 4, 5, 44, 44);
    RGBForeColor(&black);
    PaintOval(&body);
    SetGWorld(savedWorld, savedDevice);
}

static void BuildSpriteDeepMask(void)
{
    GWorldPtr savedWorld;
    GDHandle savedDevice;
    Rect full;
    Rect inner;
    RGBColor black;
    RGBColor white;

    black.red = black.green = black.blue = 0;
    white.red = white.green = white.blue = 0xffff;

    GetGWorld(&savedWorld, &savedDevice);
    SetGWorld(gSpriteDeepMask, nil);
    SetRect(&full, 0, 0, kSpriteSize, kSpriteSize);
    /* The canonical 8-bit GWorld palette maps white to index 0. Keep
       transparent pixels at zero and paint the opaque deep-mask oval black,
       whose nonzero index is accepted by CopyDeepMask. */
    RGBBackColor(&white);
    EraseRect(&full);
    SetRect(&inner, 8, 8, 40, 40);
    RGBForeColor(&black);
    PaintOval(&inner);
    SetGWorld(savedWorld, savedDevice);
}

static void PrepareSpriteScene(void)
{
    GWorldPtr savedWorld;
    GDHandle savedDevice;
    PixMapHandle sourcePixels;
    PixMapHandle maskPixels;
    PixMapHandle deepMaskPixels;
    PixMapHandle worldPixels;
    Rect worldRect;
    Rect sourceRect;
    Rect firstSpriteRect;
    Rect secondSpriteRect;
    Rect stripe;
    RGBColor background;
    RGBColor stripeColor;
    RGBColor accent;
    RGBColor probe;
    RGBColor observed;
    RGBColor black;
    RGBColor white;
    Point regionProbe;
    short x;
    short y;

    gSpritePixelVerified = false;
    gSpriteRegionVerified = false;
    gSpriteUpdateRegionVerified = false;
    gSpriteRegionError = noErr;
    gSpriteScrollDelta = 0;
    gSpriteScrolled = false;
    if (!gSpriteReady) return;

    background.red = 0x1111; background.green = 0x2222; background.blue = 0x5555;
    stripeColor.red = 0x2222; stripeColor.green = 0x5555; stripeColor.blue = 0x8888;
    accent.red = gSpriteAnimated ? 0xeeee : 0x6666;
    accent.green = gSpriteAnimated ? 0xeeee : 0x2222;
    accent.blue = gSpriteAnimated ? 0x3333 : 0xdddd;
    probe.red = 0xffff; probe.green = 0x4444; probe.blue = 0x1111;
    black.red = black.green = black.blue = 0;
    white.red = white.green = white.blue = 0xffff;

    BuildSpriteSource();
    BuildSpriteMask();
    BuildSpriteDeepMask();

    GetGWorld(&savedWorld, &savedDevice);
    SetGWorld(gSpriteWorld, nil);
    SetRect(&worldRect, 0, 0, kSpriteWorldWidth, kSpriteWorldHeight);
    RGBBackColor(&background);
    EraseRect(&worldRect);

    /* Layered bands provide landmarks before and after ScrollRect. */
    RGBForeColor(&stripeColor);
    SetRect(&stripe, 0, 24, kSpriteWorldWidth, 28);
    PaintRect(&stripe);
    SetRect(&stripe, 0, 96, kSpriteWorldWidth, 100);
    PaintRect(&stripe);
    RGBForeColor(&accent);
    for (x = 0; x < kSpriteWorldWidth; x += 32) {
        MoveTo(x, 105);
        LineTo(x + 16, 105);
    }

    /* SetCPixel/GetCPixel are intentionally performed in the offscreen port. */
    SetCPixel(15, 12, &probe);
    GetCPixel(15, 12, &observed);
    gSpritePixelVerified = observed.red > 0x8000 && observed.green < 0x8000;
    for (y = 0; y < 5; y++) {
        SetCPixel(24 + y * 7, 14 + y * 3, &probe);
    }

    SetRect(&sourceRect, 0, 0, kSpriteSize, kSpriteSize);
    SetRect(&firstSpriteRect, 38, 38, 38 + kSpriteSize, 38 + kSpriteSize);
    SetRect(&secondSpriteRect, 214, 38, 214 + kSpriteSize, 38 + kSpriteSize);
    sourcePixels = GetGWorldPixMap(gSpriteSource);
    maskPixels = GetGWorldPixMap(gSpriteMask);
    deepMaskPixels = GetGWorldPixMap(gSpriteDeepMask);
    worldPixels = GetGWorldPixMap(gSpriteWorld);
    if (sourcePixels == nil || maskPixels == nil || deepMaskPixels == nil || worldPixels == nil) {
        SetGWorld(savedWorld, savedDevice);
        return;
    }

    /* Boolean source modes apply the current foreground and background
       colors. Black and white reproduce the source colors unchanged.
       Imaging With QuickDraw (1994), pp. 4-32--4-34. */
    RGBForeColor(&black);
    RGBBackColor(&white);

    /* CopyMask is a transparent, unscaled sprite transfer. */
    CopyMask((BitMap *)*sourcePixels, (BitMap *)*maskPixels, (BitMap *)*worldPixels,
             &sourceRect, &sourceRect, &firstSpriteRect);

    /* Rebuild the region from the 1-bit matte, then move it to the second sprite. */
    gSpriteRegionError = BitMapToRegion(gSpriteRegion, (BitMap *)*maskPixels);
    OffsetRgn(gSpriteRegion, secondSpriteRect.left, secondSpriteRect.top);
    regionProbe.v = secondSpriteRect.top + 24;
    regionProbe.h = secondSpriteRect.left + 24;
    gSpriteRegionVerified = gSpriteRegionError == noErr
        && !EmptyRgn(gSpriteRegion)
        && PtInRgn(regionProbe, gSpriteRegion);

    /* CopyDeepMask adds a second frame through an 8-bit mask and region clip. */
    CopyDeepMask((BitMap *)*sourcePixels, (BitMap *)*deepMaskPixels, (BitMap *)*worldPixels,
                 &sourceRect, &sourceRect, &secondSpriteRect, srcCopy, gSpriteRegion);

    SetGWorld(savedWorld, savedDevice);
}

static void ScrollSpriteScene(void)
{
    GWorldPtr savedWorld;
    GDHandle savedDevice;
    Rect scrollRect;
    Rect exposed;
    RGBColor background;
    RGBColor accent;
    Point updateProbe;

    if (!gSpriteReady) return;
    background.red = 0x1111; background.green = 0x2222; background.blue = 0x5555;
    accent.red = 0xeeee; accent.green = 0xeeee; accent.blue = 0x3333;

    GetGWorld(&savedWorld, &savedDevice);
    SetGWorld(gSpriteWorld, nil);
    SetRect(&scrollRect, 0, 0, kSpriteWorldWidth, kSpriteWorldHeight);
    SetEmptyRgn(gSpriteUpdateRegion);
    ScrollRect(&scrollRect, gSpriteScrolled ? 24 : -24, 0, gSpriteUpdateRegion);

    /* Repaint the vacated strip with a deterministic landmark after scrolling. */
    if (gSpriteScrolled) {
        SetRect(&exposed, 0, 0, 24, kSpriteWorldHeight);
    } else {
        SetRect(&exposed, kSpriteWorldWidth - 24, 0,
                kSpriteWorldWidth, kSpriteWorldHeight);
    }
    RGBForeColor(&background);
    PaintRect(&exposed);
    RGBForeColor(&accent);
    MoveTo(exposed.left + 4, 18);
    LineTo(exposed.right - 4, 18);
    MoveTo(exposed.left + 4, 110);
    LineTo(exposed.right - 4, 110);

    updateProbe.v = kSpriteWorldHeight / 2;
    updateProbe.h = gSpriteScrolled ? 12 : kSpriteWorldWidth - 12;
    gSpriteUpdateRegionVerified = PtInRgn(updateProbe, gSpriteUpdateRegion);
    gSpriteScrollDelta = gSpriteScrolled ? 24 : -24;
    gSpriteScrolled = !gSpriteScrolled;
    SetGWorld(savedWorld, savedDevice);
}

/*
 * Page 15: Events & Cursors.
 *
 * The page deliberately keeps the probes in the content region instead of
 * wrapping them in controls. That leaves the raw EventRecord available to
 * the application while it samples GetMouse, Button, StillDown, WaitMouseUp,
 * and GetKeys. Macintosh Toolbox Essentials (1992), pp. 2-50--2-71 and
 * Inside Macintosh Volume I (1985), pp. I-258--I-260.
 */
static void DrawEventKind(short what)
{
    switch (what) {
        case nullEvent:
            DrawString("\pnullEvent");
            break;
        case mouseDown:
            DrawString("\pmouseDown");
            break;
        case mouseUp:
            DrawString("\pmouseUp");
            break;
        case keyDown:
            DrawString("\pkeyDown");
            break;
        case keyUp:
            DrawString("\pkeyUp");
            break;
        case autoKey:
            DrawString("\pautoKey");
            break;
        case updateEvt:
            DrawString("\pupdateEvt");
            break;
        case activateEvt:
            DrawString("\pactivateEvt");
            break;
        default:
            DrawString("\pother event");
            break;
    }
}

static void DrawEventBoolean(Boolean value)
{
    DrawString(value ? "\pyes" : "\pno");
}

static void DrawEventModifiers(short modifiers)
{
    Boolean wrote;

    wrote = false;
    if ((modifiers & activeFlag) != 0) {
        DrawString("\pactive");
        wrote = true;
    }
    if ((modifiers & cmdKey) != 0) {
        if (wrote) DrawString("\p + ");
        DrawString("\pcommand");
        wrote = true;
    }
    if ((modifiers & shiftKey) != 0) {
        if (wrote) DrawString("\p + ");
        DrawString("\pshift");
        wrote = true;
    }
    if ((modifiers & optionKey) != 0) {
        if (wrote) DrawString("\p + ");
        DrawString("\poption");
        wrote = true;
    }
    if ((modifiers & controlKey) != 0) {
        if (wrote) DrawString("\p + ");
        DrawString("\pcontrol");
        wrote = true;
    }
    if ((modifiers & alphaLock) != 0) {
        if (wrote) DrawString("\p + ");
        DrawString("\pcaps");
        wrote = true;
    }
    if (!wrote) DrawString("\pnone");
}

static void DrawEventHexLong(long value)
{
    static const char digits[] = "0123456789ABCDEF";
    Str255 text;
    unsigned long bits;
    short i;

    bits = (unsigned long)value;
    text[0] = 8;
    for (i = 0; i < 8; i++) {
        text[i + 1] = digits[(bits >> (28 - (i * 4))) & 0x0f];
    }
    DrawString(text);
}

static void RefreshEventInput(void)
{
    Point mouse;
    KeyMap keys;
    unsigned char *keyBytes;
    short i;

    GetMouse(&mouse);
    gEventMouseV = mouse.v;
    gEventMouseH = mouse.h;
    gEventButtonDown = Button();
    gEventStillDown = StillDown();

    GetKeys(keys);
    keyBytes = (unsigned char *)keys;
    gEventKeysDown = false;
    for (i = 0; i < 16; i++) {
        if (keyBytes[i] != 0) {
            gEventKeysDown = true;
            break;
        }
    }
}

static void RecordShowcaseEvent(EventRecord *event, Boolean sampleMouse)
{
    gEventLastWhat = event->what;
    gEventLastMessage = event->message;
    gEventLastWhen = event->when;
    gEventLastWhere = event->where;
    gEventLastModifiers = event->modifiers;

    if (event->what == mouseDown) gEventMouseDownCount++;
    if (event->what == mouseUp) gEventMouseUpCount++;
    if (event->what == keyDown || event->what == autoKey) gEventKeyDownCount++;
    if (event->what == updateEvt) gEventUpdateSeen = true;
    if (event->what == activateEvt) {
        gEventActivateSeen = true;
        gEventActive = (event->modifiers & activeFlag) != 0;
    }

    if (sampleMouse) {
        RefreshEventInput();
        gEventWaitMouseUp = WaitMouseUp();
    } else if (event->what == keyDown || event->what == autoKey) {
        RefreshEventInput();
    }
}

static void DrawEventButton(const Rect *rect, ConstStr255Param label)
{
    FrameRoundRect(rect, 8, 8);
    MoveTo(rect->left + 7, rect->top + 16);
    DrawString(label);
}

static void ProbeEventQueue(void)
{
    EventRecord probe;

    /* EventAvail/OSEventAvail peek, while GetOSEvent consumes the same
     * low-level event. Macintosh Toolbox Essentials (1992), pp. 2-97--2-99. */
    gEventPostResult = PostEvent(keyDown, 0xA1B2C3D4);
    gEventPeeked = EventAvail(keyDownMask, &probe);
    gEventPeekWhat = gEventPeeked ? probe.what : nullEvent;
    gEventOSEventPeeked = OSEventAvail(keyDownMask, &probe);
    gEventOSEventPeekWhat = gEventOSEventPeeked ? probe.what : nullEvent;
    gEventTaken = GetOSEvent(keyDownMask, &probe);
    gEventTakenWhat = gEventTaken ? probe.what : nullEvent;
}

static void SetShowcaseCursor(short cursorID)
{
    CursHandle cursor;

    cursor = GetCursor(cursorID);
    if (cursor != nil) {
        SetCursor(*cursor);
        gEventCursorMode = cursorID;
    }
}

static void DrawEventsPage(void)
{
    Rect panel;
    Str255 number;

    RefreshEventInput();
    DrawHeading("\pEvents & Cursors: raw input and queue semantics");
    TextFont(applFont);
    TextSize(9);
    MoveTo(24, 52);
    DrawString("\pEventRecord fields stay beside live mouse/key state and cursor traps.");

    SetRect(&panel, 20, 62, 300, 184);
    DrawBeveledBox(&panel, false);
    TextFace(bold);
    MoveTo(28, 77);
    DrawString("\pLast EventRecord");
    TextFace(0);
    MoveTo(28, 94);
    DrawString("\pkind: ");
    DrawEventKind(gEventLastWhat);
    MoveTo(28, 110);
    DrawString("\pwhat: ");
    NumToString(gEventLastWhat, number);
    DrawString(number);
    DrawString("\p   message: $");
    DrawEventHexLong(gEventLastMessage);
    MoveTo(28, 126);
    DrawString("\pwhen: $");
    DrawEventHexLong(gEventLastWhen);
    DrawString("\p   where: (");
    NumToString(gEventLastWhere.v, number);
    DrawString(number);
    DrawString("\p, ");
    NumToString(gEventLastWhere.h, number);
    DrawString(number);
    DrawString("\p)");
    MoveTo(28, 142);
    DrawString("\pmodifiers: ");
    DrawEventModifiers(gEventLastModifiers);
    DrawString("\p [");
    NumToString(gEventLastModifiers, number);
    DrawString(number);
    DrawString("\p]");
    MoveTo(28, 158);
    DrawString("\pclick sequence down/up: ");
    NumToString(gEventMouseDownCount, number);
    DrawString(number);
    DrawString("\p/");
    NumToString(gEventMouseUpCount, number);
    DrawString(number);
    DrawString("\p   keys: ");
    NumToString(gEventKeyDownCount, number);
    DrawString(number);
    DrawString("\p keyDown");
    MoveTo(28, 174);
    DrawString("\pupdateEvt: ");
    DrawEventBoolean(gEventUpdateSeen);
    DrawString("\p   activateEvt: ");
    DrawEventBoolean(gEventActivateSeen);
    DrawString(gEventActive ? "\p (active)" : "\p (inactive)");

    SetRect(&panel, 310, 62, 535, 184);
    DrawBeveledBox(&panel, false);
    TextFace(bold);
    MoveTo(318, 77);
    DrawString("\pLive input state");
    TextFace(0);
    MoveTo(318, 96);
    DrawString("\pGetMouse local: (");
    NumToString(gEventMouseV, number);
    DrawString(number);
    DrawString("\p, ");
    NumToString(gEventMouseH, number);
    DrawString(number);
    DrawString("\p)");
    MoveTo(318, 112);
    DrawString("\pButton: ");
    DrawEventBoolean(gEventButtonDown);
    DrawString("\p   StillDown: ");
    DrawEventBoolean(gEventStillDown);
    MoveTo(318, 128);
    DrawString("\pWaitMouseUp sample: ");
    DrawEventBoolean(gEventWaitMouseUp);
    MoveTo(318, 144);
    DrawString("\pGetKeys: ");
    DrawEventBoolean(gEventKeysDown);
    DrawString("\p (nonzero map)");
    MoveTo(318, 160);
    DrawString("\pButton is physical; StillDown tracks the click.");
    MoveTo(318, 176);
    DrawString("\pWaitMouseUp consumes the ending mouseUp.");

    SetRect(&panel, 20, 194, 300, 316);
    DrawBeveledBox(&panel, false);
    TextFace(bold);
    MoveTo(28, 209);
    DrawString("\pEvent mask and queue inspection");
    TextFace(0);
    MoveTo(28, 226);
    DrawString("\pkeyDownMask: ");
    NumToString(keyDownMask, number);
    DrawString(number);
    DrawString("\p   PostEvent: ");
    NumToString(gEventPostResult, number);
    DrawString(number);
    MoveTo(28, 242);
    DrawString("\pEventAvail peek: ");
    DrawEventBoolean(gEventPeeked);
    DrawString("\p ( ");
    DrawEventKind(gEventPeekWhat);
    DrawString("\p)");
    MoveTo(28, 258);
    DrawString("\pOSEventAvail peek: ");
    DrawEventBoolean(gEventOSEventPeeked);
    DrawString("\p ( ");
    DrawEventKind(gEventOSEventPeekWhat);
    DrawString("\p)");
    MoveTo(28, 274);
    DrawString("\pGetOSEvent took: ");
    DrawEventBoolean(gEventTaken);
    DrawString("\p ( ");
    DrawEventKind(gEventTakenWhat);
    DrawString("\p)");
    SetRect(&gEventProbeRect, 28, 284, 190, 308);
    DrawEventButton(&gEventProbeRect, "\pPost / Peek / Take");

    SetRect(&panel, 310, 194, 535, 316);
    DrawBeveledBox(&panel, false);
    TextFace(bold);
    MoveTo(318, 209);
    DrawString("\pCursor Manager");
    TextFace(0);
    MoveTo(318, 226);
    DrawString("\pGetCursor / SetCursor: ");
    if (gEventCursorMode == crossCursor) {
        DrawString("\pcross");
    } else if (gEventCursorMode == watchCursor) {
        DrawString("\pwatch");
    } else {
        DrawString("\parrow");
    }
    MoveTo(318, 242);
    DrawString("\pHideCursor / ShowCursor: ");
    DrawEventBoolean(!gEventCursorHidden);
    DrawString("\p visible");
    SetRect(&gEventCrossRect, 318, 250, 382, 274);
    SetRect(&gEventWatchRect, 388, 250, 452, 274);
    SetRect(&gEventArrowRect, 458, 250, 527, 274);
    DrawEventButton(&gEventCrossRect, "\pCross");
    DrawEventButton(&gEventWatchRect, "\pWatch");
    DrawEventButton(&gEventArrowRect, "\pArrow");
    SetRect(&gEventHideRect, 318, 282, 382, 306);
    SetRect(&gEventShowRect, 388, 282, 452, 306);
    DrawEventButton(&gEventHideRect, "\pHide");
    DrawEventButton(&gEventShowRect, "\pShow");

    TextFont(applFont);
    TextSize(9);
    MoveTo(20, 337);
    DrawString("\pClick Post / Peek / Take, hold a mouse click, or hold Shift while typing.");
    MoveTo(20, 351);
    DrawString("\pCursor IDs: crossCursor = 2, watchCursor = 4; arrow comes from InitCursor.");
}

static void DrawSpritesPage(void)
{
    GWorldPtr savedWorld;
    GDHandle savedDevice;
    PixMapHandle worldPixels;
    PixMapHandle windowPixels;
    Rect worldRect;
    Rect displayRect;
    Rect panel;
    RGBColor black;
    Str255 number;

    black.red = black.green = black.blue = 0;
    DrawHeading("\pSprites, masks, and scrolling");

    SetRect(&panel, 20, 48, 360, 260);
    DrawBeveledBox(&panel, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 62);
    DrawString("\pOffscreen indexed sprite scene");
    TextFace(0);

    SetRect(&displayRect, 24, 80, 344, 208);
    if (gSpriteReady) {
        GetGWorld(&savedWorld, &savedDevice);
        SetGWorld((GWorldPtr)gMainWindow, nil);
        worldPixels = GetGWorldPixMap(gSpriteWorld);
        windowPixels = GetGWorldPixMap((GWorldPtr)gMainWindow);
        SetRect(&worldRect, 0, 0, kSpriteWorldWidth, kSpriteWorldHeight);
        if (worldPixels != nil && windowPixels != nil) {
            CopyBits((BitMap *)*worldPixels, (BitMap *)*windowPixels,
                     &worldRect, &displayRect, srcCopy, nil);
        }
        RGBForeColor(&black);
        FrameRect(&displayRect);
        SetGWorld(savedWorld, savedDevice);
    } else {
        DrawBeveledBox(&displayRect, true);
    }

    SetRect(&panel, 370, 48, 535, 260);
    DrawBeveledBox(&panel, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(378, 62);
    DrawString("\pQuickDraw pipeline");
    TextFace(0);
    MoveTo(378, 82);
    DrawString("\pCopyMask: 1-bit matte");
    MoveTo(378, 98);
    DrawString("\pCopyDeepMask: deep mask");
    MoveTo(378, 114);
    DrawString("\pBitMapToRegion: shape clip");
    MoveTo(378, 130);
    DrawString("\pSetCPixel/GetCPixel: sample");
    MoveTo(378, 154);
    DrawString("\pPixel round-trip: ");
    DrawString(gSpritePixelVerified ? "\pverified" : "\pnot verified");
    MoveTo(378, 170);
    DrawString("\pRegion round-trip: ");
    DrawString(gSpriteRegionVerified ? "\pverified" : "\pnot verified");
    MoveTo(378, 186);
    DrawString("\pUpdate region: ");
    if (gSpriteScrollDelta == 0) {
        DrawString("\pnone");
    } else {
        DrawString(gSpriteUpdateRegionVerified ? "\pverified" : "\pnot verified");
    }
    MoveTo(378, 202);
    DrawString("\pScrollRect delta: ");
    NumToString(gSpriteScrollDelta, number);
    DrawString(number);
    DrawString("\p px");
    MoveTo(378, 218);
    DrawString("\pUpdate strip: ");
    if (gSpriteScrollDelta == 0) {
        DrawString("\pnone");
    } else {
        DrawString(gSpriteScrolled ? "\pright" : "\pleft");
    }
    MoveTo(378, 234);
    DrawString(gSpriteRegionError == noErr ? "\pAll mask operations returned noErr."
                                           : "\pBitMapToRegion returned an error.");

    TextFont(applFont);
    TextSize(9);
    MoveTo(24, 280);
    DrawString("\pCopyMask preserves the scene behind the sprite; CopyDeepMask clips a second frame.");
    MoveTo(24, 294);
    DrawString("\pScroll the existing raster to expose a deterministic update region.");
    MoveTo(24, 344);
    DrawString("\pThe same source runs through the 68K indexed and PowerPC direct-color paths.");
    DrawControls(gMainWindow);
}

static void DrawMainWindow(void)
{
    RGBColor white;
    RGBColor black;

    white.red = white.green = white.blue = 0xffff;
    black.red = black.green = black.blue = 0x0000;

    SetPort(gMainWindow);
    RGBBackColor(&white);
    RGBForeColor(&black);
    EraseRect(&gMainWindow->portRect);
    switch (gPage) {
        case pageGraphics:
            DrawGraphicsPage();
            break;
        case pageControls:
            DrawControlsPage();
            break;
        case pageWindows:
            DrawWindowsPage();
            break;
        case pageDrawing:
            DrawDrawingPage();
            break;
        case pagePreferences:
            DrawPreferencesPage();
            break;
        case pageDialogs:
            DrawDialogsPage();
            break;
        case pagePalettes:
            DrawPalettesPage();
            break;
        case pageTextEdit:
            DrawTextEditPage();
            break;
        case pageLists:
            DrawListsPage();
            break;
        case pageSound:
            DrawSoundPage();
            break;
        case pageStyledText:
            DrawStyledTextPage();
            break;
        case pageStandardFile:
            DrawStandardFilePage();
            break;
        case pageResources:
            DrawResourceBrowserPage();
            break;
        case pageSprites:
            DrawSpritesPage();
            break;
        case pageEventsCursors:
            DrawEventsPage();
            break;
    }
}

static void DrawAuxWindow(WindowPtr window)
{
    Str255 sizeStr;
    Rect body;
    RGBColor fill;
    RGBColor black;
    short width, height;

    if (window == nil) {
        return;
    }

    SetPort(window);
    EraseRect(&window->portRect);
    width = window->portRect.right - window->portRect.left;
    height = window->portRect.bottom - window->portRect.top;

    if (window == gStackWindow) {
        DrawHeading("\pStacked Inspector");
        fill.red = 0xffff;
        fill.green = 0xe000;
        fill.blue = 0x5555;
    } else {
        DrawHeading("\pAuxiliary Window");
        fill.red = 0x5555;
        fill.green = 0xd000;
        fill.blue = 0xffff;
    }
    black.red = black.green = black.blue = 0x0000;

    RGBForeColor(&fill);
    SetRect(&body, 20, 40, width - 20, height - 20);
    PaintRoundRect(&body, 12, 12);
    PenNormal();
    RGBForeColor(&black);
    FrameRoundRect(&body, 12, 12);

    MoveTo(24, 68);
    if (window == gStackWindow) {
        DrawString("\pFront-to-back activation and occlusion probe.");
    } else {
        DrawString("\pMove and resize this document to expose repaint work.");
    }
    MoveTo(24, 92);
    DrawString("\pWindow dimensions: ");
    NumToString(width, sizeStr);
    DrawString(sizeStr);
    DrawString("\p x ");
    NumToString(height, sizeStr);
    DrawString(sizeStr);
    RGBForeColor(&black);
    DrawGrowIcon(window);
}

static void ShowAllControls(short page)
{
    Boolean isControls = (page == pageControls);
    Boolean isPrefs = (page == pagePreferences);
    Boolean isDialogs = (page == pageDialogs);
    Boolean isPalettes = (page == pagePalettes);
    Boolean isTextEdit = (page == pageTextEdit);
    Boolean isLists = (page == pageLists);
    Boolean isSound = (page == pageSound);
    Boolean isStandardFile = (page == pageStandardFile);
    Boolean isResources = (page == pageResources);
    Boolean isSprites = (page == pageSprites);

    /* Page 2: Controls */
    if (isControls) {
        ShowControl(gButton);
        ShowControl(gCheckbox);
        ShowControl(gScrollbar);
    } else {
        HideControl(gButton);
        HideControl(gCheckbox);
        HideControl(gScrollbar);
    }

    /* Page 5: Game Preferences */
    if (isPrefs) {
        ShowControl(gPrefSndFX);
        ShowControl(gPrefMusic);
        ShowControl(gPrefVolume);
        ShowControl(gPrefDiffEasy);
        ShowControl(gPrefDiffNormal);
        ShowControl(gPrefDiffHard);
        ShowControl(gPrefRendFlat);
        ShowControl(gPrefRendBevel);
        ShowControl(gPrefRendContrast);
        ShowControl(gPrefBtnApply);
        ShowControl(gPrefBtnReset);
        ShowControl(gPrefBtnModal);
    } else {
        HideControl(gPrefSndFX);
        HideControl(gPrefMusic);
        HideControl(gPrefVolume);
        HideControl(gPrefDiffEasy);
        HideControl(gPrefDiffNormal);
        HideControl(gPrefDiffHard);
        HideControl(gPrefRendFlat);
        HideControl(gPrefRendBevel);
        HideControl(gPrefRendContrast);
        HideControl(gPrefBtnApply);
        HideControl(gPrefBtnReset);
        HideControl(gPrefBtnModal);
    }

    /* Page 6: Dialogs */
    if (isDialogs) {
        ShowControl(gDlgBtnOpenPrefs);
        ShowControl(gDlgBtnOpenAlert);
    } else {
        HideControl(gDlgBtnOpenPrefs);
        HideControl(gDlgBtnOpenAlert);
    }

    /* Page 7: Palette Manager */
    if (isPalettes) {
        ShowControl(gPaletteAnimate);
    } else {
        HideControl(gPaletteAnimate);
    }

    /* Page 8: TextEdit */
    if (isTextEdit) {
        ShowControl(gTEJustLeft);
        ShowControl(gTEJustCenter);
        ShowControl(gTEJustRight);
        ShowControl(gTEBtnCut);
        ShowControl(gTEBtnCopy);
        ShowControl(gTEBtnPaste);
        ShowControl(gTEBtnReset);
    } else {
        HideControl(gTEJustLeft);
        HideControl(gTEJustCenter);
        HideControl(gTEJustRight);
        HideControl(gTEBtnCut);
        HideControl(gTEBtnCopy);
        HideControl(gTEBtnPaste);
        HideControl(gTEBtnReset);
    }

    /* Page 9: Lists & Inventory */
    if (isLists) {
        ShowControl(gListInspect);
        ShowControl(gListMutate);
        ShowControl(gListScroll);
        ShowControl(gListResize);
        ShowControl(gListActivate);
    } else {
        HideControl(gListInspect);
        HideControl(gListMutate);
        HideControl(gListScroll);
        HideControl(gListResize);
        HideControl(gListActivate);
    }

    /* Page 10: Sound Manager */
    if (isSound) {
        ShowControl(gSoundBtnBeep);
        ShowControl(gSoundBtnPlay);
        ShowControl(gSoundBtnQueue);
        ShowControl(gSoundBtnFlush);
        ShowControl(gSoundBtnQuiet);
        ShowControl(gSoundBtnComplete);
        ShowControl(gSoundBtnDispose);
    } else {
        HideControl(gSoundBtnBeep);
        HideControl(gSoundBtnPlay);
        HideControl(gSoundBtnQueue);
        HideControl(gSoundBtnFlush);
        HideControl(gSoundBtnQuiet);
        HideControl(gSoundBtnComplete);
        HideControl(gSoundBtnDispose);
    }

    /* Page 12: Standard File */
    if (isStandardFile) {
        ShowControl(gFileOpen);
        ShowControl(gFileLegacyOpen);
        ShowControl(gFileSave);
        ShowControl(gFileLegacySave);
    } else {
        HideControl(gFileOpen);
        HideControl(gFileLegacyOpen);
        HideControl(gFileSave);
        HideControl(gFileLegacySave);
    }

    /* Page 13: Resource Browser */
    if (isResources) {
        ShowControl(gResourceRefresh);
        ShowControl(gResourceLoad);
        ShowControl(gResourceRelease);
    } else {
        HideControl(gResourceRefresh);
        HideControl(gResourceLoad);
        HideControl(gResourceRelease);
    }

    /* Page 14: Sprites, Masks & Scrolling */
    if (isSprites) {
        ShowControl(gSpriteAnimate);
        ShowControl(gSpriteScroll);
        ShowControl(gSpriteReset);
    } else {
        HideControl(gSpriteAnimate);
        HideControl(gSpriteScroll);
        HideControl(gSpriteReset);
    }
}

static void SyncMenuState(void)
{
    MenuHandle pages;
    MenuHandle hDiff;
    MenuHandle hSound;
    MenuHandle hRend;
    short i;

    pages = GetMenuHandle(mPages);
    if (pages != nil) {
        for (i = 1; i <= 15; i++) {
            CheckItem(pages, i, gPage == i);
        }
    }

    hDiff = GetMenuHandle(mDifficulty);
    if (hDiff != nil) {
        CheckItem(hDiff, iDiffEasy, gDifficulty == iDiffEasy);
        CheckItem(hDiff, iDiffNormal, gDifficulty == iDiffNormal);
        CheckItem(hDiff, iDiffHard, gDifficulty == iDiffHard);
    }

    hSound = GetMenuHandle(mSoundMenu);
    if (hSound != nil) {
        CheckItem(hSound, iSndMute, !gSoundFX && !gMusic);
        CheckItem(hSound, iSndFXOnly, gSoundFX && !gMusic);
        CheckItem(hSound, iSndMusicOnly, !gSoundFX && gMusic);
        CheckItem(hSound, iSndFull, gSoundFX && gMusic);
    }

    hRend = GetMenuHandle(mRendererMenu);
    if (hRend != nil) {
        CheckItem(hRend, iRendFlat, gRenderer == iRendFlat);
        CheckItem(hRend, iRendBevel, gRenderer == iRendBevel);
        CheckItem(hRend, iRendContrast, gRenderer == iRendContrast);
    }

    /* Update control handles if created */
    if (gPrefSndFX != nil) {
        SetControlValue(gPrefSndFX, gSoundFX ? 1 : 0);
        SetControlValue(gPrefMusic, gMusic ? 1 : 0);
        SetControlValue(gPrefVolume, gVolume);
        SetControlValue(gPrefDiffEasy, gDifficulty == iDiffEasy ? 1 : 0);
        SetControlValue(gPrefDiffNormal, gDifficulty == iDiffNormal ? 1 : 0);
        SetControlValue(gPrefDiffHard, gDifficulty == iDiffHard ? 1 : 0);
        SetControlValue(gPrefRendFlat, gRenderer == iRendFlat ? 1 : 0);
        SetControlValue(gPrefRendBevel, gRenderer == iRendBevel ? 1 : 0);
        SetControlValue(gPrefRendContrast, gRenderer == iRendContrast ? 1 : 0);
    }

    if (gTEJustLeft != nil) {
        SetControlValue(gTEJustLeft, gTEJust == teJustLeft ? 1 : 0);
        SetControlValue(gTEJustCenter, gTEJust == teJustCenter ? 1 : 0);
        SetControlValue(gTEJustRight, gTEJust == teJustRight ? 1 : 0);
    }

    CheckItem(StateMenu(), iSoundState, gSoundCompleted);
}

static void DoModalPrefsDialog(void)
{
    DialogPtr theDialog;
    short itemHit;
    short itemType;
    Handle itemHandle;
    Rect itemRect;

    theDialog = GetNewDialog(rPrefDialog, nil, (WindowPtr)-1);
    if (theDialog == nil) {
        return;
    }
    SetPort(theDialog);
    ShowWindow(theDialog);

    do {
        ModalDialog(nil, &itemHit);
        if (itemHit == 4 || itemHit == 5) {
            GetDialogItem(theDialog, itemHit, &itemType, &itemHandle, &itemRect);
            SetControlValue((ControlHandle)itemHandle,
                            GetControlValue((ControlHandle)itemHandle) == 0 ? 1 : 0);
        }
    } while (itemHit != 1 && itemHit != 2);

    if (itemHit == 1) {
        gModalDialogCompleted = true;
    }

    DisposeDialog(theDialog);
    SetPort(gMainWindow);
    DrawMainWindow();
}

static void DoAboutAlert(void)
{
    Alert(rAboutAlert, nil);
    SetPort(gMainWindow);
    DrawMainWindow();
}

static void SetPage(short page)
{
    Rect bounds;
    Boolean wasEvents;

    wasEvents = (gPage == pageEventsCursors);

    if (gPage == pageSound && page != pageSound) {
        DisposeShowcaseSoundChannel();
    }
    if (gPage == pageTextEdit && page != pageTextEdit) {
        if (gTE != nil) TEDeactivate(gTE);
    }
    if (gPage == pagePalettes && page != pagePalettes) {
        SetPalette(gMainWindow, gOriginalPalette, true);
        ActivatePalette(gMainWindow);
    }
    if (gPage == pageLists && page != pageLists && gInventoryList != nil) {
        LActivate(false, gInventoryList);
        LSetDrawingMode(false, gInventoryList);
    }
    if (gPage == pageEventsCursors && page != pageEventsCursors) {
        /* InitCursor also resets the hide count, so a hidden showcase cursor
         * cannot leak into another page or the next visit. */
        InitCursor();
        ShowCursor();
        gEventCursorMode = 0;
        gEventCursorHidden = false;
    }
    gPage = page;
    if (page == pagePalettes) {
        SetPalette(gMainWindow, gShowcasePalette, true);
        ActivatePalette(gMainWindow);
    }
    if (page == pageTextEdit) {
        if (gTE != nil) TEActivate(gTE);
    }
    if (page == pageLists && gInventoryList != nil) {
        gListActive = true;
        LSetDrawingMode(true, gInventoryList);
        LActivate(true, gInventoryList);
    }
    if (page == pageSound) {
        EnsureShowcaseSoundChannel();
    }
    if (page == pageResources) {
        PrepareResourceBrowser();
    }
    if (page == pageSprites) {
        PrepareSpriteScene();
    }
    if (page == pageEventsCursors && !wasEvents) {
        InitCursor();
        ShowCursor();
        gEventCursorMode = 0;
        gEventCursorHidden = false;
    }
    ShowAllControls(page);
    SyncMenuState();

    if (page == pageWindows && gAuxWindow == nil && gStackWindow == nil) {
        SetRect(&bounds, 180, 155, 500, 400);
        gAuxWindow = NewCWindow(nil, &bounds, "\pAuxiliary Window", true,
                                zoomDocProc, (WindowPtr)-1, true, 1);
        SetRect(&bounds, 330, 250, 575, 495);
        gStackWindow = NewCWindow(nil, &bounds, "\pStacked Inspector", true,
                                  zoomDocProc, (WindowPtr)-1, true, 2);
        CheckItem(StateMenu(), iWindowState,
                  gAuxWindow != nil && gStackWindow != nil);
        DrawAuxWindow(gAuxWindow);
        DrawAuxWindow(gStackWindow);
    } else if (page != pageWindows) {
        /* Dispose front-to-back so each CloseWindow promotes its predecessor. */
        if (gStackWindow != nil) {
            DisposeWindow(gStackWindow);
            gStackWindow = nil;
        }
        if (gAuxWindow != nil) {
            DisposeWindow(gAuxWindow);
            gAuxWindow = nil;
        }
        CheckItem(StateMenu(), iWindowState, false);
    }

    DrawMainWindow();
}

static void Initialize(void)
{
    Handle menuBar;
    MenuHandle hMenu;
    Rect r;
    Rect dataBounds;
    Rect sourceRect;
    Point cellSize;

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

    /* Insert hierarchical submenus into the hierarchical partition (-1) */
    hMenu = GetMenu(mOptions);
    if (hMenu != nil) InsertMenu(hMenu, -1);
    hMenu = GetMenu(mDifficulty);
    if (hMenu != nil) InsertMenu(hMenu, -1);
    hMenu = GetMenu(mSoundMenu);
    if (hMenu != nil) InsertMenu(hMenu, -1);
    hMenu = GetMenu(mRendererMenu);
    if (hMenu != nil) InsertMenu(hMenu, -1);

    DrawMenuBar();

    gMainWindow = GetNewCWindow(rMainWindow, nil, (WindowPtr)-1);
    if (gMainWindow == nil) {
        ExitToShell();
    }
    SetPort(gMainWindow);
    TextFont(applFont);
    TextSize(9);
    ShowWindow(gMainWindow);

    gOriginalPalette = GetPalette(gMainWindow);
    if (gOriginalPalette == nil) {
        gOriginalPalette = GetNewPalette(0);
    }
    gShowcasePalette = GetNewPalette(rShowcasePalette);
    SetPalette(gMainWindow, gOriginalPalette, true);
    ActivatePalette(gMainWindow);

    /* Page 2: Controls */
    SetRect(&r, 40, 255, 150, 279);
    gButton = NewControl(gMainWindow, &r, "\pActivate", false, 0, 0, 1,
                         pushButProc, 0);
    SetRect(&r, 185, 255, 315, 279);
    gCheckbox = NewControl(gMainWindow, &r, "\pCheckbox", false, 0, 0, 1,
                           checkBoxProc, 0);
    SetRect(&r, 40, 310, 500, 326);
    gScrollbar = NewControl(gMainWindow, &r, "\p", false, 0, 0, 10,
                            scrollBarProc, 0);

    /* Page 5: Game Preferences Controls */
    SetRect(&r, 35, 70, 210, 90);
    gPrefSndFX = NewControl(gMainWindow, &r, "\pSound Effects (SFX)", false, 1, 0, 1,
                            checkBoxProc, 0);
    SetRect(&r, 35, 95, 210, 115);
    gPrefMusic = NewControl(gMainWindow, &r, "\pBackground Music", false, 1, 0, 1,
                            checkBoxProc, 0);
    SetRect(&r, 35, 195, 215, 211);
    gPrefVolume = NewControl(gMainWindow, &r, "\p", false, 75, 0, 100,
                             scrollBarProc, 0);

    SetRect(&r, 250, 70, 390, 90);
    gPrefDiffEasy = NewControl(gMainWindow, &r, "\pRecruit (Easy)", false, 0, 0, 1,
                               radioButProc, 0);
    SetRect(&r, 250, 95, 390, 115);
    gPrefDiffNormal = NewControl(gMainWindow, &r, "\pVeteran (Normal)", false, 1, 0, 1,
                                 radioButProc, 0);
    SetRect(&r, 250, 120, 390, 140);
    gPrefDiffHard = NewControl(gMainWindow, &r, "\pNightmare (Hard)", false, 0, 0, 1,
                               radioButProc, 0);

    SetRect(&r, 250, 195, 420, 215);
    gPrefRendFlat = NewControl(gMainWindow, &r, "\pClassic 2D Flat", false, 0, 0, 1,
                               radioButProc, 0);
    SetRect(&r, 250, 215, 420, 235);
    gPrefRendBevel = NewControl(gMainWindow, &r, "\pQD3D Bevels (Emul.)", false, 1, 0, 1,
                                radioButProc, 0);
    SetRect(&r, 250, 235, 420, 255);
    gPrefRendContrast = NewControl(gMainWindow, &r, "\pHigh Contrast", false, 0, 0, 1,
                                   radioButProc, 0);

    SetRect(&r, 35, 315, 145, 339);
    gPrefBtnApply = NewControl(gMainWindow, &r, "\pSave & Apply", false, 0, 0, 1,
                               pushButProc, 0);
    SetRect(&r, 160, 315, 270, 339);
    gPrefBtnReset = NewControl(gMainWindow, &r, "\pReset Defaults", false, 0, 0, 1,
                               pushButProc, 0);
    SetRect(&r, 285, 315, 425, 339);
    gPrefBtnModal = NewControl(gMainWindow, &r, "\pModal Dialog\311", false, 0, 0, 1,
                               pushButProc, 0);

    /* Page 6: Dialogs Controls */
    SetRect(&r, 40, 305, 220, 329);
    gDlgBtnOpenPrefs = NewControl(gMainWindow, &r, "\pOpen Modal Dialog\311", false, 0, 0, 1,
                                  pushButProc, 0);
    SetRect(&r, 240, 305, 410, 329);
    gDlgBtnOpenAlert = NewControl(gMainWindow, &r, "\pDisplay About Alert…", false, 0, 0, 1,
                                  pushButProc, 0);

    /* Page 7: Palette Manager Control */
    SetRect(&r, 40, 342, 230, 366);
    gPaletteAnimate = NewControl(gMainWindow, &r, "\pAnimate Palette", false, 0, 0, 1,
                                 pushButProc, 0);

    /* Page 8: TextEdit Controls */
    SetRect(&gTERect, 34, 76, 326, 211);
    gTE = TENew(&gTERect, &gTERect);
    if (gTE != nil) {
        TESetText((const void *)kTESampleText, sizeof(kTESampleText) - 1, gTE);
        TESetSelect(0, 0, gTE);
    }

    SetRect(&r, 360, 168, 412, 188);
    gTEJustLeft = NewControl(gMainWindow, &r, "\pLeft", false, 1, 0, 1,
                             radioButProc, 0);
    SetRect(&r, 416, 168, 474, 188);
    gTEJustCenter = NewControl(gMainWindow, &r, "\pCenter", false, 0, 0, 1,
                               radioButProc, 0);
    SetRect(&r, 478, 168, 530, 188);
    gTEJustRight = NewControl(gMainWindow, &r, "\pRight", false, 0, 0, 1,
                              radioButProc, 0);

    SetRect(&r, 28, 256, 100, 280);
    gTEBtnCut = NewControl(gMainWindow, &r, "\pCut", false, 0, 0, 1,
                           pushButProc, 0);
    SetRect(&r, 106, 256, 178, 280);
    gTEBtnCopy = NewControl(gMainWindow, &r, "\pCopy", false, 0, 0, 1,
                            pushButProc, 0);
    SetRect(&r, 184, 256, 256, 280);
    gTEBtnPaste = NewControl(gMainWindow, &r, "\pPaste", false, 0, 0, 1,
                             pushButProc, 0);
    SetRect(&r, 262, 256, 332, 280);
    gTEBtnReset = NewControl(gMainWindow, &r, "\pReset", false, 0, 0, 1,
                             pushButProc, 0);

    /* Page 9: Lists & Inventory */
    SetRect(&gInventoryView, 24, 78, 528, 228);
    SetRect(&dataBounds, 0, 0, 1, 1);
    cellSize.v = 18;
    cellSize.h = 504;
    gInventoryList = LNew(&gInventoryView, &dataBounds, cellSize, 0,
                          gMainWindow, false, true, false, true);
    PopulateInventoryList();
    if (gInventoryList != nil) LSetDrawingMode(false, gInventoryList);

    SetRect(&r, 24, 242, 135, 266);
    gListInspect = NewControl(gMainWindow, &r, "\pInspect Selection", false, 0, 0, 1,
                              pushButProc, 0);
    SetRect(&r, 143, 242, 270, 266);
    gListMutate = NewControl(gMainWindow, &r, "\pUpdate Selected Row", false, 0, 0, 1,
                             pushButProc, 0);
    SetRect(&r, 278, 242, 390, 266);
    gListScroll = NewControl(gMainWindow, &r, "\pScroll Four Rows", false, 0, 0, 1,
                             pushButProc, 0);
    SetRect(&r, 398, 242, 514, 266);
    gListResize = NewControl(gMainWindow, &r, "\pResize List", false, 0, 0, 1,
                             pushButProc, 0);
    SetRect(&r, 24, 274, 170, 298);
    gListActivate = NewControl(gMainWindow, &r, "\pToggle Activation", false, 0, 0, 1,
                               pushButProc, 0);

    /* Page 10: Sound Manager Controls */
    SetRect(&r, 20, 255, 105, 279);
    gSoundBtnBeep = NewControl(gMainWindow, &r, "\pSysBeep", false, 0, 0, 1,
                               pushButProc, 0);
    SetRect(&r, 110, 255, 205, 279);
    gSoundBtnPlay = NewControl(gMainWindow, &r, "\pPlay snd", false, 0, 0, 1,
                               pushButProc, 0);
    SetRect(&r, 210, 255, 315, 279);
    gSoundBtnQueue = NewControl(gMainWindow, &r, "\pQueue cmds", false, 0, 0, 1,
                                pushButProc, 0);
    SetRect(&r, 320, 255, 385, 279);
    gSoundBtnFlush = NewControl(gMainWindow, &r, "\pFlush", false, 0, 0, 1,
                                pushButProc, 0);
    SetRect(&r, 390, 255, 455, 279);
    gSoundBtnQuiet = NewControl(gMainWindow, &r, "\pQuiet", false, 0, 0, 1,
                                pushButProc, 0);
    SetRect(&r, 20, 290, 145, 314);
    gSoundBtnComplete = NewControl(gMainWindow, &r, "\pPlay to complete", false, 0, 0, 1,
                                   pushButProc, 0);
    SetRect(&r, 150, 290, 250, 314);
    gSoundBtnDispose = NewControl(gMainWindow, &r, "\pDispose", false, 0, 0, 1,
                                  pushButProc, 0);

    /* Page 11: Styled TextEdit & Font Manager */
    InitializeStyledText();

    /* Page 12: Standard File */
    gFileTypeList[0] = 'TEXT';
    gFileTypeList[1] = 0;
    gFileTypeList[2] = 0;
    gFileTypeList[3] = 0;
    SetRect(&r, 24, 204, 148, 228);
    gFileOpen = NewControl(gMainWindow, &r, "\pOpen TEXT", false, 0, 0, 1,
                           pushButProc, 0);
    SetRect(&r, 158, 204, 294, 228);
    gFileLegacyOpen = NewControl(gMainWindow, &r, "\pLegacy Open", false, 0, 0, 1,
                                 pushButProc, 0);
    SetRect(&r, 304, 204, 416, 228);
    gFileSave = NewControl(gMainWindow, &r, "\pSave As", false, 0, 0, 1,
                           pushButProc, 0);
    SetRect(&r, 426, 204, 535, 228);
    gFileLegacySave = NewControl(gMainWindow, &r, "\pLegacy Save", false, 0, 0, 1,
                                 pushButProc, 0);

    /* Page 13: Resource Browser */
    SetRect(&r, 24, 252, 132, 276);
    gResourceRefresh = NewControl(gMainWindow, &r, "\pRefresh Map", false, 0, 0, 1,
                                  pushButProc, 0);
    SetRect(&r, 140, 252, 260, 276);
    gResourceLoad = NewControl(gMainWindow, &r, "\pLoad Named", false, 0, 0, 1,
                               pushButProc, 0);
    SetRect(&r, 268, 252, 388, 276);
    gResourceRelease = NewControl(gMainWindow, &r, "\pRelease Handle", false, 0, 0, 1,
                                  pushButProc, 0);

    /* Page 14: Sprites, Masks & Scrolling */
    SetRect(&dataBounds, 0, 0, kSpriteWorldWidth, kSpriteWorldHeight);
    SetRect(&sourceRect, 0, 0, kSpriteSize, kSpriteSize);
    gSpriteWorld = nil;
    gSpriteSource = nil;
    gSpriteMask = nil;
    gSpriteDeepMask = nil;
    gSpriteRegion = NewRgn();
    gSpriteUpdateRegion = NewRgn();
    if (NewGWorld(&gSpriteWorld, 8, &dataBounds, nil, nil, 0) == noErr
        && NewGWorld(&gSpriteSource, 8, &sourceRect, nil, nil, 0) == noErr
        && NewGWorld(&gSpriteMask, 1, &sourceRect, nil, nil, 0) == noErr
        && NewGWorld(&gSpriteDeepMask, 8, &sourceRect, nil, nil, 0) == noErr
        && gSpriteRegion != nil && gSpriteUpdateRegion != nil) {
        gSpriteReady = true;
    }
    SetRect(&r, 35, 304, 145, 328);
    gSpriteAnimate = NewControl(gMainWindow, &r, "\pAnimate Sprite", false, 0, 0, 1,
                                pushButProc, 0);
    SetRect(&r, 153, 304, 263, 328);
    gSpriteScroll = NewControl(gMainWindow, &r, "\pScroll Scene", false, 0, 0, 1,
                               pushButProc, 0);
    SetRect(&r, 271, 304, 365, 328);
    gSpriteReset = NewControl(gMainWindow, &r, "\pReset Scene", false, 0, 0, 1,
                              pushButProc, 0);

    SetPage(pageGraphics);
}

static void DoMenuChoice(long choice)
{
    short menuID;
    short item;

    menuID = HiWord(choice);
    item = LoWord(choice);

    if (menuID == mPages && item >= iGraphics && item <= iEventsCursors) {
        SetPage(item);
    } else if (menuID == mDifficulty) {
        gDifficulty = item;
        SyncMenuState();
        DrawMainWindow();
    } else if (menuID == mSoundMenu) {
        if (item == iSndMute) {
            gSoundFX = false;
            gMusic = false;
        } else if (item == iSndFXOnly) {
            gSoundFX = true;
            gMusic = false;
        } else if (item == iSndMusicOnly) {
            gSoundFX = false;
            gMusic = true;
        } else if (item == iSndFull) {
            gSoundFX = true;
            gMusic = true;
        }
        SyncMenuState();
        DrawMainWindow();
    } else if (menuID == mRendererMenu) {
        gRenderer = item;
        SyncMenuState();
        DrawMainWindow();
    } else if (menuID == mOptions) {
        if (item == iOptResetPrefs) {
            gDifficulty = iDiffNormal;
            gSoundFX = true;
            gMusic = true;
            gVolume = 75;
            gRenderer = iRendBevel;
            SyncMenuState();
            DrawMainWindow();
        } else if (item == iOptLaunchDialog) {
            DoModalPrefsDialog();
        }
    } else if (menuID == mFile) {
        if (item == iFilePrefs) {
            SetPage(pagePreferences);
        } else if (item == iQuit) {
            gQuit = true;
        }
    } else if (menuID == mApple) {
        if (item == iAbout) {
            DoAboutAlert();
        }
    }
    HiliteMenu(0);
}

static void DoContentClick(WindowPtr window, EventRecord *event)
{
    ControlHandle control;
    short part;
    short trackedPart;
    short value;
    Point where;
    Rect listHit;

    if (window != gMainWindow) {
        return;
    }
    SetPort(window);
    where = event->where;
    GlobalToLocal(&where);

    if (gPage == pageTextEdit && gTE != nil && PtInRect(where, &gTERect)) {
        Boolean shift = (event->modifiers & shiftKey) != 0;
        TEClick(where, shift, gTE);
        DrawMainWindow();
        return;
    }

    if (gPage == pageLists && gInventoryList != nil && gListActive) {
        /* LClick also accepts the List Manager scroll-bar strip. */
        listHit = gInventoryView;
        listHit.top -= 1;
        listHit.bottom += 1;
        listHit.right += 16;
        if (PtInRect(where, &listHit)) {
            LClick(where, event->modifiers, gInventoryList);
            InspectInventorySelection();
            DrawMainWindow();
            return;
        }
    }

    if (gPage == pageEventsCursors) {
        if (PtInRect(where, &gEventProbeRect)) {
            ProbeEventQueue();
        } else if (PtInRect(where, &gEventCrossRect)) {
            SetShowcaseCursor(crossCursor);
        } else if (PtInRect(where, &gEventWatchRect)) {
            SetShowcaseCursor(watchCursor);
        } else if (PtInRect(where, &gEventArrowRect)) {
            InitCursor();
            gEventCursorMode = 0;
            gEventCursorHidden = false;
        } else if (PtInRect(where, &gEventHideRect)) {
            if (!gEventCursorHidden) {
                HideCursor();
                gEventCursorHidden = true;
            }
        } else if (PtInRect(where, &gEventShowRect)) {
            if (gEventCursorHidden) {
                ShowCursor();
                gEventCursorHidden = false;
            }
        }
        DrawMainWindow();
        return;
    }

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
    } else if (control == gPrefSndFX) {
        gSoundFX = !gSoundFX;
        SyncMenuState();
    } else if (control == gPrefMusic) {
        gMusic = !gMusic;
        SyncMenuState();
    } else if (control == gPrefVolume) {
        value = GetControlValue(gPrefVolume);
        if (part == kControlUpButtonPart || part == kControlPageUpPart) {
            value -= part == kControlPageUpPart ? 15 : 5;
        } else if (part == kControlDownButtonPart || part == kControlPageDownPart) {
            value += part == kControlPageDownPart ? 15 : 5;
        }
        if (value < 0) value = 0;
        if (value > 100) value = 100;
        gVolume = value;
        SetControlValue(gPrefVolume, gVolume);
    } else if (control == gPrefDiffEasy) {
        gDifficulty = iDiffEasy;
        SyncMenuState();
    } else if (control == gPrefDiffNormal) {
        gDifficulty = iDiffNormal;
        SyncMenuState();
    } else if (control == gPrefDiffHard) {
        gDifficulty = iDiffHard;
        SyncMenuState();
    } else if (control == gPrefRendFlat) {
        gRenderer = iRendFlat;
        SyncMenuState();
    } else if (control == gPrefRendBevel) {
        gRenderer = iRendBevel;
        SyncMenuState();
    } else if (control == gPrefRendContrast) {
        gRenderer = iRendContrast;
        SyncMenuState();
    } else if (control == gPrefBtnApply) {
        /* Save / Apply feedback */
        SyncMenuState();
    } else if (control == gPrefBtnReset) {
        gDifficulty = iDiffNormal;
        gSoundFX = true;
        gMusic = true;
        gVolume = 75;
        gRenderer = iRendBevel;
        SyncMenuState();
    } else if (control == gPrefBtnModal || control == gDlgBtnOpenPrefs) {
        DoModalPrefsDialog();
        return;
    } else if (control == gDlgBtnOpenAlert) {
        DoAboutAlert();
        return;
    } else if (control == gPaletteAnimate) {
        AnimateShowcasePalette();
        return;
    } else if (control == gSoundBtnBeep) {
        SysBeep(30);
        gSoundBeeped = true;
    } else if (control == gSoundBtnPlay) {
        PlayShowcaseSound(false);
    } else if (control == gSoundBtnQueue) {
        QueueShowcaseSoundCommands();
    } else if (control == gSoundBtnFlush) {
        FlushShowcaseSound();
    } else if (control == gSoundBtnQuiet) {
        QuietShowcaseSound();
    } else if (control == gSoundBtnComplete) {
        PlayShowcaseSound(true);
    } else if (control == gSoundBtnDispose) {
        DisposeShowcaseSoundChannel();
    } else if (control == gTEJustLeft) {
        gTEJust = teJustLeft;
        TESetAlignment(teJustLeft, gTE);
        SyncMenuState();
    } else if (control == gTEJustCenter) {
        gTEJust = teJustCenter;
        TESetAlignment(teJustCenter, gTE);
        SyncMenuState();
    } else if (control == gTEJustRight) {
        gTEJust = teJustRight;
        TESetAlignment(teJustRight, gTE);
        SyncMenuState();
    } else if (control == gTEBtnCut) {
        TECut(gTE);
    } else if (control == gTEBtnCopy) {
        TECopy(gTE);
    } else if (control == gTEBtnPaste) {
        TEPaste(gTE);
    } else if (control == gTEBtnReset) {
        TESetText((const void *)kTESampleText, sizeof(kTESampleText) - 1, gTE);
        TESetSelect(0, 14, gTE);
    } else if (control == gListInspect) {
        InspectInventorySelection();
    } else if (control == gListMutate) {
        MutateInventorySelection();
    } else if (control == gListScroll) {
        ScrollInventoryList();
    } else if (control == gListResize) {
        ResizeInventoryList();
    } else if (control == gListActivate) {
        ToggleInventoryActivation();
    } else if (control == gFileOpen) {
        DoStandardFileOpen();
        DrawMainWindow();
        return;
    } else if (control == gFileLegacyOpen) {
        DoLegacyFileOpen();
        DrawMainWindow();
        return;
    } else if (control == gFileSave) {
        DoStandardFileSave();
        DrawMainWindow();
        return;
    } else if (control == gFileLegacySave) {
        DoLegacyFileSave();
        DrawMainWindow();
        return;
    } else if (control == gResourceRefresh) {
        RefreshResourceBrowser();
    } else if (control == gResourceLoad) {
        LoadNamedResourceBrowserEntry();
    } else if (control == gResourceRelease) {
        ReleaseNamedResourceBrowserEntry();
    } else if (control == gSpriteAnimate) {
        gSpriteAnimated = !gSpriteAnimated;
        PrepareSpriteScene();
    } else if (control == gSpriteScroll) {
        ScrollSpriteScene();
    } else if (control == gSpriteReset) {
        gSpriteAnimated = false;
        PrepareSpriteScene();
    }
    DrawMainWindow();
}

static void DoEvent(EventRecord *event)
{
    WindowPtr window;
    short part;
    char key;

    RecordShowcaseEvent(event,
                        gPage == pageEventsCursors && event->what == mouseDown);

    switch (event->what) {
        case mouseDown:
            part = FindWindow(event->where, &window);
            if (part == inMenuBar) {
                DoMenuChoice(MenuSelect(event->where));
            } else if (part == inContent) {
                if (window != FrontWindow()) {
                    SelectWindow(window);
                    if (window == gAuxWindow || window == gStackWindow) {
                        /* Selecting a covered document exposes its content;
                         * repaint the newly frontmost visible region before
                         * the next event-loop frame. */
                        DrawAuxWindow(window);
                    }
                } else {
                    DoContentClick(window, event);
                }
            } else if (part == inDrag) {
                DragWindow(window, event->where, &qd.screenBits.bounds);
                if (window == gAuxWindow || window == gStackWindow) {
                    /* DragWindow changes the shared screen-backed port
                     * geometry; redraw the moved content immediately so the
                     * newly exposed and newly covered regions agree. */
                    DrawAuxWindow(window);
                }
            } else if (part == inGrow) {
                Rect sizeRect;
                long growResult;
                SetRect(&sizeRect, 180, 100, 520, 350);
                growResult = GrowWindow(window, event->where, &sizeRect);
                if (growResult != 0) {
                    SizeWindow(window, LoWord(growResult), HiWord(growResult), true);
                    SetPort(window);
                    InvalRect(&window->portRect);
                    if (window == gAuxWindow || window == gStackWindow) {
                        /* The updateEvt remains queued for normal event-loop
                         * semantics, while this redraw makes the resized
                         * edge deterministic before the next frame. */
                        DrawAuxWindow(window);
                    }
                }
            } else if ((part == inZoomIn || part == inZoomOut) && TrackBox(window, event->where, part)) {
                ZoomWindow(window, part, window == FrontWindow());
                SetPort(window);
                InvalRect(&window->portRect);
            } else if (part == inGoAway && TrackGoAway(window, event->where)) {
                if (window == gStackWindow) {
                    DisposeWindow(gStackWindow);
                    gStackWindow = nil;
                    /* The promoted auxiliary window may have been partly
                     * covered; redraw it explicitly so CloseWindow's
                     * exposure is visible before the next event-loop pass. */
                    if (gAuxWindow != nil) {
                        DrawAuxWindow(gAuxWindow);
                    }
                    CheckItem(StateMenu(), iWindowState,
                              gAuxWindow != nil);
                } else if (window == gAuxWindow) {
                    DisposeWindow(gAuxWindow);
                    gAuxWindow = nil;
                    /* The main page is exposed after the final auxiliary
                     * window closes. Keep the promotion deterministic on
                     * both the indexed and direct-color paths. */
                    SetPort(gMainWindow);
                    DrawMainWindow();
                    CheckItem(StateMenu(), iWindowState,
                              gStackWindow != nil);
                } else {
                    gQuit = true;
                }
            }
            break;

        case activateEvt:
            window = (WindowPtr)event->message;
            if (window == gMainWindow && gInventoryList != nil) {
                gListActive = (event->modifiers & activeFlag) != 0;
                LActivate(gListActive, gInventoryList);
                if (gPage == pageLists) DrawMainWindow();
            }
            break;

        case keyDown:
        case autoKey:
            key = (char)(event->message & charCodeMask);
            if ((event->modifiers & cmdKey) != 0) {
                DoMenuChoice(MenuKey(key));
            } else if (gPage == pageTextEdit && gTE != nil) {
                TEKey(key, gTE);
                DrawMainWindow();
            } else if (gPage == pageEventsCursors) {
                DrawMainWindow();
            }
            break;

        case updateEvt:
            window = (WindowPtr)event->message;
            BeginUpdate(window);
            if (window == gMainWindow) {
                DrawMainWindow();
            } else if (window == gAuxWindow) {
                DrawAuxWindow(gAuxWindow);
            } else if (window == gStackWindow) {
                DrawAuxWindow(gStackWindow);
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
        } else if (gPage == pageTextEdit && gTE != nil) {
            TEIdle(gTE);
        } else if (gPage == pageSound) {
            PollShowcaseSound();
        }
    }
    DisposeShowcaseSoundChannel();
    SetPalette(gMainWindow, nil, true);
    if (gShowcasePalette != nil) DisposePalette(gShowcasePalette);
    if (gOriginalPalette != nil) DisposePalette(gOriginalPalette);
    if (gTE != nil) TEDispose(gTE);
    if (gStyledTE != nil) TEDispose(gStyledTE);
    if (gInventoryList != nil) {
        LActivate(false, gInventoryList);
        LDispose(gInventoryList);
    }
    ExitToShell();
}
