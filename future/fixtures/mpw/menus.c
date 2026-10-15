#include <Types.h>
#include <Quickdraw.h>
#include <Windows.h>
#include <Menus.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>
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
    InitCursor(); HideCursor(); HideCursor(); ShowCursor();
    cursor=GetCursor(-123); if (!cursor || GetHandleSize((Handle)cursor)!=68) return 9;
    SetCursor(*cursor); ShowCursor(); ShowCursor();
    ClosePort(&port); PostEvent(diskEvt,0x600d); ExitToShell(); return 0;
}
