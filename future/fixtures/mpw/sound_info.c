#include <Types.h>
#include <Memory.h>
#include <Resources.h>
#include <Sound.h>
#include <Gestalt.h>
#include <Events.h>
#include <Processes.h>
#include <MacErrors.h>
static short samples[4]={1000,3000,-2000,-4000};
static ExtSoundHeader header;
static short callbackCount,callbackError;
static pascal void finished(SndChannelPtr channel,SndCommand *command) {
    SCStatus status;
    SMStatus manager;
    if (command->param1!=19) callbackError=1;
    if (SndChannelStatus(channel,sizeof(status),&status) || !status.scChannelBusy || status.scChannelPaused || status.scChannelAttributes!=0xc4) callbackError=2;
    if (SndManagerStatus(sizeof(manager),&manager) || manager.smNumChannels!=1) callbackError=3;
    ++callbackCount;
}
int main(void) {
    NumVersion version;
    long attributes;
    SndChannelPtr channel=0;
    SndCommand command;
    SCStatus status;
    SMStatus manager;
    EventRecord event;
    Handle resource;
#if TARGET_CPU_PPC
    RoutineDescriptor descriptor=BUILD_ROUTINE_DESCRIPTOR(uppSndCallBackProcInfo,finished);
    SndCallBackUPP routine=(SndCallBackUPP)&descriptor;
#else
    SndCallBackUPP routine=finished;
#endif
    *((unsigned char *)0x260)=7;
    version=SndSoundManagerVersion();
    if (version.majorRev!=3 || version.minorAndBugRev!=0 || version.stage!=finalStage || version.nonRelRev) return 1;
    if (Gestalt(gestaltSoundAttr,&attributes) || attributes!=0x1801) return 2;
    if (SndManagerStatus(sizeof(manager),&manager) || manager.smNumChannels) return 3;
    if (SndNewChannel(&channel,sampledSynth,initStereo+initNoInterp,routine)) return 4;
    if (SndChannelStatus(channel,sizeof(status),&status) || status.scChannelBusy || status.scChannelAttributes!=0xc4) return 5;
    header.samplePtr=(Ptr)samples;header.numChannels=2;header.sampleRate=8000UL<<16;
    header.encode=extSH;header.baseFrequency=60;header.numFrames=2;header.sampleSize=16;
    command.cmd=bufferCmd;command.param1=0;command.param2=(long)&header;
    if (SndDoCommand(channel,&command,true)) return 6;
    command.cmd=callBackCmd;command.param1=19;command.param2=0;
    if (SndDoCommand(channel,&command,true)) return 7;
    if (SndChannelStatus(channel,sizeof(status),&status) || !status.scChannelBusy) return 8;
    PostEvent(diskEvt,0xa820);WaitNextEvent(0,&event,1,0);
    if (callbackCount!=1 || callbackError) return 9;
    if (SndChannelStatus(channel,sizeof(status),&status) || status.scChannelBusy) return 10;
    if (SndDisposeChannel(channel,true)) return 11;
    channel=0;
    if (SndNewChannel(&channel,sampledSynth,initMono+initNoInterp,0)) return 12;
    resource=GetResource('snd ',128);if (!resource) return 13;
    PostEvent(diskEvt,0xa822);
    if (SndPlay(channel,(SndListHandle)resource,false)) return 14;
    if (SndChannelStatus(channel,sizeof(status),&status) || status.scChannelBusy || status.scChannelAttributes!=0x84) return 15;
    status.scChannelAttributes=0x12345678;
    if (SndChannelStatus(0,sizeof(status),&status)!=badChannel || status.scChannelAttributes!=0x12345678) return 16;
    manager.smNumChannels=123;
    if (SndManagerStatus(5,&manager)!=paramErr || manager.smNumChannels!=123) return 17;
    ReleaseResource(resource);
    if (SndDisposeChannel(channel,true) || SndManagerStatus(sizeof(manager),&manager) || manager.smNumChannels) return 18;
    PostEvent(diskEvt,0xa823);ExitToShell();return 0;
}
