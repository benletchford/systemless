#include <Types.h>
#include <Events.h>
#include <Processes.h>
int main(void) {
    EventRecord event;
    EvQElPtr head;
    if (PostEvent(keyUp, 9) != 1) return 1;
    SetEventMask(everyEvent);
    if (PostEvent(keyDown, 11) || PostEvent(keyUp, 12) || PostEvent(keyDown, 13)) return 2;
    head = (EvQElPtr)GetEvQHdr()->qHead;
    if (!head || head->qType != 4 || head->evtQWhat != keyDown || head->evtQMessage != 11) return 3;
    if (!OSEventAvail(keyDownMask, &event) || event.message != 11) return 4;
    if (!OSEventAvail(keyDownMask, &event) || event.message != 11) return 5;
    if (!GetOSEvent(keyUpMask, &event) || event.message != 12) return 6;
    FlushEvents(keyDownMask, 0);
    if (GetOSEvent(everyEvent, &event) || event.what != nullEvent) return 7;
    if (PostEvent(keyDown, 21) || PostEvent(diskEvt, 22) || PostEvent(keyDown, 23)) return 8;
    FlushEvents(keyDownMask, diskMask);
    if (!GetOSEvent(keyDownMask, &event) || event.message != 23) return 9;
    if (!GetOSEvent(diskMask, &event) || event.message != 22) return 10;
    if (GetEvQHdr()->qHead || GetEvQHdr()->qTail) return 11;
    /* A required success marker makes early 68K CRT returns distinguishable. */
    PostEvent(diskEvt, 0x600D);
    ExitToShell();
    return 0;
}
