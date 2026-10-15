#include <Types.h>
#include <Quickdraw.h>
#include <Fonts.h>
#include <Windows.h>
#include <StandardFile.h>
#include <Files.h>
#include <Devices.h>
#include <MacErrors.h>
#include <Events.h>
#include <Processes.h>
QDGlobals qd;
int main(void) {
    Point where={60,70};
    SFReply reply;
    WDPBRec wd={0};
    ParamBlockRec file={0};
    InitGraf(&qd.thePort); InitFonts(); InitWindows();
    SFPutFile(where,"\pSave fixture as:","\pFixture",0,&reply);
    if (reply.good) {
        if (reply.fName[0]!=7 || reply.fName[1]!='F' || reply.fName[7]!='e') return 1;
        wd.ioVRefNum=reply.vRefNum;
        if (PBGetWDInfoSync(&wd) || wd.ioWDVRefNum!=-1 || wd.ioWDDirID!=2) return 2;
        file.ioParam.ioNamePtr=reply.fName; file.ioParam.ioVRefNum=reply.vRefNum;
        if (PBDeleteSync(&file)!=fnfErr || PBCreateSync(&file)) return 3;
        file.ioParam.ioPermssn=3;
        if (PBOpenSync(&file)) return 4;
        file.ioParam.ioBuffer="ABC"; file.ioParam.ioReqCount=3;
        file.ioParam.ioPosMode=fsFromStart; file.ioParam.ioPosOffset=0;
        if (PBWriteSync(&file) || file.ioParam.ioActCount!=3 || PBCloseSync(&file)) return 5;
        if (PBGetFInfoSync(&file) || file.fileParam.ioFlLgLen!=3) return 6;
    }
    PostEvent(7,reply.good?0x600d:0x600c);
    ExitToShell();
    return 7;
}
