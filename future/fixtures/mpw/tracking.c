#include <Types.h>
#include <Quickdraw.h>
#include <Fonts.h>
#include <Windows.h>
#include <Controls.h>
#include <ControlDefinitions.h>
#include <Events.h>
#include <Processes.h>
#include <Resources.h>
QDGlobals qd;
int main(void) {
    CursHandle watch;
    WindowPtr window;
    ControlHandle button;
    GrafPtr current;
    Rect bounds={50,50,200,300},controlBounds={10,10,30,90};
    Point start={20,20};
    InitGraf(&qd.thePort); InitFonts(); InitWindows();
    watch=GetCursor(watchCursor);
    if (!watch || GetResource('CURS',watchCursor)!=(Handle)watch || GetHandleSize((Handle)watch)!=68) return 5;
    SetCursor(*watch); ReleaseResource((Handle)watch);
    watch=GetCursor(watchCursor); if (!watch) return 6;
    ReleaseResource((Handle)watch);
    window=NewWindow(0,&bounds,"\pTracking",true,2,(WindowPtr)-1,false,0);
    if (!window) return 1;
    button=NewControl(window,&controlBounds,"\pOK",true,0,0,1,0,0);
    if (!button) return 2;
    SetPort(0);
    PostEvent(diskEvt,0xf1);
    if (TrackControl(button,start,0)!=kControlButtonPart) return 3;
    GetPort(&current);
    if (current || (*button)->contrlHilite || (*button)->contrlValue) return 4;
    DisposeControl(button); DisposeWindow(window);
    PostEvent(diskEvt,0x600d); ExitToShell(); return 0;
}
