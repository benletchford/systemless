#include <Types.h>
#include <Quickdraw.h>
#include <Resources.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>

QDGlobals qd;
static unsigned char pixels[8] = {0x80,0x33,0,0x44,0,0x55,0,0x66};
int main(void) {
    GrafPort port;
    GrafPort other;
    GrafPtr current;
    BitMap bitmap;
    Rect all = {0,0,4,8};
    Rect clip = {1,1,3,7};
    Rect patch = {1,2,2,4};
    EventRecord event;
    Handle data;
    RgnHandle visible, clipping;
    InitGraf(&qd.thePort);
    if (qd.thePort != 0 || qd.randSeed != 1) return 1;
    GetPort(&current);
    if (current != 0) return 7;
    OpenPort(&port);
    OpenPort(&other);
    SetPort(&port);
    GetPort(&current);
    if (current != &port) return 2;
    bitmap.baseAddr = (Ptr)pixels;
    bitmap.rowBytes = 2;
    bitmap.bounds = all;
    SetPortBits(&bitmap);
    ClipRect(&clip);
    data = GetResource('DATA',128);
    if (!data || GetHandleSize(data) != 3 || **data != 1) return 3;
    PenPat(&qd.gray);
    PaintRect(&all);
    PenPat(&qd.black);
    PenMode(patXor);
    PaintRect(&patch);
    pixels[2] ^= 2;
    PaintRect(&patch);
    if (pixels[0] != 0x80 || pixels[2] != 0x56 || pixels[4] != 0x2a || pixels[6] != 0) return 4;
    if (pixels[1] != 0x33 || pixels[3] != 0x44 || pixels[5] != 0x55 || pixels[7] != 0x66) return 5;
    PostEvent(keyDown,42);
    if (!WaitNextEvent(keyDownMask,&event,0,0) || event.message != 42) return 6;
    visible = port.visRgn;
    clipping = port.clipRgn;
    SetPort(&other);
    InitPort(&port);
    GetPort(&current);
    if (current != &port || port.visRgn != visible || port.clipRgn != clipping) return 9;
    if (port.pnMode != patCopy || port.portBits.baseAddr != qd.screenBits.baseAddr) return 10;
    ClosePort(&other);
    GetPort(&current);
    if (current != &port) return 8;
    OpenPort(&other);
    SetPort(&other);
    ClosePort(&port);
    ClosePort(&other);
    PostEvent(diskEvt,0x600D);
    ExitToShell();
    return 0;
}
