#include <Types.h>
#include <Quickdraw.h>
#include <Windows.h>
#include <Menus.h>
#include <Fonts.h>
#include <Events.h>
#include <Processes.h>
QDGlobals qd;
int main(void) {
    Handle bar;
    Point start;
    long choice;
    InitGraf(&qd.thePort); InitFonts(); InitWindows(); InitMenus();
    bar=GetNewMBar(42); if (!bar) return 1;
    SetMenuBar(bar); DrawMenuBar();
    start.v=10; start.h=15;
    choice=MenuSelect(start);
    if (choice!=((70L<<16)|1) || MenuChoice()!=choice || *(short *)0xa26!=70) return 2;
    HiliteMenu(0); if (*(short *)0xa26) return 3;
    PostEvent(diskEvt,0x600d); ExitToShell(); return 0;
}
