#include <Types.h>
#include <CodeFragments.h>
static long *sharedCounter;
static long symbolCount;
OSErr PluginInit(const CFragInitBlock *block) {
    return CountSymbols(block->connectionID, &symbolCount);
}
long PluginMain(long *counter) {
    sharedCounter = counter;
    *sharedCounter += 1;
    return symbolCount;
}
void PluginTerm(void) {
    if (sharedCounter != 0) *sharedCounter += 10;
}
