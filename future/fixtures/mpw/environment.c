#include <Types.h>
#include <OSUtils.h>
#include <Memory.h>
#include <MacErrors.h>
#include <Events.h>
#include <Processes.h>
#include <Files.h>
#include <Devices.h>
int main(void) {
    SysEnvRec world;
    Ptr p;
    CInfoPBRec catalog;
    Str255 name;
    long folder;
    HParamBlockRec file;
    if (SysEnvirons(1,&world) || world.environsVersion!=2 || world.machineType!=envMachUnknown || world.processor!=env68020 || world.systemVersion!=0x0700 || world.hasFPU || world.hasColorQD || world.keyBoardType!=envUnknownKbd || world.atDrvrVersNum || world.sysVRefNum) return 1;
    world.environsVersion=123;
    if (SysEnvirons(0,&world)!=envBadVers || world.environsVersion!=123) return 2;
    if (SysEnvirons(3,&world)!=envVersTooBig || world.environsVersion!=2) return 3;
    if (*(long *)0x130!=0x10400000 || *(long *)0x114!=0x10400000) return 4;
    *(long *)0x114=0x103ff000;
    MaxApplZone(); if (*(long *)0x114!=0x10400000 || GetApplLimit()!=(Ptr)0x10400000 || MemError()) return 5;
    p=NewPtr(16); if (!p || GetPtrSize(p)!=16) return 6;
    p[0]=42; MaxApplZone(); if (p[0]!=42) return 7; DisposePtr(p);
    catalog.dirInfo.ioNamePtr=name; catalog.dirInfo.ioVRefNum=0;
    catalog.dirInfo.ioFDirIndex=-1; catalog.dirInfo.ioDrDirID=2;
    if (PBGetCatInfoSync(&catalog) || catalog.dirInfo.ioResult || !(catalog.dirInfo.ioFlAttrib&16) || catalog.dirInfo.ioDrDirID!=2 || catalog.dirInfo.ioDrParID!=1 || catalog.dirInfo.ioDrNmFls!=1 || name[0]!=18) return 8;
    catalog.hFileInfo.ioFDirIndex=1; catalog.hFileInfo.ioDirID=2;
    if (PBGetCatInfoSync(&catalog) || catalog.hFileInfo.ioDirID!=16 || catalog.hFileInfo.ioFlParID!=2 || (!catalog.hFileInfo.ioFlLgLen && !catalog.hFileInfo.ioFlRLgLen) || !(catalog.hFileInfo.ioFlAttrib&1)) return 9;
    catalog.hFileInfo.ioFDirIndex=2; catalog.hFileInfo.ioDirID=2;
    if (PBGetCatInfoSync(&catalog)!=fnfErr || catalog.hFileInfo.ioResult!=fnfErr) return 10;
    catalog.dirInfo.ioNamePtr="\pPrefs"; catalog.dirInfo.ioVRefNum=0; catalog.dirInfo.ioDrDirID=2;
    if (PBDirCreateSync((HParmBlkPtr)&catalog) || catalog.dirInfo.ioResult || catalog.dirInfo.ioDrDirID<=16) return 11;
    folder=catalog.dirInfo.ioDrDirID;
    catalog.dirInfo.ioNamePtr=name; catalog.dirInfo.ioFDirIndex=-1;
    if (PBGetCatInfoSync(&catalog) || catalog.dirInfo.ioDrParID!=2 || catalog.dirInfo.ioDrNmFls || name[0]!=5) return 12;
    catalog.dirInfo.ioNamePtr="\pNested"; catalog.dirInfo.ioDrDirID=folder;
    if (PBDirCreateSync((HParmBlkPtr)&catalog)) return 13;
    catalog.dirInfo.ioDrDirID=folder;
    if (PBDirCreateSync((HParmBlkPtr)&catalog)!=dupFNErr) return 14;
    catalog.dirInfo.ioNamePtr=name; catalog.dirInfo.ioFDirIndex=-1; catalog.dirInfo.ioDrDirID=folder;
    if (PBGetCatInfoSync(&catalog) || catalog.dirInfo.ioDrNmFls!=1) return 15;
    catalog.dirInfo.ioNamePtr="\p:Prefs:Nested"; catalog.dirInfo.ioFDirIndex=0; catalog.dirInfo.ioDrDirID=2;
    if (PBGetCatInfoSync(&catalog) || catalog.dirInfo.ioDrParID!=folder) return 16;
    file.fileParam.ioNamePtr=name; file.fileParam.ioVRefNum=0; file.fileParam.ioFDirIndex=1; file.fileParam.ioDirID=2;
    if (PBHGetFInfoSync(&file) || file.fileParam.ioResult || file.fileParam.ioDirID!=16 || !(file.fileParam.ioFlAttrib&1) || (!file.fileParam.ioFlLgLen && !file.fileParam.ioFlRLgLen)) return 17;
    file.fileParam.ioDirID=folder;
    if (PBGetFInfoSync((ParmBlkPtr)&file)) return 18;
    file.fileParam.ioNamePtr="\pMissing"; file.fileParam.ioFDirIndex=0; file.fileParam.ioDirID=folder;
    if (PBHGetFInfoSync(&file)!=fnfErr || file.fileParam.ioResult!=fnfErr) return 19;
    {
        HParamBlockRec opened;
        ParamBlockRec io;
        Str255 settings="\pSettings";
        char bytes[8];
        short dataRef, resourceRef, i;
        for (i=0;i<sizeof(opened);i++) ((char *)&opened)[i]=0;
        for (i=0;i<sizeof(io);i++) ((char *)&io)[i]=0;
        opened.fileParam.ioNamePtr=settings; opened.fileParam.ioDirID=folder;
        if (PBHCreateSync(&opened)) return 20;
        if (PBHCreateSync(&opened)!=dupFNErr) return 21;
        opened.ioParam.ioPermssn=fsRdWrPerm;
        if (PBHOpenSync(&opened) || opened.ioParam.ioRefNum<=0) return 22;
        dataRef=opened.ioParam.ioRefNum;
        io.ioParam.ioRefNum=dataRef; io.ioParam.ioBuffer="ABCD"; io.ioParam.ioReqCount=4;
        if (PBWriteSync(&io) || io.ioParam.ioActCount!=4 || io.ioParam.ioPosOffset!=4) return 23;
        if (PBGetEOFSync(&io) || (long)io.ioParam.ioMisc!=4) return 24;
        io.ioParam.ioBuffer=bytes; io.ioParam.ioReqCount=8; io.ioParam.ioPosMode=fsFromStart; io.ioParam.ioPosOffset=0;
        if (PBReadSync(&io)!=eofErr || io.ioParam.ioActCount!=4 || bytes[0]!='A' || bytes[3]!='D') return 25;
        if (PBGetFPosSync(&io) || io.ioParam.ioPosOffset!=4) return 26;
        io.ioParam.ioPosOffset=1; io.ioParam.ioPosMode=fsFromStart;
        if (PBSetFPosSync(&io)) return 27;
        io.ioParam.ioMisc=(Ptr)2; if (PBSetEOFSync(&io)) return 28;
        if (PBFlushFileSync(&io)) return 29;
        opened.fileParam.ioDirID=folder;
        if (PBHOpenRFSync(&opened)) return 30;
        resourceRef=opened.ioParam.ioRefNum;
        io.ioParam.ioRefNum=resourceRef;io.ioParam.ioBuffer="R";io.ioParam.ioReqCount=1;io.ioParam.ioPosMode=fsFromStart;io.ioParam.ioPosOffset=0;
        if (PBWriteSync(&io) || io.ioParam.ioActCount!=1 || PBCloseSync(&io)) return 31;
        io.ioParam.ioRefNum=dataRef; if (PBCloseSync(&io) || PBCloseSync(&io)!=rfNumErr) return 32;
        for (i=0;i<sizeof(opened);i++) ((char *)&opened)[i]=0;
        opened.fileParam.ioNamePtr=settings; opened.fileParam.ioDirID=folder;
        if (PBHGetFInfoSync(&opened) || opened.fileParam.ioFlLgLen!=2 || opened.fileParam.ioFlRLgLen!=1) return 33;
        opened.fileParam.ioFlFndrInfo.fdType='TEXT';opened.fileParam.ioFlFndrInfo.fdCreator='FUTR';opened.fileParam.ioDirID=folder;
        if (PBHSetFInfoSync(&opened)) return 34;
        opened.fileParam.ioDirID=folder;
        if (PBHGetFInfoSync(&opened) || opened.fileParam.ioFlFndrInfo.fdType!='TEXT' || opened.fileParam.ioFlFndrInfo.fdCreator!='FUTR') return 35;
        opened.fileParam.ioDirID=folder;opened.ioParam.ioPermssn=fsRdPerm;
        if (PBHOpenSync(&opened)) return 36;
        io.ioParam.ioRefNum=opened.ioParam.ioRefNum;io.ioParam.ioBuffer=bytes;io.ioParam.ioReqCount=8;io.ioParam.ioPosMode=fsFromStart;io.ioParam.ioPosOffset=0;
        if (PBReadSync(&io)!=eofErr || io.ioParam.ioActCount!=2 || bytes[0]!='A' || bytes[1]!='B') return 37;
        if (PBCloseSync(&io)) return 38;
    }
    PostEvent(diskEvt,0x600d); ExitToShell(); return 0;
}
