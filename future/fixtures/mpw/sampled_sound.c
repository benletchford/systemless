#include <Types.h>
#include <Sound.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>
static unsigned char samples[4]={0,128,255,64};
static SoundHeader header;
int main(void) {
    SndChannelPtr channel=0;
    SndCommand command;
    EventRecord event;
    short amplitude=0;
    long volume=0;
    *((unsigned char *)0x260)=7;
    if (SndNewChannel(&channel,sampledSynth,initNoInterp,0)) return 1;
    command.cmd=ampCmd;command.param1=255;command.param2=0;
    if (SndDoImmediate(channel,&command)) return 2;
    command.cmd=getAmpCmd;command.param1=0;command.param2=(long)&amplitude;
    if (SndDoImmediate(channel,&command) || amplitude!=255) return 3;
    command.cmd=volumeCmd;command.param2=0x01000100;
    if (SndDoImmediate(channel,&command)) return 4;
    command.cmd=getVolumeCmd;command.param2=(long)&volume;
    if (SndDoImmediate(channel,&command) || volume!=0x01000100) return 5;
    header.samplePtr=(Ptr)samples;header.length=4;header.sampleRate=8000UL<<16;
    header.loopStart=0;header.loopEnd=0;header.encode=stdSH;header.baseFrequency=60;
    command.cmd=bufferCmd;command.param2=(long)&header;
    if (SndDoImmediate(channel,&command) || channel->cmdInProgress.cmd!=bufferCmd) return 6;
    PostEvent(diskEvt,0xa804);
    WaitNextEvent(0,&event,1,0);
    if (channel->cmdInProgress.cmd!=0) return 7;
    if (SndDisposeChannel(channel,true)) return 8;
    PostEvent(diskEvt,0xa805);ExitToShell();return 0;
}
