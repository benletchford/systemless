#include <Types.h>
#include <Quickdraw.h>
#include <Fonts.h>
#include <TextEdit.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>
QDGlobals qd;
static char returnText[6]={'A','B',13,10,'C','D'};
int main(void) {
    GrafPort port;
    Rect dest={2,3,40,80}, view={1,1,50,90};
    TEHandle plain,styled;
    TEStyleHandle style;
    Handle text;
    Point savedPen,mouse;
    InitGraf(&qd.thePort); InitFonts(); OpenPort(&port);
    TextFont(3); TextSize(9); TextFace(0); TextMode(srcOr);
    TEInit(); TEInit();
    dest.right=dest.left+TextWidth("AB ",0,3);
    plain=TENew(&dest,&view);
    if (!plain || (*plain)->teLength || (*plain)->nLines || (*plain)->txFont!=3 || (*plain)->txSize!=9 || (*plain)->inPort!=&port) return 1;
    if ((*plain)->destRect.left!=3 || (*plain)->viewRect.bottom!=50 || (*plain)->lineHeight<=0 || (*plain)->fontAscent<=0) return 2;
    text=TEGetText(plain);
    if (!text || text!=(*plain)->hText || GetHandleSize(text)!=0 || TEGetStyleHandle(plain)!=0) return 3;
    if (TEGetHeight(32767,0,plain)!=0) return 30;
    styled=TEStyleNew(&dest,&view);
    if (!styled || (*styled)->txSize!=-1 || (*styled)->lineHeight!=-1 || (*styled)->fontAscent!=-1) return 4;
    style=TEGetStyleHandle(styled);
    if (!style || (*style)->nRuns!=1 || (*style)->nStyles!=1 || !(*style)->styleTab || !(*style)->lhTab || !(*style)->nullStyle) return 5;
    if ((**(*style)->styleTab).stFont!=3 || (**(*style)->styleTab).stSize!=9 || (**(*style)->styleTab).stCount!=1) return 6;
    if ((*style)->runs[0].startChar!=0 || (*style)->runs[0].styleIndex!=0 || (*style)->runs[1].styleIndex!=-1) return 7;
    text=TEGetText(styled);
    if (!text || GetHandleSize(text)!=0 || text!=(*styled)->hText) return 8;
    TESetText("AB CD EF",8,plain);
    if (TEGetText(plain)!=(*plain)->hText || (*plain)->teLength!=8 || (*plain)->selStart!=8 || (*plain)->selEnd!=8 || (*plain)->nLines!=3) return 11;
    if ((*plain)->lineStarts[0]!=0 || (*plain)->lineStarts[1]!=3 || (*plain)->lineStarts[2]!=6 || (*plain)->lineStarts[3]!=8) return 12;
    if (TEGetHeight(32767,0,plain)!=(long)(*plain)->lineHeight*3) return 31;
    TESetSelect(2,32767,plain);
    if ((*plain)->selStart!=2 || (*plain)->selEnd!=8) return 18;
    TESetSelect(3,3,styled);
    if ((*styled)->selStart || (*styled)->selEnd) return 19;
    (*plain)->destRect.right=(*plain)->destRect.left+200;
    TECalText(plain); if ((*plain)->nLines!=1 || (*plain)->lineStarts[1]!=8) return 13;
    TESetText("AB CD EF",8,styled);
    if ((*styled)->nLines!=3 || (*styled)->teLength!=8 || (*style)->runs[1].startChar!=9) return 14;
    if (GetHandleSize((Handle)(*style)->lhTab)!=16 || (**(*style)->lhTab).lhHeight<=0) return 15;
    TESetText(returnText,6,styled); if ((*styled)->nLines!=2 || (*styled)->lineStarts[1]!=4) return 16;
    if (TEGetHeight(32767,0,styled)!=(long)(**(*style)->styleTab).stHeight*2) return 32;
    TESetText(0,0,styled); if ((*styled)->teLength || (*styled)->nLines || GetHandleSize(TEGetText(styled))) return 17;
    if (TEGetHeight(1,1,styled)!=0) return 33;
    TEDispose(plain); TEDispose(styled);
    if (MemError()) return 9;
    plain=TENew(&dest,&view); if (!plain) return 10; TEDispose(plain);
    TextFace(bold); MoveTo(23,27); savedPen=port.pnLoc;
    TETextBox("AB CD EF",8,&view,teCenter);
    if (port.pnLoc.h!=savedPen.h || port.pnLoc.v!=savedPen.v || port.txFace!=bold) return 20;
    TETextBox(0,0,&view,teFlushRight);
    if (port.pnLoc.h!=savedPen.h || port.pnLoc.v!=savedPen.v || MemError()) return 21;
    (*(Point *)0x830).v=123; (*(Point *)0x830).h=234;
    GetMouse(&mouse);
    if (mouse.v!=123+port.portBits.bounds.top || mouse.h!=234+port.portBits.bounds.left) return 22;
    dest.top=10;dest.left=20;dest.bottom=30;dest.right=40;
    mouse.v=10;mouse.h=20;if (!PtInRect(mouse,&dest)) return 23;
    mouse.v=30;if (PtInRect(mouse,&dest)) return 24;
    mouse.v=29;mouse.h=40;if (PtInRect(mouse,&dest)) return 25;
    ClosePort(&port);
    PostEvent(diskEvt,0x600D); ExitToShell(); return 0;
}
