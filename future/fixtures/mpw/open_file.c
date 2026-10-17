#include <Types.h>
#include <Quickdraw.h>
#include <Fonts.h>
#include <Windows.h>
#include <StandardFile.h>
#include <Files.h>
#include <Devices.h>
#include <Events.h>
#include <Processes.h>
QDGlobals qd;
static long filtered;
static pascal Boolean filter(CInfoPBPtr pb) {
    if (pb->hFileInfo.ioFlFndrInfo.fdType != 'GAME') return true;
    if (pb->hFileInfo.ioFlLgLen != 3) return true;
    ++filtered;
    PostEvent(7, 0xf17e);
    return pb->hFileInfo.ioNamePtr[1]=='H';
}
static short create_fixture(StringPtr name) {
    ParamBlockRec pb={0};
    pb.ioParam.ioNamePtr=name; pb.ioParam.ioVRefNum=-1;
    if (PBCreateSync(&pb)) return 1;
    pb.fileParam.ioFlFndrInfo.fdType='GAME';
    pb.fileParam.ioFlFndrInfo.fdCreator='FUTR';
    if (PBSetFInfoSync(&pb)) return 2;
    pb.ioParam.ioPermssn=3;
    if (PBOpenSync(&pb)) return 3;
    pb.ioParam.ioBuffer="ABC"; pb.ioParam.ioReqCount=3; pb.ioParam.ioPosMode=fsFromStart;
    if (PBWriteSync(&pb) || PBCloseSync(&pb)) return 4;
    return 0;
}
int main(void) {
    Point where={60,70};
    SFReply reply;
    SFTypeList types={'GAME',0,0,0};
    ParamBlockRec pb={0};
    char data[3];
    InitGraf(&qd.thePort); InitFonts(); InitWindows();
    if (create_fixture("\pHidden") || create_fixture("\pVisible")) return 1;
#if TARGET_CPU_PPC
    {
        RoutineDescriptor descriptor=BUILD_ROUTINE_DESCRIPTOR(uppFileFilterProcInfo,filter);
        SFGetFile(where,0,(FileFilterUPP)&descriptor,1,types,0,&reply);
    }
#else
    SFGetFile(where,0,filter,1,types,0,&reply);
#endif
    if (filtered!=2) return 2;
    if (reply.good) {
        if (reply.fType!='GAME' || reply.fName[0]!=7 || reply.fName[1]!='V') return 3;
        pb.ioParam.ioNamePtr=reply.fName; pb.ioParam.ioVRefNum=reply.vRefNum;
        if (PBOpenSync(&pb)) return 4;
        pb.ioParam.ioBuffer=data; pb.ioParam.ioReqCount=3; pb.ioParam.ioPosMode=fsFromStart;
        if (PBReadSync(&pb) || PBCloseSync(&pb) || data[0]!='A' || data[2]!='C') return 5;
    }
    PostEvent(7,reply.good?0x0bed:0x0bec);
    ExitToShell();
    return 6;
}
