#include <Types.h>
#include <Events.h>
#include <Quickdraw.h>
#include <Processes.h>
int main(void) {
    EventRecord event;
    unsigned long start;
    RgnHandle mouseRegion;
    Rect inside={0,0,10,10};
    SetEventMask(everyEvent);
    PostEvent(keyDown, 1);
    if (!WaitNextEvent(keyDownMask, &event, 10, 0) || event.message != 1) return 1;
    start = TickCount();
    if (WaitNextEvent(keyDownMask, &event, 3, 0) || event.what != nullEvent) return 2;
    if (TickCount() - start < 3) return 3;
    /* Acceptance host injects keyDown 42 during this suspended call. */
    if (!WaitNextEvent(keyDownMask, &event, 100, 0) || event.message != 42) return 4;
    mouseRegion=NewRgn(); if (!mouseRegion) return 5;
    RectRgn(mouseRegion,&inside);
    /* Host moves from (0,0) to (20,20) while this call is suspended. */
    if (!WaitNextEvent(osMask,&event,50,mouseRegion) || event.what!=osEvt || (unsigned long)event.message!=0xfa000000 || event.where.h!=20 || event.where.v!=20) return 6;
    DisposeRgn(mouseRegion);
    PostEvent(diskEvt, 0x600D);
    ExitToShell();
    return 0;
}
