#include <Types.h>
#include <Files.h>
#include <Devices.h>
#include <MacErrors.h>
#include <Events.h>
#include <Processes.h>
int main(void) {
    ParamBlockRec pb={0};
    ParamBlockRec resource={0};
    HParamBlockRec folder={0};
    char data[7];
    short fresh;
    folder.fileParam.ioNamePtr="\pSaves"; folder.fileParam.ioVRefNum=-1;
    folder.fileParam.ioDirID=2;
    { short e=PBDirCreateSync(&folder); if (e && e!=dupFNErr) return 1; }
    pb.ioParam.ioNamePtr="\p:Saves:Game";pb.ioParam.ioVRefNum=-1;
    fresh=PBGetFInfoSync(&pb)==fnfErr;
    if (fresh) {
        if (PBCreateSync(&pb)) return 2;
        pb.fileParam.ioFlFndrInfo.fdType='SAVE';pb.fileParam.ioFlFndrInfo.fdCreator='FUTR';
        if (PBSetFInfoSync(&pb)) return 3;
    } else if (pb.fileParam.ioFlFndrInfo.fdType!='SAVE' || pb.fileParam.ioFlLgLen!=7 || pb.fileParam.ioFlRLgLen!=4) return 4;
    pb.ioParam.ioPermssn=3;
    if (PBOpenSync(&pb)) return 5;
    pb.ioParam.ioBuffer=fresh?"PERSIST":data;pb.ioParam.ioReqCount=7;pb.ioParam.ioPosMode=fsFromStart;
    if (fresh ? PBWriteSync(&pb) : PBReadSync(&pb)) return 6;
    if (!fresh && (data[0]!='P' || data[6]!='T')) return 7;
    if (PBCloseSync(&pb)) return 8;
    resource.ioParam.ioNamePtr="\p:Saves:Game";resource.ioParam.ioVRefNum=-1;resource.ioParam.ioPermssn=3;
    if (PBOpenRFSync(&resource)) return 9;
    resource.ioParam.ioBuffer=fresh?"RSRC":data;resource.ioParam.ioReqCount=4;resource.ioParam.ioPosMode=fsFromStart;
    if (fresh ? PBWriteSync(&resource) : PBReadSync(&resource)) return 10;
    if (!fresh && (data[0]!='R' || data[3]!='C')) return 11;
    if (PBCloseSync(&resource)) return 12;
    PostEvent(7,fresh?0x5a01:0x5a02);
    ExitToShell();return 13;
}
