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
    DisposeHandle(allocated);
    ExitToShell();
    return 0;
}
