#include <Types.h>
#include <Quickdraw.h>
#include <Windows.h>
#include <Memory.h>
#include <MacErrors.h>
#include <Events.h>
#include <Processes.h>
#include <ToolUtils.h>
QDGlobals qd;
static CWindowRecord storage;
int main(void) {
    Rect bounds, ink;
    WindowPtr window, other;
    CWindowPeek peek;
    GrafPtr saved, current;
    Handle owned[11];
    CTabHandle table;
    PixMapHandle pixels;
    Handle vars;
    short i;
    volatile long value=0x89abcdef;
    long available;
    Ptr temporary;
    *((short *)0x220)=-42;
    if (BitAnd(value,0x13579bdf)!=0x010389cf || BitOr(value,0x13579bdf)!=0x9bffdfff || BitXor(value,0x13579bdf)!=0x9afc5630 || BitNot(value)!=0x76543210 || *((short *)0x220)!=-42) return 10;
    available=FreeMem();temporary=NewPtr(33);if (!temporary || FreeMem()!=available-36) return 11;
    DisposePtr(temporary);if (FreeMem()!=available) return 12;
    InitGraf(&qd.thePort);InitWindows();GetPort(&saved);SetGDevice(GetGDevice());
    bounds.top=70;bounds.left=50;bounds.bottom=70;bounds.right=50;
    window=NewCWindow((Ptr)&storage,&bounds,"\pColor",false,plainDBox,(WindowPtr)-1,false,0x12345678);
    if (!window || window!=(WindowPtr)&storage || MemError()) return 1;
    peek=(CWindowPeek)window;GetPort(&current);
    if (current!=saved || peek->visible || peek->refCon!=0x12345678 || peek->port.portVersion!=(short)0xc000) return 2;
    pixels=peek->port.portPixMap;vars=peek->port.grafVars;table=(**pixels).pmTable;
    owned[0]=(Handle)pixels;owned[1]=(Handle)peek->port.visRgn;owned[2]=(Handle)peek->port.clipRgn;
    owned[3]=(Handle)peek->port.bkPixPat;owned[4]=(Handle)peek->port.pnPixPat;owned[5]=(Handle)peek->port.fillPixPat;
    owned[6]=peek->port.grafVars;owned[7]=(Handle)peek->strucRgn;owned[8]=(Handle)peek->contRgn;owned[9]=(Handle)peek->updateRgn;owned[10]=(Handle)peek->titleHandle;
    if (sizeof(CWindowRecord)!=156 || !EmptyRgn(peek->port.visRgn) || peek->port.portRect.bottom || peek->port.portRect.right) return 3;
    SizeWindow(window,80,60,true);ShowWindow(window);SetPort((GrafPtr)window);
    if ((**peek->contRgn).rgnBBox.top!=70 || (**peek->contRgn).rgnBBox.left!=50 || peek->port.portRect.right!=80 || peek->port.portRect.bottom!=60) return 4;
    ink.top=0;ink.left=0;ink.bottom=1;ink.right=8;PaintRect(&ink);
    if (!(((unsigned char *)qd.screenBits.baseAddr)[70*qd.screenBits.rowBytes+6]&0x20)) return 5;
    BeginUpdate(window);EndUpdate(window);MoveWindow(window,100,80,false);
    if ((**pixels).bounds.top!=-80 || (**pixels).bounds.left!=-100 || peek->port.grafVars!=vars || (**pixels).pmTable!=table) return 6;
    bounds.top=100;bounds.left=140;bounds.bottom=150;bounds.right=220;
    other=NewCWindow(0,&bounds,"\pOther",true,plainDBox,(WindowPtr)-1,false,0);
    if (!other || ((CWindowPeek)other)->port.portPixMap==pixels) return 7;
    DisposeWindow(other);SetPort(saved);CloseWindow(window);
    for (i=0;i<11;i++) {GetHandleSize(owned[i]);if (MemError()!=memWZErr) return 8;}
    if (GetHandleSize((Handle)table)!=24 || peek->port.grafVars!=vars) return 9;
    other=GetNewCWindow(500,0,(WindowPtr)-1);
    if (!other || ((CWindowPeek)other)->port.portVersion!=(short)0xc000 || ((CWindowPeek)other)->refCon!=0x01020304) return 13;
    if (((CWindowPeek)other)->port.rgbBkColor.red!=65535 || ((CWindowPeek)other)->port.rgbBkColor.green || ((CWindowPeek)other)->port.rgbBkColor.blue) return 14;
    DisposeWindow(other);
    if (GetNewCWindow(999,0,(WindowPtr)-1)) return 15;
    PostEvent(diskEvt,0xc945);ExitToShell();return 0;
}
