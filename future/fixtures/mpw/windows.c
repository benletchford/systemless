#include <Types.h>
#include <Quickdraw.h>
#include <Windows.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>
QDGlobals qd;
static WindowRecord storage;
int main(void) {
    Rect firstBounds={40,40,60,60};
    Rect nextBounds={50,50,70,70};
    Rect local={0,0,20,20};
    WindowPtr first, next, hidden;
    GrafPtr old;
    Point point;
    InitGraf(&qd.thePort);
    InitWindows();
    GetPort(&old);
    if (old!=0 || FrontWindow()!=0) return 1;
    first=NewWindow(0,&firstBounds,"\ptest",true,plainDBox,(WindowPtr)-1,false,0x12345678);
    if (!first || ((WindowPeek)first)->windowKind!=8 || ((WindowPeek)first)->refCon!=0x12345678 || FrontWindow()!=first) return 2;
    GetPort(&old);
    if (old!=(GrafPtr)first || first->portRect.bottom!=20 || first->portRect.right!=20) return 3;
    next=NewWindow(0,&nextBounds,"\pnext",true,plainDBox,(WindowPtr)-1,false,7);
    if (!next || ((WindowPeek)next)->nextWindow!=(WindowPeek)first || FrontWindow()!=next) return 4;
    point.v=15;point.h=15;
    if (PtInRgn(point,first->visRgn)) return 5;
    point.v=0;point.h=0;
    if (!PtInRgn(point,first->visRgn)) return 6;
    SetPort((GrafPtr)first);
    PaintRect(&local);
    CloseWindow(next);
    if (FrontWindow()!=first) return 7;
    point.v=15;point.h=15;
    if (!PtInRgn(point,first->visRgn)) return 8;
    DisposeWindow(next);
    DisposeWindow(first);
    if (FrontWindow()!=0) return 9;
    GetPort(&old);
    hidden=NewWindow((Ptr)&storage,&firstBounds,"\phidden",false,plainDBox,0,true,11);
    if (hidden!=(WindowPtr)&storage || ((WindowPeek)hidden)->visible || ((WindowPeek)hidden)->refCon!=11 || FrontWindow()!=0) return 10;
    GetPort(&qd.thePort);
    if (qd.thePort!=old) return 11;
    CloseWindow(hidden);
    hidden=NewWindow((Ptr)&storage,&firstBounds,"\phidden",false,plainDBox,0,false,12);
    if (hidden!=(WindowPtr)&storage || ((WindowPeek)hidden)->refCon!=12) return 12;
    CloseWindow(hidden);
    if (MemError()) return 13;
    PostEvent(diskEvt,0x600D);
    ExitToShell();
    return 0;
}
