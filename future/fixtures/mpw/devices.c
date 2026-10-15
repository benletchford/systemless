#include <Types.h>
#include <Quickdraw.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>
QDGlobals qd;
int main(void) {
    GDHandle device;
    PixMapHandle pixels;
    CTabHandle colors;
    RGBColor rgb;
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
    (**colors).ctTable[0].rgb.red=0x1234;
    Index2Color(0,&rgb); if (rgb.red!=0x1234 || rgb.green!=65535 || rgb.blue!=65535) return 10;
    PostEvent(diskEvt,0xd311); ExitToShell(); return 0;
}
