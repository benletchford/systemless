#include "Types.r"
#include "CodeFragmentTypes.r"
#include "Processes.r"
#include "Menus.r"
#include "Windows.r"

#define rMenuBar 128
#define rMainWindow 128

#define mApple 128
#define mPages 129
#define mState 130
#define mFile 131

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
        "About Toolbox Showcase", noIcon, noKey, noMark, plain
    }
};

resource 'MENU' (mPages, preload) {
    mPages, textMenuProc, allEnabled, enabled, "Pages",
    {
        "Graphics", noIcon, noKey, check, plain;
        "Controls", noIcon, noKey, noMark, plain;
        "Windows", noIcon, noKey, noMark, plain
    }
};

resource 'MENU' (mState, preload) {
    mState, textMenuProc, allEnabled, enabled, "State",
    {
        "Button activated", noIcon, noKey, noMark, plain;
        "Checkbox selected", noIcon, noKey, noMark, plain;
        "Scrollbar moved", noIcon, noKey, noMark, plain;
        "Auxiliary window open", noIcon, noKey, noMark, plain
    }
};

resource 'MENU' (mFile, preload) {
    mFile, textMenuProc, allEnabled, enabled, "File",
    {
        "Quit", noIcon, "Q", noMark, plain
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

resource 'vers' (1) {
    0x01, 0x00, release, 0x00,
    verUS,
    "1.0",
    "Toolbox Showcase 1.0"
};
