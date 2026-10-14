#include <Types.h>
#include <Memory.h>
#include <Resources.h>
#include <Events.h>
#include <Processes.h>
int main(void) {
    Handle resource;
    Handle allocated;
    Ptr original;
    resource = GetResource('DATA', 128);
    if (resource == 0 || GetHandleSize(resource) != 3) return 1;
    allocated = NewHandleClear(12);
    if (allocated == 0 || **allocated != 0) return 2;
    ReleaseResource(allocated);
    if (ResError() != resNotFound || GetHandleSize(allocated) != 12) return 3;
    if (StripAddress((Ptr)0xFEDCBA98) != (Ptr)0xFEDCBA98) return 12;
    original = *allocated;
    **allocated = 17;
    HLock(allocated);
    HLock(allocated);
    if (MemError() != noErr) return 7;
    SetHandleSize(allocated, 64);
    if (MemError() != memFullErr || *allocated != original) return 8;
    if (GetHandleSize(allocated) != 12 || **allocated != 17) return 9;
    HUnlock(allocated);
    if (MemError() != noErr) return 10;
    SetHandleSize(allocated, 64);
    if (MemError() != noErr || GetHandleSize(allocated) != 64 || **allocated != 17) return 11;
    DisposeHandle(allocated);
    **resource = 99;
    ReleaseResource(resource);
    if (ResError() != noErr) return 4;
    resource = GetResource('DATA', 128);
    if (resource == 0 || **resource != 1 || GetHandleSize(resource) != 3) return 5;
    ReleaseResource(resource);
    if (ResError() != noErr) return 6;
    ExitToShell();
    return 0;
}
