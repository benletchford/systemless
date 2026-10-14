#include <Types.h>
#include <Memory.h>
#include <Resources.h>
#include <CodeFragments.h>
#include <Events.h>
#include <Processes.h>
typedef long (*PluginProc)(long *counter);
int main(void) {
    Handle resource;
    CFragConnectionID connection;
    Ptr mainAddress;
    Ptr foundAddress;
    CFragSymbolClass symbolClass;
    Str255 errorName;
    long count;
    long counter;
    PluginProc plugin;
    resource = GetResource('plug', 128);
    if (resource == 0) return 1;
    HLock(resource);
    if (GetMemFragment(*resource, GetHandleSize(resource), "\pProbe Plugin", kPrivateCFragCopy,
            &connection, &mainAddress, errorName) != noErr) return 2;
    if (CountSymbols(connection, &count) != noErr || count != 1) return 3;
    if (FindSymbol(connection, "\pPluginMain", &foundAddress, &symbolClass) != noErr) return 4;
    if (foundAddress != mainAddress || symbolClass != kTVectorCFragSymbol) return 5;
    counter = 0;
    plugin = (PluginProc)mainAddress;
    if (plugin(&counter) != 1 || counter != 1) return 6;
    if (CloseConnection(&connection) != noErr || connection != 0 || counter != 11) return 7;
    HUnlock(resource);
    ReleaseResource(resource);
    if (ResError() != noErr) return 8;
    PostEvent(app4Evt, 0x600D);
    ExitToShell();
    return 0;
}
