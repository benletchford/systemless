#include <Types.h>
#include <Sound.h>
#include <Events.h>
#include <Processes.h>
static unsigned char samples[4]={0,128,255,64};
static SoundHeader header;
static short callbackError;
static SndChannelPtr replacement;
static pascal void recycle(SndChannelPtr channel, SndCommand *command) {
    SndCommand buffer;
    if (command->param1!=99 || SndDisposeChannel(channel,true)) {callbackError=3;return;}
    if (SndNewChannel(&replacement,sampledSynth,initNoInterp,0)) {callbackError=4;return;}
    buffer.cmd=bufferCmd;buffer.param1=0;buffer.param2=(long)&header;
    if (SndDoImmediate(replacement,&buffer)) callbackError=5;
    PostEvent(diskEvt,0xa807);
}
static pascal void completed(SndChannelPtr channel, SndCommand *command) {
    short amplitude=0;
    if (command->cmd!=callBackCmd || command->param1!=12 || command->param2!=0x1234abcd) callbackError=1;
    command->cmd=getAmpCmd;command->param1=0;command->param2=(long)&amplitude;
    if (SndDoImmediate(channel,command) || amplitude!=128) callbackError=2;
    ++channel->userInfo;
    PostEvent(diskEvt,0xa803);
}
int main(void) {
    SndChannelPtr channel=0;
    SndCommand command;
    EventRecord event;
#if TARGET_CPU_PPC
    RoutineDescriptor callback=BUILD_ROUTINE_DESCRIPTOR(uppSndCallBackProcInfo,completed);
    SndCallBackUPP routine=(SndCallBackUPP)&callback;
#else
    SndCallBackUPP routine=completed;
#endif
    *((unsigned char *)0x260)=7;
    if (SndNewChannel(&channel,sampledSynth,initNoInterp,routine)) return 1;
    header.samplePtr=(Ptr)samples;header.length=4;header.sampleRate=8000UL<<16;
    header.loopStart=0;header.loopEnd=0;header.encode=stdSH;header.baseFrequency=60;
    command.cmd=bufferCmd;command.param1=0;command.param2=(long)&header;
    if (SndDoCommand(channel,&command,true)) return 2;
    command.cmd=ampCmd;command.param1=128;command.param2=0;
    if (SndDoCommand(channel,&command,true)) return 3;
    command.cmd=bufferCmd;command.param1=0;command.param2=(long)&header;
    if (SndDoCommand(channel,&command,true)) return 4;
    command.cmd=callBackCmd;command.param1=12;command.param2=0x1234abcd;
    if (SndDoCommand(channel,&command,true) || channel->userInfo!=0) return 5;
    PostEvent(diskEvt,0xa802);
    WaitNextEvent(0,&event,1,0);
    if (channel->userInfo!=1 || callbackError || channel->cmdInProgress.cmd || channel->qHead!=channel->qTail) return 6;
    if (SndDisposeChannel(channel,true)) return 7;
    channel=0;
#if TARGET_CPU_PPC
    { RoutineDescriptor next=BUILD_ROUTINE_DESCRIPTOR(uppSndCallBackProcInfo,recycle); callback=next; }
    routine=(SndCallBackUPP)&callback;
#else
    routine=recycle;
#endif
    if (SndNewChannel(&channel,sampledSynth,initNoInterp,routine)) return 8;
    command.cmd=bufferCmd;command.param1=0;command.param2=(long)&header;
    if (SndDoCommand(channel,&command,true)) return 9;
    command.cmd=callBackCmd;command.param1=99;command.param2=0;
    if (SndDoCommand(channel,&command,true)) return 10;
    PostEvent(diskEvt,0xa808);
    WaitNextEvent(0,&event,1,0);
    if (callbackError || !replacement) return 11;
    while (replacement->cmdInProgress.cmd) WaitNextEvent(0,&event,1,0);
    if (SndDisposeChannel(replacement,true)) return 12;
    PostEvent(diskEvt,0xa806);ExitToShell();return 0;
}
