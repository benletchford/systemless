#include <Types.h>
#include <Memory.h>
#include <Resources.h>
#include <Sound.h>
#include <Events.h>
#include <Processes.h>
#include <MacErrors.h>
static unsigned char tailSamples[2]={192,224};
static SoundHeader tailHeader;
static volatile long spin;
static short mode,callbackCount,callbackError;
static pascal void completed(SndChannelPtr channel,SndCommand *command) {
    long i;
    if (mode==1) {
        if (command->param1!=11) callbackError=1;
        for (i=0;i<3000;++i) spin+=i;
    } else {
        if (command->param1!=12) callbackError=2;
        command->cmd=bufferCmd;command->param1=0;command->param2=(long)&tailHeader;
        if (SndDoCommand(channel,command,true)) callbackError=3;
    }
    ++callbackCount;
}
int main(void) {
    Handle first,second,third,bad;
    SignedByte saved;
    SndChannelPtr channel=0;
    EventRecord event;
    long free;
#if TARGET_CPU_PPC
    RoutineDescriptor callback=BUILD_ROUTINE_DESCRIPTOR(uppSndCallBackProcInfo,completed);
    SndCallBackUPP routine=(SndCallBackUPP)&callback;
#else
    SndCallBackUPP routine=completed;
#endif
    *((unsigned char *)0x260)=7;
    first=GetResource('snd ',128);if (!first) return 1;
    saved=HGetState(first);free=FreeMem();
    if (SndPlay(0, (SndListHandle)first,true)) return 2;
    if (HGetState(first)!=saved || FreeMem()!=free || *((unsigned short *)(*first+12))!=0x8051) return 3;
    PostEvent(diskEvt,0xa810);
    if (SndNewChannel(&channel,sampledSynth,initNoInterp,routine)) return 4;
    tailHeader.samplePtr=(Ptr)tailSamples;tailHeader.length=2;tailHeader.sampleRate=8000UL<<16;
    tailHeader.loopStart=0;tailHeader.loopEnd=0;tailHeader.encode=stdSH;tailHeader.baseFrequency=60;
    second=GetResource('snd ',129);if (!second) return 5;HLock(second);mode=1;
    if (SndPlay(channel, (SndListHandle)second,true) || callbackCount || channel->cmdInProgress.cmd!=bufferCmd) return 6;
    PostEvent(diskEvt,0xa811);
    WaitNextEvent(0,&event,1,0);
    if (callbackCount!=1 || callbackError) return 7;
    PostEvent(diskEvt,0xa814);
    HUnlock(second);ReleaseResource(second);
    third=GetResource('snd ',132);if (!third) return 8;HLock(third);mode=2;
    if (SndPlay(channel, (SndListHandle)third,false)) return 9;
    if (callbackCount!=2 || callbackError || channel->cmdInProgress.cmd!=bufferCmd) return 10;
    PostEvent(diskEvt,0xa812);
    while (channel->cmdInProgress.cmd) WaitNextEvent(0,&event,1,0);
    HUnlock(third);ReleaseResource(third);
    bad=GetResource('snd ',130);if (!bad) return 11;free=FreeMem();
    if (SndPlay(0, (SndListHandle)bad,false)!=badFormat || FreeMem()!=free) return 12;
    ReleaseResource(bad);ReleaseResource(first);
    if (SndDisposeChannel(channel,true)) return 13;
    PostEvent(diskEvt,0xa813);ExitToShell();return 0;
}
