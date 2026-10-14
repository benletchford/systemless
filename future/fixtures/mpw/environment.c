#include <Types.h>
#include <OSUtils.h>
#include <Memory.h>
#include <MacErrors.h>
#include <Events.h>
#include <Processes.h>
int main(void) {
    SysEnvRec world;
    Ptr p;
    if (SysEnvirons(1,&world) || world.environsVersion!=2 || world.machineType!=envMachUnknown || world.processor!=env68020 || world.systemVersion!=0x0700 || world.hasFPU || world.hasColorQD || world.keyBoardType!=envUnknownKbd || world.atDrvrVersNum || world.sysVRefNum) return 1;
    world.environsVersion=123;
    if (SysEnvirons(0,&world)!=envBadVers || world.environsVersion!=123) return 2;
    if (SysEnvirons(3,&world)!=envVersTooBig || world.environsVersion!=2) return 3;
    if (*(long *)0x130!=0x10400000 || *(long *)0x114!=0x10400000) return 4;
    *(long *)0x114=0x103ff000;
    MaxApplZone(); if (*(long *)0x114!=0x10400000 || GetApplLimit()!=(Ptr)0x10400000 || MemError()) return 5;
    p=NewPtr(16); if (!p || GetPtrSize(p)!=16) return 6;
    p[0]=42; MaxApplZone(); if (p[0]!=42) return 7; DisposePtr(p);
    PostEvent(diskEvt,0x600d); ExitToShell(); return 0;
}
