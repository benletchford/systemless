#include "Types.r"
#include "CodeFragmentTypes.r"
#include "Processes.r"
#include "Menus.r"
#include "Windows.r"
#include "Dialogs.r"
#include "Palettes.r"

#define rMenuBar 128
#define rMainWindow 128
#define rPrefDialog 129
#define rAboutAlert 130
#define rShowcaseIcon 128
#define rShowcasePalette 150
#define rShowcaseSound 151
#define pmAnimatedExplicit 12

#define mApple 128
#define mPages 129
#define mState 130
#define mFile 131
#define mOptions 132

#define mDifficulty 140
#define mSoundMenu 141
#define mRendererMenu 142

#ifdef FAT
Include "showcase.68k" 'CODE';
#endif

#ifdef PowerPC
resource 'cfrg' (0) {
    {
        kPowerPC,
        kFullLib,
        kNoVersionNum,
        kNoVersionNum,
        kDefaultStackSize,
        kNoAppSubFolder,
        kIsApp,
        kOnDiskFlat,
        kZeroOffset,
        kWholeFork,
        "Toolbox Showcase"
    }
};
#endif

resource 'SIZE' (-1) {
    dontSaveScreen,
    acceptSuspendResumeEvents,
    enableOptionSwitch,
    canBackground,
    multiFinderAware,
    backgroundAndForeground,
    dontGetFrontClicks,
    ignoreChildDiedEvents,
    is32BitCompatible,
    isHighLevelEventAware,
    onlyLocalHLEvents,
    notStationeryAware,
    dontUseTextEditServices,
    reserved,
    reserved,
    reserved,
    2 * 1024 * 1024,
    1 * 1024 * 1024
};

resource 'MBAR' (rMenuBar, preload) {
    { mApple, mPages, mState, mFile };
};

resource 'MENU' (mApple, preload) {
    mApple, textMenuProc, allEnabled, enabled, apple,
    {
        "About Toolbox Showcase…", noIcon, noKey, noMark, plain
    }
};

resource 'MENU' (mPages, preload) {
    mPages, textMenuProc, allEnabled, enabled, "Pages",
    {
        "Graphics", noIcon, noKey, check, plain;
        "Controls", noIcon, noKey, noMark, plain;
        "Windows", noIcon, noKey, noMark, plain;
        "Drawing & 3D Bevels", noIcon, noKey, noMark, plain;
        "Game Preferences", noIcon, noKey, noMark, plain;
        "Dialogs & Alerts", noIcon, noKey, noMark, plain;
        "TextEdit", noIcon, noKey, noMark, plain;
        "Palettes", noIcon, noKey, noMark, plain;
        "Lists & Inventory", noIcon, noKey, noMark, plain;
        "Sound & Channels", noIcon, noKey, noMark, plain;
        "Styled Text & Fonts", noIcon, noKey, noMark, plain;
        "Standard File", noIcon, noKey, noMark, plain
    }
};

/*
 * The first three colored entries combine pmAnimated with pmExplicit, making
 * their palette positions stable CLUT indexes. The remaining colors exercise
 * tolerant allocation. Inside Macintosh Volume VI (1991), pp. 20-14..20-16.
 */
resource 'pltt' (rShowcasePalette, "Showcase Palette") {
    {
        65535, 65535, 65535, pmTolerant, 0,
        0, 0, 0, pmTolerant, 0,
        65535, 65535, 0, pmAnimatedExplicit, 0,
        65535, 26214, 0, pmAnimatedExplicit, 0,
        0, 52428, 43690, pmAnimatedExplicit, 0,
        0, 50000, 50000, pmTolerant, 0,
        22015, 0, 0, pmTolerant, 0,
        56683, 2242, 1698, pmTolerant, 0,
        49152, 49152, 49152, pmTolerant, 0,
        32768, 32768, 32768, pmTolerant, 0,
        65535, 50140, 33120, pmTolerant, 0,
        37887, 10266, 4812, pmTolerant, 0,
        25892, 49919, 0, pmTolerant, 0,
        0, 65535, 1265, pmTolerant, 0,
        0, 0, 65535, pmTolerant, 0,
        32768, 0, 65535, pmTolerant, 0
    }
};

/*
 * A deterministic format-1 sampled sound: one bufferCmd points at a standard
 * sound header followed by 256 unsigned 8-bit mono samples. The repeating
 * sixteen-sample waveform is intentionally small but long enough to expose
 * active playback in the fixture's audio mixer. Sound (1994), pp. 2-76--2-77.
 */
data 'snd ' (rShowcaseSound, preload) {
    $"0001 0000 0001 8051 0000 0000 000E"
    $"0000 0000 0000 0100 56EE 8BA3 0000 0000"
    $"0000 0000 003C"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
    $"80A0 C0E0 F0E0 C0A0 8060 4020 1020 4060"
};

