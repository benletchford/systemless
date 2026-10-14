#include <Types.h>
#include <Memory.h>
#include <Resources.h>
#include <Events.h>
#include <Processes.h>
#include <CodeFragments.h>
#if TARGET_CPU_PPC
static CFragConnectionID futureConnection;
static long futureInitCount;
OSErr FutureInit(const CFragInitBlock *block) {
    futureConnection = block->connectionID;
    return CountSymbols(futureConnection, &futureInitCount);
}
#endif
int main(void) {
    Handle resource;
    Handle allocated;
    Ptr original;
    long symbolCount;
#if TARGET_CPU_PPC
    Ptr symbolAddress;
    Str255 symbolName;
    struct { CFragSymbolClass value; unsigned char guard; } symbolClass;
#endif
    symbolCount = 0x12345678;
    if (CountSymbols((CFragConnectionID)0, &symbolCount) != -2801 || symbolCount != 0x12345678) return 20;
#if TARGET_CPU_PPC
    if (futureConnection == 0 || futureInitCount != 1) return 21;
    if (CountSymbols(futureConnection, &symbolCount) != noErr || symbolCount != 1) return 22;
    symbolClass.guard = 0x5A;
    if (FindSymbol(futureConnection, "\pmain", &symbolAddress, &symbolClass.value) != noErr) return 23;
    if (symbolAddress != (Ptr)main || symbolClass.value != kTVectorCFragSymbol || symbolClass.guard != 0x5A) return 24;
    if (GetIndSymbol(futureConnection, 0, symbolName, &symbolAddress, &symbolClass.value) != noErr) return 25;
    if (symbolName[0] != 4 || symbolName[1] != 'm' || symbolName[2] != 'a' || symbolName[3] != 'i' || symbolName[4] != 'n') return 26;
    if (symbolAddress != (Ptr)main || symbolClass.value != kTVectorCFragSymbol || symbolClass.guard != 0x5A) return 27;
    if (GetIndSymbol(futureConnection, 1, 0, 0, 0) != -2802) return 28;
    if (FindSymbol(futureConnection, "\pMain", 0, 0) != -2802) return 29;
    if (FindSymbol(futureConnection, "\pmain", 0, 0) != noErr) return 30;
#endif
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
