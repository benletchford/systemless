#include <Types.h>
#include <Quickdraw.h>
#include <Windows.h>
#include <Menus.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>
#include <Fonts.h>
#include <Resources.h>
QDGlobals qd;
static void putLong(unsigned char *p,unsigned long value) {
    p[0]=value>>24; p[1]=value>>16; p[2]=value>>8; p[3]=value;
}
int main(void) {
    GrafPort port;
    Handle live,colors,copy;
    CursHandle cursor;
    RgnHandle gray;
    unsigned char *pixels;
    unsigned char below;
    InitGraf(&qd.thePort); OpenPort(&port); InitWindows();
    pixels=(unsigned char *)qd.screenBits.baseAddr; below=pixels[64*20]; InitMenus();
    live=*(Handle *)0xa1c; colors=*(Handle *)0xd50;
    gray=*(RgnHandle *)0x9ee; pixels=(unsigned char *)qd.screenBits.baseAddr;
    if (!live || !colors || GetHandleSize(live)!=12 || GetHandleSize(colors)!=30) return 1;
    if (*(short *)0xbaa!=20 || (*gray)->rgnBBox.top!=20 || (*(short *)0xa26)) return 2;
    if (pixels[0]!=0xf8 || pixels[63]!=0x1f || pixels[64]!=0xe0 || pixels[127]!=7 || pixels[64*5] || pixels[64*19]!=0xff || pixels[64*20]!=below) return 3;
    ((short *)*live)[1]=123;
    copy=GetMenuBar(); if (!copy || copy==live || ((short *)*copy)[1]!=123 || GetHandleSize(copy)!=12) return 4;
    DisposeHandle(copy);
    *(short *)0xa26=77; ClearMenuBar();
    if (*(Handle *)0xa1c!=live || ((short *)*live)[1] || (*(short *)0xa26)) return 5;
    InitMenus(); if (*(Handle *)0xa1c!=live || *(Handle *)0xd50!=colors) return 6;
    if (((unsigned short *)*colors)[14]!=0xbeef) return 7;
    SetHandleSize(colors,0); DrawMenuBar();
    if (MemError() || pixels[0]!=0xf8 || port.pnMode!=patCopy || qd.thePort!=&port) return 8;
    {
        MenuHandle normal=(MenuHandle)NewHandleClear(16), sub=(MenuHandle)NewHandleClear(16);
        unsigned char *table;
        short i;
        if (!normal || !sub || GetMenuHandle(99)!=0) return 11;
        *(short *)*normal=-7; *(short *)*sub=-7;
        SetHandleSize(live,24); if (MemError()) return 12;
        table=(unsigned char *)*live; for (i=0;i<24;i++) table[i]=0;
        *(short *)table=6; putLong(table+6,(unsigned long)normal);
        *(short *)(table+12)=6; putLong(table+18,(unsigned long)sub);
        if (GetMenuHandle(-7)!=sub || GetMenuHandle(99)!=0) return 13;
        SetHandleSize((Handle)sub,32); *(short *)*sub=9;
        if (GetMenuHandle(-7)!=normal || GetMenuHandle(9)!=sub) return 14;
        ClearMenuBar(); if (GetMenuHandle(9)!=0 || GetHandleSize((Handle)normal)!=16 || GetHandleSize((Handle)sub)!=32) return 15;
        DisposeHandle((Handle)normal); DisposeHandle((Handle)sub);
    }
    {
        Handle bar;
        MenuHandle menu;
        InitFonts();
        bar=GetNewMBar(42); if (!bar || GetHandleSize(bar)!=24) return 16;
        menu=GetMenu(7); if (!menu || menu!=(MenuHandle)GetResource('MENU',7) || (*menu)->menuID!=70) return 17;
        if ((*menu)->menuWidth!=36 || (*menu)->menuHeight!=16) return 18;
        if (CountMItems(menu)!=1) return 22;
        AppendResMenu(menu,'TEST');
        if (CountMItems(menu)!=3 || !(*(unsigned char *)0xa5e)) return 23;
        AppendResMenu(menu,'TEST'); if (CountMItems(menu)!=3) return 24;
        SetItemMark(menu,1,0x1234); if (((unsigned char *)*menu)[20]!=0x34) return 25;
        CheckItem(menu,1,true); if (((unsigned char *)*menu)[20]!=0x12) return 26;
        CheckItem(menu,1,false); if (((unsigned char *)*menu)[20]!=0) return 27;
        DisableItem(menu,1); if ((((unsigned char *)*menu)[13]&3)!=1) return 30;
        DisableItem(menu,0); if ((((unsigned char *)*menu)[13]&3)!=0) return 31;
        EnableItem(menu,0); if ((((unsigned char *)*menu)[13]&3)!=1) return 32;
        EnableItem(menu,1); if ((((unsigned char *)*menu)[13]&3)!=3) return 33;
        DisableItem(menu,32); EnableItem(menu,-1);
        if (((unsigned char *)*menu)[10]!=255 || ((unsigned char *)*menu)[11]!=255 || ((unsigned char *)*menu)[12]!=255 || ((unsigned char *)*menu)[13]!=255) return 34;

        SetItemCmd(menu,1,0x1250); if (((unsigned char *)*menu)[19]!=0x50) return 29;
        SetMenuItemText(menu,1,"\pLonger title");
        if (((unsigned char *)*menu)[16]!=12 || ((unsigned char *)*menu)[31]!=0 || CountMItems(menu)!=3) return 28;
        SetMenuItemText(menu,1,"\pA");
        SetMenuBar(bar); DisposeHandle(bar);
        if (*(Handle *)0xa1c!=live || GetMenuHandle(70)!=menu) return 19;
        DrawMenuBar();
        if ((pixels[64*8+2]&0x70)!=0x70 || qd.thePort!=&port) return 20;
        if (GetNewMBar(43)!=0 || ResError()!=-192 || GetMenuHandle(70)!=menu) return 21;
    }
    {
        Str255 item;
        MenuHandle created=NewMenu(-17,"\pTemporary");
        if (!created || (*created)->menuID!=-17 || CountMItems(created)!=0 || GetHandleSize((Handle)created)!=25) return 30;
        AppendMenu(created,"\pOne/O;Two");
        if (CountMItems(created)!=2 || ((unsigned char *)*created)[29]!='O') return 31;
        GetMenuItemText(created,1,item); if (item[0]!=3 || item[1]!='O' || item[2]!='n' || item[3]!='e') return 34;
        DisposeMenu(created);
    }
    InitCursor(); HideCursor(); HideCursor(); ShowCursor();
    cursor=GetCursor(-123); if (!cursor || GetHandleSize((Handle)cursor)!=68) return 9;
    SetCursor(*cursor); ShowCursor(); ShowCursor();
    MoveTo(23,29); PenSize(4,3); PenMode(patXor); PenPat(&qd.gray); PenNormal();
    if (port.pnSize.h!=1 || port.pnSize.v!=1 || port.pnMode!=patCopy || port.pnLoc.h!=23 || port.pnLoc.v!=29) return 32;
    { short i; for (i=0;i<8;i++) if (((unsigned char *)&port.pnPat)[i]!=255) return 33; }
    {
        PolyHandle polygon;
        MoveTo(100,100); polygon=OpenPoly();
        if (!polygon || port.pnVis!=-1 || !port.polySave) return 35;
        LineTo(108,100); LineTo(100,108); ClosePoly();
        if (port.pnVis || port.polySave || (*polygon)->polyBBox.left!=100 || (*polygon)->polyBBox.bottom!=108) return 36;
        PaintPoly(polygon); if (!(pixels[64*100+12]&8)) return 37;
        InvertPoly(polygon); if (pixels[64*100+12]&8) return 38;
        FillPoly(polygon,&qd.black); if (!(pixels[64*100+12]&8)) return 39;
        FramePoly(polygon); if (port.pnLoc.h!=100 || port.pnLoc.v!=108) return 40;
        ErasePoly(polygon); OffsetPoly(polygon,4,2);
        if ((*polygon)->polyBBox.left!=104 || (*polygon)->polyBBox.top!=102) return 41;
        HidePen(); if (port.pnVis!=-1) return 42; ShowPen(); if (port.pnVis) return 43;
        KillPoly(polygon);
    }
    ClosePort(&port); PostEvent(diskEvt,0x600d); ExitToShell(); return 0;
}
