#include <Types.h>
#include <Quickdraw.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>
QDGlobals qd;
int main(void) {
    GDHandle device, offscreen;
    PixMapHandle pixels;
    CTabHandle colors;
    ITabHandle inverse;
    unsigned char *lookup;
    long tableSize;
    RGBColor rgb;
    offscreen=(GDHandle)NewHandleClear(sizeof(GDevice));if (!offscreen) return 11;
    (**offscreen).gdRefCon=0x12345678;
    SetDeviceAttribute(offscreen,noDriver,true);
    SetDeviceAttribute(offscreen,gdDevType,true);
    if (!TestDeviceAttribute(offscreen,noDriver) || !TestDeviceAttribute(offscreen,gdDevType) || (**offscreen).gdFlags!=0x4001) return 12;
    SetDeviceAttribute(offscreen,gdDevType,false);
    if (TestDeviceAttribute(offscreen,gdDevType) || (**offscreen).gdFlags!=0x4000 || (**offscreen).gdRefCon!=0x12345678) return 13;
    DisposeHandle((Handle)offscreen);
    InitGraf(&qd.thePort);
    device=GetGDevice();
    if (!device || GetMainDevice()!=device || GetDeviceList()!=device || GetNextDevice(device)) return 1;
    if (!TestDeviceAttribute(device,mainScreen) || !TestDeviceAttribute(device,screenDevice) || !TestDeviceAttribute(device,screenActive) || TestDeviceAttribute(device,gdDevType)) return 2;
    if (*(GDHandle *)0x08a4!=device || *(GDHandle *)0x08a8!=device || *(GDHandle *)0x0cc8!=device) return 3;
    pixels=(**device).gdPMap;
    if (!pixels || (**pixels).pixelSize!=1 || (**pixels).cmpCount!=1 || (**pixels).cmpSize!=1 || (**pixels).rowBytes!=(short)0x8040) return 4;
    if ((**pixels).baseAddr!=qd.screenBits.baseAddr || (**pixels).bounds.right!=qd.screenBits.bounds.right || (**pixels).bounds.bottom!=qd.screenBits.bounds.bottom) return 5;
    ((unsigned char *)(**pixels).baseAddr)[0]=0x80;
    if (*(unsigned char *)qd.screenBits.baseAddr!=0x80) return 6;
    if (GetHandleSize((Handle)device)!=sizeof(GDevice) || GetHandleSize((Handle)pixels)!=sizeof(PixMap)) return 7;
    Index2Color(0,&rgb); if (rgb.red!=65535 || rgb.green!=65535 || rgb.blue!=65535) return 8;
    Index2Color(1,&rgb); if (rgb.red || rgb.green || rgb.blue) return 9;
    colors=(**pixels).pmTable;
    inverse=(ITabHandle)NewHandle(2);if (!inverse) return 14;
    MakeITable(colors,inverse,3);if (QDError() || GetHandleSize((Handle)inverse)<518 || (**inverse).iTabSeed!=(**colors).ctSeed || (**inverse).iTabRes!=3) return 15;
    lookup=(unsigned char *)(*inverse);if (lookup[6]!=1 || lookup[517]!=0) return 16;
    tableSize=GetHandleSize((Handle)inverse);
    MakeITable(colors,inverse,2);if (QDError()!=-156 || GetHandleSize((Handle)inverse)!=tableSize) return 17;
    MakeITable(0,0,0);if (QDError() || !(**device).gdITable || (**((**device).gdITable)).iTabRes!=4) return 18;
    DisposeHandle((Handle)inverse);
    (**colors).ctTable[0].rgb.red=0x1234;
    Index2Color(0,&rgb); if (rgb.red!=0x1234 || rgb.green!=65535 || rgb.blue!=65535) return 10;
    offscreen=device;if (HandToHand((Handle *)&offscreen)) return 19;
    SetGDevice(offscreen);
    if (GetGDevice()!=offscreen || GetMainDevice()!=device || GetDeviceList()!=device || GetNextDevice(offscreen)) return 20;
    Index2Color(0,&rgb);if (rgb.red!=0x1234 || rgb.green!=65535 || rgb.blue!=65535) return 21;
    SetGDevice(device);if (GetGDevice()!=device) return 22;
    DisposeHandle((Handle)offscreen);
    PostEvent(diskEvt,0xd311); ExitToShell(); return 0;
}
