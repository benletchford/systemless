#include <Types.h>
#include <Quickdraw.h>
#include <Windows.h>
#include <Dialogs.h>
#include <Controls.h>
#include <ControlDefinitions.h>
#include <Memory.h>
#include <Resources.h>
#include <Events.h>
#include <Processes.h>
#include <Menus.h>
#include <Devices.h>
QDGlobals qd;
static WindowRecord storage;
int main(void) {
    Rect firstBounds={40,40,60,60};
    Rect nextBounds={50,50,70,70};
    Rect local={0,0,20,20};
    WindowPtr first, next, hidden, found;
    short savedMenuHeight;
    GrafPtr old;
    GrafPort offscreen;
    PicHandle picture;
    Point point;
    EventRecord event;
    Rect dirty={43,42,49,48};
    RgnHandle saved;
    ControlHandle button, check, radio, foundControl;
    Rect buttonBounds={8,8,28,68},checkBounds={32,8,48,78},radioBounds={32,80,48,150};
    Str255 controlTitle;
    SetRect(&firstBounds,40,40,60,60); OffsetRect(&firstBounds,2,-3); InsetRect(&firstBounds,1,2);
    if (firstBounds.top!=39 || firstBounds.left!=43 || firstBounds.bottom!=55 || firstBounds.right!=61) return 14;
    SetRect(&firstBounds,40,40,60,60);
    InitGraf(&qd.thePort); InitFonts();
    InitWindows(); SystemTask();
    GetPort(&old);
    if (old!=0 || FrontWindow()!=0) return 1;
    first=NewWindow(0,&firstBounds,"\ptest",true,plainDBox,(WindowPtr)-1,false,0x12345678);
    if (!first || ((WindowPeek)first)->windowKind!=8 || ((WindowPeek)first)->refCon!=0x12345678 || FrontWindow()!=first) return 2;
    event.what=nullEvent; event.message=0; event.when=0;
    event.where.v=0; event.where.h=0; event.modifiers=0;
    if (IsDialogEvent(&event)!=false) return 100;
    ((WindowPeek)first)->windowKind=dialogKind;
    if (IsDialogEvent(&event)!=true) return 101;
    event.what=updateEvt; event.message=0x12345678;
    if (IsDialogEvent(&event)!=true) return 102;
    ((WindowPeek)first)->windowKind=userKind;
    if (IsDialogEvent(&event)!=false) return 103;
    {
        Rect probe={0,0,0,0};
        if (EmptyRect(&probe)!=true) return 104;
        probe.top=-32768;probe.left=-32768;probe.bottom=32767;probe.right=32767;
        if (EmptyRect(&probe)!=false) return 105;
        probe.bottom=-32768;
        if (EmptyRect(&probe)!=true) return 106;
        probe.top=1;probe.left=1;probe.bottom=2;probe.right=2;
        if (EmptyRect(&probe)!=false) return 107;
    }
    GetPort(&old);
    if (old!=(GrafPtr)first || first->portRect.bottom!=20 || first->portRect.right!=20) return 3;
    savedMenuHeight=*(short*)0xbaa; *(short*)0xbaa=20;
    point.v=40;point.h=40;
    if (FindWindow(point,&found)!=inContent || found!=first || FrontWindow()!=first) return 60;
    point.v=10;point.h=10;
    if (FindWindow(point,&found)!=inMenuBar || found!=0) return 61;
    point.v=39;point.h=40;
    if (FindWindow(point,&found)!=inDesk || found!=0) return 62;
    *(short*)0xbaa=savedMenuHeight;
    PenSize(2,3); if (first->pnSize.h!=2 || first->pnSize.v!=3) return 46; PenSize(1,1);
    if (!RectInRgn(&local,first->visRgn)) return 47;
    picture=GetPicture(322);
    if (!picture || (Handle)picture!=GetResource('PICT',322) || GetHandleSize((Handle)picture)!=44) return 48;
    DrawPicture(picture,&local);
    if (!(((unsigned char*)first->portBits.baseAddr)[40*first->portBits.rowBytes+5]&0x80) || !(((unsigned char*)first->portBits.baseAddr)[55*first->portBits.rowBytes+6]&1)) return 50;
    {
        Rect sourceCopy={0,0,10,10}, destinationCopy={10,0,20,10};
        CopyBits(&first->portBits,&first->portBits,&sourceCopy,&destinationCopy,srcCopy,0);
        if (!(((unsigned char*)first->portBits.baseAddr)[55*first->portBits.rowBytes+5]&0x80)) return 51;
    }
    {
        unsigned char *pixels=(unsigned char*)first->portBits.baseAddr;
        unsigned long offset=42*first->portBits.rowBytes+5;
        unsigned char savedByte=pixels[offset];
        PenSize(3,2); PenMode(patXor); MoveTo(1,1); LineTo(6,4);
        if (first->pnLoc.h!=6 || first->pnLoc.v!=4 || !((pixels[offset]^savedByte)&0x20)) return 52;
        MoveTo(1,1); LineTo(6,4);
        if (pixels[offset]!=savedByte) return 53;
        Line(-2,1); if (first->pnLoc.h!=4 || first->pnLoc.v!=5) return 54;
        PenSize(1,1); PenMode(patCopy); MoveTo(0,0);
    }
    {
        Rect box={1,1,7,7};
        Pattern fill={255,255,255,255,255,255,255,255};
        unsigned char *pixels=(unsigned char*)first->portBits.baseAddr;
        unsigned long offset=42*first->portBits.rowBytes+5;
        unsigned char savedByte;
        EraseRect(&box); if (pixels[offset]&0x20) return 55;
        savedByte=pixels[offset]; InvertRect(&box);
        if (!((pixels[offset]^savedByte)&0x20)) return 56;
        InvertRect(&box); if (pixels[offset]!=savedByte) return 57;
        FrameRect(&box);
        if (!(pixels[41*first->portBits.rowBytes+5]&0x40) || (pixels[offset]&0x20)) return 58;
        FillRect(&box,&fill);
        if (!(pixels[offset]&0x20) || ((unsigned char*)&first->fillPat)[0]!=255 || ((unsigned char*)&first->fillPat)[7]!=255) return 59;
        EraseRect(&box);
    }
    ReleaseResource((Handle)picture);
    picture=GetPicture(323);
    if (!picture || GetHandleSize((Handle)picture)!=154) return 60;
    DrawPicture(picture,&local);
    if (!(((unsigned char*)first->portBits.baseAddr)[40*first->portBits.rowBytes+5]&0x80) || !(((unsigned char*)first->portBits.baseAddr)[55*first->portBits.rowBytes+6]&1) || (((unsigned char*)first->portBits.baseAddr)[40*first->portBits.rowBytes+6]&1)) return 61;
    ReleaseResource((Handle)picture);
    OpenPort(&offscreen); PortSize(8,4); MovePortTo(2,1);
    if (offscreen.portRect.right!=8 || offscreen.portRect.bottom!=4 || offscreen.portBits.bounds.left!=-2 || offscreen.portBits.bounds.top!=-1 || offscreen.portBits.bounds.right!=510 || offscreen.portBits.bounds.bottom!=341) return 49;
    MoveTo(7,9); SetOrigin(20,30);
    if (offscreen.portRect.left!=20 || offscreen.portRect.top!=30 || offscreen.portBits.bounds.left!=18 || offscreen.portBits.bounds.top!=29 || (*offscreen.visRgn)->rgnBBox.left!=20 || (*offscreen.visRgn)->rgnBBox.top!=30 || offscreen.pnLoc.h!=7 || offscreen.pnLoc.v!=9) return 63;
    SetOrigin(0,0);
    if (offscreen.portRect.left || offscreen.portRect.top || offscreen.portBits.bounds.left!=-2 || offscreen.portBits.bounds.top!=-1) return 64;
    ClosePort(&offscreen); SetPort((GrafPtr)first);
    saved=NewRgn(); CopyRgn(first->visRgn,saved);
    OffsetRgn(saved,3,4);
    if ((*saved)->rgnBBox.left!=(*first->visRgn)->rgnBBox.left+3 || (*saved)->rgnBBox.top!=(*first->visRgn)->rgnBBox.top+4) return 90;
    OffsetRgn(saved,-3,-4);
    if (!EqualRgn(saved,first->visRgn)) return 91;
    SetEmptyRgn(saved);
    if (!EmptyRgn(saved) || GetHandleSize((Handle)saved)!=10) return 65;
    CopyRgn(first->visRgn,saved);
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
    point.v=8;point.h=8;
    if (FindControl(point,first,&foundControl)!=10 || foundControl!=button) return 63;
    point.v=28;if (FindControl(point,first,&foundControl)!=0 || foundControl!=0) return 64;
    point.v=32;point.h=80;
    if (FindControl(point,first,&foundControl)!=11 || foundControl!=radio) return 65;
    if (!button || !check || !radio || ((WindowPeek)first)->controlList!=(Handle)radio || (*radio)->nextControl!=check || (*check)->nextControl!=button || GetControlReference(button)!=0x11223344) return 27;
    if (GetHandleSize((Handle)button)!=296 || (*button)->contrlOwner!=first || (*button)->contrlVis!=255) return 28;
    SetControlReference(button,9); if (GetControlReference(button)!=9) return 29;
    SetControlValue(check,5); if (GetControlValue(check)!=1 || GetControlMinimum(check)!=0 || GetControlMaximum(check)!=1) return 30;
    GetControlTitle(button,controlTitle); if (controlTitle[0]!=2 || controlTitle[1]!='O') return 31;
    {
        GrafPtr saved,actual;
        GetPort(&saved); SetPort(0);
        MoveControl(button,12,10);
        if ((*button)->contrlRect.left!=12 || (*button)->contrlRect.top!=10 || (*button)->contrlRect.right!=72 || (*button)->contrlRect.bottom!=30) return 108;
        SizeControl(button,70,22);
        if ((*button)->contrlRect.left!=12 || (*button)->contrlRect.top!=10 || (*button)->contrlRect.right!=82 || (*button)->contrlRect.bottom!=32) return 109;
        MoveControl(button,8,8); SizeControl(button,60,20);
        GetPort(&actual); if (actual!=0 || (*button)->contrlRect.right!=68 || (*button)->contrlRect.bottom!=28 || (*button)->contrlVis!=255) return 110;
        SetPort(saved);
    }

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
    MoveWindow(first,120,100,false);
    if (first->portBits.bounds.top!=-100 || first->portBits.bounds.left!=-120 || first->portRect.right!=180) return 43;
    DrawControls(first);
    HideWindow(first); if (((WindowPeek)first)->visible || FrontWindow()) return 44;
    ShowWindow(first); SelectWindow(first);
    while (GetNextEvent(activMask,&event)) {if (event.what!=activateEvt || event.message!=(long)first) return 45;}
    DrawControls(first); DisposeControl(check); if ((*radio)->nextControl!=button || *check) return 38;
    DisposeWindow(first); if (FrontWindow() || *button || *radio) return 26;
    SetPort(0); GetPort(&old); if (old!=0) return 68;
    FlushEvents(mDownMask|mUpMask,0);
    if (!StillDown()) return 66;
    PostEvent(mouseUp,0); if (StillDown()) return 67;
    FlushEvents(mUpMask,0);
    PostEvent(diskEvt,0x600D);
    ExitToShell();
    return 0;
}
