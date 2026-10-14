#include <Types.h>
#include <Quickdraw.h>
#include <Fonts.h>
#include <TextEdit.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>
QDGlobals qd;
int main(void) {
    GrafPort port;
    Rect dest={2,3,40,80}, view={1,1,50,90};
    TEHandle plain,styled;
    TEStyleHandle style;
    Handle text;
    InitGraf(&qd.thePort); InitFonts(); OpenPort(&port);
    TextFont(3); TextSize(9); TextFace(0); TextMode(srcOr);
    TEInit(); TEInit();
    plain=TENew(&dest,&view);
    if (!plain || (*plain)->teLength || (*plain)->nLines || (*plain)->txFont!=3 || (*plain)->txSize!=9 || (*plain)->inPort!=&port) return 1;
    if ((*plain)->destRect.left!=3 || (*plain)->viewRect.bottom!=50 || (*plain)->lineHeight<=0 || (*plain)->fontAscent<=0) return 2;
    text=TEGetText(plain);
    if (!text || text!=(*plain)->hText || GetHandleSize(text)!=0 || TEGetStyleHandle(plain)!=0) return 3;
    styled=TEStyleNew(&dest,&view);
    if (!styled || (*styled)->txSize!=-1 || (*styled)->lineHeight!=-1 || (*styled)->fontAscent!=-1) return 4;
    style=TEGetStyleHandle(styled);
    if (!style || (*style)->nRuns!=1 || (*style)->nStyles!=1 || !(*style)->styleTab || !(*style)->lhTab || !(*style)->nullStyle) return 5;
    if ((**(*style)->styleTab).stFont!=3 || (**(*style)->styleTab).stSize!=9 || (**(*style)->styleTab).stCount!=1) return 6;
    if ((*style)->runs[0].startChar!=0 || (*style)->runs[0].styleIndex!=0 || (*style)->runs[1].styleIndex!=-1) return 7;
    text=TEGetText(styled);
    if (!text || GetHandleSize(text)!=0 || text!=(*styled)->hText) return 8;
    TEDispose(plain); TEDispose(styled);
    if (MemError()) return 9;
    plain=TENew(&dest,&view); if (!plain) return 10; TEDispose(plain);
    ClosePort(&port);
    PostEvent(diskEvt,0x600D); ExitToShell(); return 0;
}
