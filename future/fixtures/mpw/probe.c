#include <Types.h>
#include <Memory.h>
#include <Resources.h>
#include <Events.h>
#include <Processes.h>
int main(void) {
    Handle resource;
    Handle allocated;
    resource = GetResource('DATA', 128);
    if (resource == 0 || GetHandleSize(resource) != 3) return 1;
    allocated = NewHandleClear(12);
    if (allocated == 0 || **allocated != 0) return 2;
    ReleaseResource(allocated);
    if (ResError() != resNotFound || GetHandleSize(allocated) != 12) return 3;
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
