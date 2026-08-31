#include "Types.r"
#include "Processes.r"

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

resource 'MBAR' (kMenuBar, preload) {
	{ kAppleMenu, kFileMenu, kEditMenu, kPagesMenu, kDemoMenu }
};

resource 'MENU' (kEditMenu, preload) {
	kEditMenu, textMenuProc, 0xFFFFFFFF, enabled, "Edit",
	{
		"Undo", noicon, "Z", nomark, plain;
		"-", noicon, nokey, nomark, plain;
		"Cut", noicon, "X", nomark, plain;
		"Copy", noicon, "C", nomark, plain;
		"Paste", noicon, "V", nomark, plain;
		"Clear", noicon, nokey, nomark, plain
	}
};

resource 'MENU' (kAppleMenu, preload) {
	kAppleMenu, textMenuProc, 0xFFFFFFFF, enabled, apple,
	{
		"About Toolbox Showcase...", noicon, nokey, nomark, plain;
		"-", noicon, nokey, nomark, plain
	}
};

resource 'MENU' (kFileMenu, preload) {
	kFileMenu, textMenuProc, 0xFFFFFFFF, enabled, "File",
	{
		"New Companion Window", noicon, "N", nomark, plain;
		"Close", noicon, "W", nomark, plain;
		"-", noicon, nokey, nomark, plain;
		"Quit", noicon, "Q", nomark, plain
	}
};

resource 'MENU' (kPagesMenu, preload) {
	kPagesMenu, textMenuProc, 0xFFFFFFFF, enabled, "Pages",
	{
		"Overview", noicon, "1", nomark, plain;
		"QuickDraw", noicon, "2", nomark, plain;
		"Controls", noicon, "3", nomark, plain;
		"TextEdit", noicon, "4", nomark, plain;
		"Windows", noicon, "5", nomark, plain;
		"Resources & Events", noicon, "6", nomark, plain
	}
};

resource 'MENU' (kDemoMenu, preload) {
	kDemoMenu, textMenuProc, 0xFFFFFFFF, enabled, "Demo",
	{
		"Show About Alert...", noicon, nokey, nomark, plain;
		"Toggle Checkbox", noicon, "T", nomark, plain;
		"Reset Interactive State", noicon, "R", nomark, plain;
		"-", noicon, nokey, nomark, plain;
		"Checkbox selected", noicon, nokey, nomark, plain;
		"Scrollbar moved", noicon, nokey, nomark, plain;
		"Text edited", noicon, nokey, nomark, plain;
		"Palette", noicon, nokey, nomark, plain
	}
};

resource 'MENU' (kPaletteMenu, preload) {
	kPaletteMenu, textMenuProc, 0xFFFFFFFF, enabled, "Palette",
	{
		"Red", noicon, nokey, nomark, plain;
		"Green", noicon, nokey, nomark, plain;
		"Blue", noicon, nokey, nomark, plain
	}
};

resource 'WIND' (kMainWindow, preload, purgeable) {
	{40, 20, 440, 620},
	zoomDocProc, invisible, goAway, 0x0, "Systemless Toolbox Showcase",
	noAutoCenter
};

resource 'ALRT' (kAboutAlert, purgeable) {
	{80, 100, 245, 540},
	kAboutAlert,
	{
		OK, visible, silent;
		OK, visible, silent;
		OK, visible, silent;
		OK, visible, silent
	},
	centerMainScreen
};

resource 'DITL' (kAboutAlert, purgeable) {
	{
		{128, 330, 148, 410},
		Button { enabled, "OK" };
		{14, 18, 34, 414},
		StaticText { disabled, "Systemless Toolbox Showcase" };
		{45, 18, 80, 414},
		StaticText { disabled, "One fat application exercising the same Toolbox code through 68K and native PowerPC runtimes." };
		{91, 18, 115, 414},
		StaticText { disabled, "Use Pages, the navigation buttons, or keys 1-6 to explore." }
	}
};

resource 'STR#' (kPageDescriptions, purgeable) {
	{
		"A shared event loop connects menus, windows, controls, resources, and both CPU architectures.";
		"Primitives, patterns, colors, clipping regions, polygons, text, and CopyBits share one drawing port.";
		"Buttons, checkboxes, radio buttons, and scroll bars expose Control Manager tracking and state.";
		"A live TextEdit record handles selection, insertion, caret idling, scrolling, and redraw.";
		"Open, activate, drag, grow, zoom, update, and close a second document window.";
		"Resource-loaded menus, strings, windows, and alerts are driven by Event Manager dispatch."
	}
};

resource 'vers' (1) {
	0x01, 0x00, development, 0x00,
	verUS,
	"1.0",
	"Toolbox Showcase 1.0"
};

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
	reserved,
	reserved,
	reserved,
	reserved,
	reserved,
	reserved,
	reserved,
	1024 * 1024,
	512 * 1024
};
