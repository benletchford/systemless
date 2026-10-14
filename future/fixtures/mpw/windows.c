#include <Types.h>
#include <Quickdraw.h>
#include <Windows.h>
#include <Controls.h>
#include <ControlDefinitions.h>
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
    EventRecord event;
    Rect dirty={43,42,49,48};
    RgnHandle saved;
    ControlHandle button, check, radio;
    Rect buttonBounds={8,8,28,68},checkBounds={32,8,48,78},radioBounds={32,80,48,150};
    Str255 controlTitle;
    SetRect(&firstBounds,40,40,60,60); OffsetRect(&firstBounds,2,-3); InsetRect(&firstBounds,1,2);
    if (firstBounds.top!=39 || firstBounds.left!=43 || firstBounds.bottom!=55 || firstBounds.right!=61) return 14;
    SetRect(&firstBounds,40,40,60,60);
    InitGraf(&qd.thePort); InitFonts();
    InitWindows();
    GetPort(&old);
    if (old!=0 || FrontWindow()!=0) return 1;
    first=NewWindow(0,&firstBounds,"\ptest",true,plainDBox,(WindowPtr)-1,false,0x12345678);
    if (!first || ((WindowPeek)first)->windowKind!=8 || ((WindowPeek)first)->refCon!=0x12345678 || FrontWindow()!=first) return 2;
    GetPort(&old);
    if (old!=(GrafPtr)first || first->portRect.bottom!=20 || first->portRect.right!=20) return 3;
    saved=NewRgn(); CopyRgn(first->visRgn,saved);
    RectRgn(((WindowPeek)first)->updateRgn,&dirty);
    if (OSEventAvail(updateMask,&event)) return 15;
    if (!EventAvail(updateMask,&event) || event.what!=updateEvt || event.message!=(long)first) return 16;
    if (!EventAvail(updateMask,&event) || !GetNextEvent(updateMask,&event)) return 17;
    if (!WaitNextEvent(updateMask,&event,5,0) || event.message!=(long)first) return 18;
    BeginUpdate(first);
    if (!EmptyRgn(((WindowPeek)first)->updateRgn) || EventAvail(updateMask,&event)) return 19;
    point.v=4;point.h=3; if (!PtInRgn(point,first->visRgn)) return 20;
    point.v=0;point.h=0; if (PtInRgn(point,first->visRgn)) return 21;
    PaintRect(&local); RectRgn(((WindowPeek)first)->updateRgn,&dirty); EndUpdate(first);
    if (!EqualRgn(first->visRgn,saved) || !EventAvail(updateMask,&event)) return 22;
    BeginUpdate(first); EndUpdate(first);
    if (EventAvail(updateMask,&event)) return 23;
    DisposeRgn(saved);
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
    if (GetNewWindow(322,0,(WindowPtr)-1)) return 24;
    first=GetNewWindow(321,0,(WindowPtr)-1);
    if (!first || FrontWindow()!=first || ((WindowPeek)first)->refCon!=0x12345678 || !((WindowPeek)first)->goAwayFlag || first->portRect.right!=160 || first->portRect.bottom!=60) return 25;
    button=NewControl(first,&buttonBounds,"\pOK",true,0,0,1,pushButProc,0x11223344);
    check=NewControl(first,&checkBounds,"\pCheck",true,0,0,1,checkBoxProc,7);
    radio=NewControl(first,&radioBounds,"\pRadio",true,1,0,1,radioButProc,8);
    if (!button || !check || !radio || ((WindowPeek)first)->controlList!=(Handle)radio || (*radio)->nextControl!=check || (*check)->nextControl!=button || GetControlReference(button)!=0x11223344) return 27;
    if (GetHandleSize((Handle)button)!=296 || (*button)->contrlOwner!=first || (*button)->contrlVis!=255) return 28;
    SetControlReference(button,9); if (GetControlReference(button)!=9) return 29;
    SetControlValue(check,5); if (GetControlValue(check)!=1 || GetControlMinimum(check)!=0 || GetControlMaximum(check)!=1) return 30;
    GetControlTitle(button,controlTitle); if (controlTitle[0]!=2 || controlTitle[1]!='O') return 31;
    SetControlTitle(button,"\pGo"); GetControlTitle(button,controlTitle); if (controlTitle[1]!='G') return 32;
    HiliteControl(button,10); if ((*button)->contrlHilite!=10) return 33; HiliteControl(button,0);
    HideControl(check); if ((*check)->contrlVis) return 34; ShowControl(check);
    point.v=40;point.h=12; if (TestControl(check,point)!=11) return 35;
    HiliteControl(check,255); if (TestControl(check,point)) return 36; HiliteControl(check,0);
    (*check)->contrlValue=0; Draw1Control(check); if (GetControlValue(check)) return 37;
    SetWRefCon(first,0x76543210); if (GetWRefCon(first)!=0x76543210) return 39;
    SizeWindow(first,180,70,true); if (first->portRect.right!=180 || first->portRect.bottom!=70 || (*button)->contrlOwner!=first) return 40;
    ShowHide(first,false); if (((WindowPeek)first)->visible || FrontWindow()) return 41;
    ShowHide(first,true); if (!((WindowPeek)first)->visible || FrontWindow()!=first) return 42;
    DrawControls(first); DisposeControl(check); if ((*radio)->nextControl!=button || *check) return 38;
    DisposeWindow(first); if (FrontWindow() || *button || *radio) return 26;
    PostEvent(diskEvt,0x600D);
    ExitToShell();
    return 0;
}
