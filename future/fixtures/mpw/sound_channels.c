#include <Types.h>
#include <Sound.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>
int main(void) {
    SndChannelPtr owned=0;
    SndChannelPtr caller=(SndChannelPtr)NewPtrClear(sizeof(SndChannel));
    SndChannelPtr saved=caller;
    Handle resource=NewHandleClear(8);
    if (!caller || !resource) return 1;
    caller->qLength=3;caller->userInfo=0x12345678;
    if (SndNewChannel(&owned,sampledSynth,initMono,0) || !owned || owned->qLength!=stdQLength) return 2;
    owned->userInfo=(long)resource;
    if (SndNewChannel(&caller,0,0,0) || caller!=saved || caller->userInfo!=0x12345678 || caller->qLength!=3) return 3;
    if (SndDisposeChannel(owned,false) || GetHandleSize(resource)!=8) return 4;
    if (caller->nextChan) return 5;
    if (SndDisposeChannel(caller,true) || GetPtrSize((Ptr)caller)!=sizeof(SndChannel) || caller->userInfo!=0x12345678) return 6;
    if (SndDisposeChannel(caller,true)!=badChannel || SndDisposeChannel(0,false)!=badChannel) return 7;
    DisposePtr((Ptr)caller);DisposeHandle(resource);
    PostEvent(7,0x50cd);ExitToShell();return 8;
}
