#include <Types.h>
#include <Events.h>
#include <Processes.h>
int main(void) {
    EventRecord event;
    unsigned long start;
    SetEventMask(everyEvent);
    PostEvent(keyDown, 1);
    if (!WaitNextEvent(keyDownMask, &event, 10, 0) || event.message != 1) return 1;
    start = TickCount();
    if (WaitNextEvent(keyDownMask, &event, 3, 0) || event.what != nullEvent) return 2;
    if (TickCount() - start < 3) return 3;
    /* Acceptance host injects keyDown 42 during this suspended call. */
    if (!WaitNextEvent(keyDownMask, &event, 100, 0) || event.message != 42) return 4;
    PostEvent(diskEvt, 0x600D);
    ExitToShell();
    return 0;
}
