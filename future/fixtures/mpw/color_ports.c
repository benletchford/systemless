#include <Types.h>
#include <Quickdraw.h>
#include <Memory.h>
#include <MacErrors.h>
#include <Events.h>
#include <Processes.h>
QDGlobals qd;
static CGrafPort first, second;
static GrafPort basic;
int main(void) {
    GrafPtr saved, current;
    GDHandle device;
    CTabHandle deviceTable, custom;
    Handle attached[7];
    GVarHandle vars;
    RGBColor *hilite=(RGBColor *)0x0da0;
    short i;
    InitGraf(&qd.thePort); GetPort(&saved);
    hilite->red=0x1111;hilite->green=0x2222;hilite->blue=0x3333;
    OpenCPort(&first); if (MemError()) return 1;
    GetPort(&current); if (current!=(GrafPtr)&first || first.portVersion!=(short)0xc000 || first.pnLocHFrac!=(short)0x8000) return 2;
    if (first.rgbFgColor.red || first.rgbFgColor.green || first.rgbFgColor.blue || first.rgbBkColor.red!=65535 || first.rgbBkColor.green!=65535 || first.rgbBkColor.blue!=65535) return 3;
    if (!first.portPixMap || !first.visRgn || !first.clipRgn || !first.bkPixPat || !first.pnPixPat || !first.fillPixPat || !first.grafVars) return 4;
    if (GetHandleSize((Handle)first.portPixMap)!=sizeof(PixMap) || GetHandleSize((Handle)first.bkPixPat)!=sizeof(PixPat) || GetHandleSize(first.grafVars)!=sizeof(GrafVars)) return 5;
    vars=(GVarHandle)first.grafVars;
    if ((**vars).rgbHiliteColor.red!=0x1111 || (**vars).rgbHiliteColor.green!=0x2222 || (**vars).rgbHiliteColor.blue!=0x3333 || (**vars).rgbOpColor.red || (**vars).rgbOpColor.green || (**vars).rgbOpColor.blue) return 6;
    device=GetGDevice();deviceTable=(**(**device).gdPMap).pmTable;
    if ((**first.portPixMap).pmTable!=deviceTable || (**first.portPixMap).baseAddr!=qd.screenBits.baseAddr) return 7;
    attached[0]=(Handle)first.portPixMap;attached[1]=(Handle)first.visRgn;attached[2]=(Handle)first.clipRgn;attached[3]=(Handle)first.bkPixPat;attached[4]=(Handle)first.pnPixPat;attached[5]=(Handle)first.fillPixPat;attached[6]=first.grafVars;
    for (i=0;i<8;i++) if ((**first.bkPixPat).pat1Data.pat[i] || (**first.pnPixPat).pat1Data.pat[i]!=255 || (**first.fillPixPat).pat1Data.pat[i]!=255) return 8;
    OpenCPort(&second); if (MemError() || second.portPixMap==first.portPixMap || second.visRgn==first.visRgn || (**second.portPixMap).pmTable!=deviceTable) return 9;
    first.txSize=27;(**first.pnPixPat).pat1Data.pat[0]=0x55;
    InitCPort(&first); if (MemError() || first.txSize || (**first.pnPixPat).pat1Data.pat[0]!=255) return 10;
    GetPort(&current); if (current!=(GrafPtr)&first || attached[0]!=(Handle)first.portPixMap || attached[4]!=(Handle)first.pnPixPat) return 11;
    custom=(CTabHandle)NewHandle(24); if (!custom) return 12;
    (**first.portPixMap).pmTable=custom;
    SetPort((GrafPtr)&second);CloseCPort(&first);
    for (i=0;i<7;i++) {GetHandleSize(attached[i]);if (MemError()!=memWZErr) return 13;}
    if (GetHandleSize((Handle)custom)!=24 || GetHandleSize((Handle)deviceTable)!=24 || GetHandleSize((Handle)second.portPixMap)!=sizeof(PixMap)) return 14;
    DisposeHandle((Handle)custom);
    SetPort(saved);CloseCPort(&second);if (MemError()) return 15;
    OpenPort(&basic); InitCPort((CGrafPtr)&basic);
    GetPort(&current);if (current!=&basic || basic.portBits.rowBytes!=64) return 16;
    SetPort(saved);ClosePort(&basic);
    if (GetHandleSize((Handle)deviceTable)!=24) return 17;
    PostEvent(diskEvt,0xc907);ExitToShell();return 0;
}
