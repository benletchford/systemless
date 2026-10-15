#include <Types.h>
#include <Memory.h>
#include <Resources.h>
#include <Events.h>
#include <Processes.h>
#include <MacErrors.h>
int main(void) {
    Handle h, r, fresh;
    SignedByte saved;
    Ptr original;
    h=NewHandle(16);if (!h || HGetState(h)!=0) return 1;
    saved=HGetState(h);HPurge(h);HLock(h);
    if ((unsigned char)HGetState(h)!=0xc0) return 2;
    HSetRBit(h);if ((unsigned char)HGetState(h)!=0xe0) return 3;
    HNoPurge(h);if ((unsigned char)HGetState(h)!=0xa0) return 4;
    HClrRBit(h);HUnlock(h);if (HGetState(h)!=0) return 5;
    HSetState(h,saved);original=*h;HLock(h);SetHandleSize(h,64);
    if (MemError()!=memFullErr || GetHandleSize(h)!=16 || *h!=original) return 6;
    HSetState(h,saved);HPurge(h);SetHandleSize(h,64);
    if (MemError() || GetHandleSize(h)!=64 || (unsigned char)HGetState(h)!=0x40) return 7;
    MoveHHi(h);if (MemError() || GetHandleSize(h)!=64 || (unsigned char)HGetState(h)!=0x40) return 15;
    HLock(h);original=*h;MoveHHi(h);if (MemError()!=memLockedErr || *h!=original) return 16;
    HUnlock(h);
    r=GetResource('DATA',128);if (!r || (unsigned char)HGetState(r)!=0xe0) return 8;
    original=*r;saved=HGetState(r);HUnlock(r);HNoPurge(r);
    if ((unsigned char)HGetState(r)!=0x20) return 9;
    HSetState(r,saved);DetachResource(r);
    if (*r!=original || (unsigned char)HGetState(r)!=0xc0) return 10;
    fresh=GetResource('DATA',128);if (!fresh || fresh==r || (unsigned char)HGetState(fresh)!=0xe0) return 11;
    ReleaseResource(fresh);HGetState(fresh);if (MemError()!=memWZErr) return 12;
    HGetState(0);if (MemError()!=nilHandleErr) return 13;
    HSetState(0,0);if (MemError()!=nilHandleErr) return 14;
    DisposeHandle(h);DisposeHandle(r);
    PostEvent(diskEvt,0xa069);ExitToShell();return 0;
}