resource 'MENU' (mState, preload) {
    mState, textMenuProc, allEnabled, enabled, "State",
    {
        "Button activated", noIcon, noKey, noMark, plain;
        "Checkbox selected", noIcon, noKey, noMark, plain;
        "Scrollbar moved", noIcon, noKey, noMark, plain;
        "Auxiliary window open", noIcon, noKey, noMark, plain;
        "Sound completion received", noIcon, noKey, noMark, plain
    }
};

resource 'MENU' (mFile, preload) {
    mFile, textMenuProc, allEnabled, enabled, "File",
    {
        "Preferences…", noIcon, "P", noMark, plain;
        "Game Options", noIcon, hierarchicalMenu, "\204", plain;
        "-", noIcon, noKey, noMark, plain;
        "Quit", noIcon, "Q", noMark, plain
    }
};

resource 'MENU' (mOptions, preload) {
    mOptions, textMenuProc, allEnabled, enabled, "Options",
    {
        "Difficulty", noIcon, hierarchicalMenu, "\214", plain;
        "Sound Configuration", noIcon, hierarchicalMenu, "\215", plain;
        "Renderer Style", noIcon, hierarchicalMenu, "\216", plain;
        "-", noIcon, noKey, noMark, plain;
        "Reset All Preferences", noIcon, "R", noMark, plain;
        "Launch Modal Dialog…", noIcon, "D", noMark, plain
    }
};

resource 'MENU' (mDifficulty, preload) {
    mDifficulty, textMenuProc, allEnabled, enabled, "Difficulty",
    {
        "Recruit (Easy)", noIcon, noKey, noMark, plain;
        "Veteran (Normal)", noIcon, noKey, check, plain;
        "Nightmare (Hard)", noIcon, noKey, noMark, plain
    }
};

resource 'MENU' (mSoundMenu, preload) {
    mSoundMenu, textMenuProc, allEnabled, enabled, "Sound",
    {
        "Mute All", noIcon, noKey, noMark, plain;
        "Sound Effects Only", noIcon, noKey, noMark, plain;
        "Music Only", noIcon, noKey, noMark, plain;
        "Full Audio (FX + Music)", noIcon, noKey, check, plain
    }
};

resource 'MENU' (mRendererMenu, preload) {
    mRendererMenu, textMenuProc, allEnabled, enabled, "Renderer",
    {
        "Classic 2D Flat", noIcon, noKey, noMark, plain;
        "QD3D Beveled (Emulated)", noIcon, noKey, check, plain;
        "High Contrast Outlines", noIcon, noKey, noMark, plain
    }
};

resource 'WIND' (rMainWindow, preload) {
    {50, 40, 420, 600},
    documentProc,
    invisible,
    goAway,
    0x0,
    "Toolbox Showcase",
    noAutoCenter
};

resource 'DLOG' (rPrefDialog, preload) {
    {100, 130, 290, 470},
    dBoxProc,
    invisible,
    noGoAway,
    0x0,
    rPrefDialog,
    "Game Preferences",
    noAutoCenter
};

resource 'DITL' (rPrefDialog, preload) {
    {
        {150, 240, 170, 310}, Button { enabled, "OK" };
        {150, 150, 170, 220}, Button { enabled, "Cancel" };
        {15, 20, 35, 320}, StaticText { disabled, "Game Engine Configuration" };
        {45, 20, 65, 320}, CheckBox { enabled, "Enable 3D Hardware Acceleration" };
        {70, 20, 90, 320}, CheckBox { enabled, "High-Resolution Texture Filtering" };
        {100, 20, 120, 100}, StaticText { disabled, "Callsign:" };
        {100, 105, 120, 300}, EditText { enabled, "Ace Pilot" }
    }
};

resource 'ALRT' (rAboutAlert, preload) {
    {130, 150, 260, 450},
    rAboutAlert,
    {
        OK, visible, sound1,
        OK, visible, sound1,
        OK, visible, sound1,
        OK, visible, sound1
    },
    noAutoCenter
};

resource 'DITL' (rAboutAlert, preload) {
    {
        {90, 210, 110, 280}, Button { enabled, "OK" };
        {20, 20, 80, 280}, StaticText { disabled, "Toolbox Showcase 2.0\nClassic Macintosh Fat-App Fixture\nRunning 68K and PowerPC slices" }
    }
};

resource 'ICON' (rShowcaseIcon) {
    $"0000 0000 000F F000 0030 0C00 0040 0200"
    $"0080 0100 0100 0080 0200 0040 040F F020"
    $"0830 0C10 1040 0208 2080 0104 2100 0084"
    $"4200 0042 440F F022 4830 0C12 5040 020A"
    $"5080 010A 4830 0C12 440F F022 4200 0042"
    $"2100 0084 2080 0104 1040 0208 0830 0C10"
    $"040F F020 0200 0040 0100 0080 0080 0100"
    $"0040 0200 0030 0C00 000F F000 0000 0000"
};

resource 'vers' (1) {
    0x02, 0x00, release, 0x00,
    verUS,
    "2.0",
    "Toolbox Showcase 2.0"
};
