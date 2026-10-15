#include <Types.h>
#include <Files.h>
#include <MacErrors.h>
#include <Devices.h>
#include <Events.h>
#include <Processes.h>
int main(void) {
    CInfoPBRec folder={0};
    WDPBRec wd={0};
    ParamBlockRec file={0};
    short reference;
    long directory;
    folder.dirInfo.ioNamePtr=(StringPtr)"\pSaves";
    folder.dirInfo.ioVRefNum=-1;
    folder.dirInfo.ioDrDirID=2;
    if (PBDirCreateSync((HParmBlkPtr)&folder)) return 1;
    directory=folder.dirInfo.ioDrDirID;
    wd.ioVRefNum=-1; wd.ioWDDirID=directory; wd.ioWDProcID='FUTR';
    if (PBOpenWDSync(&wd)) return 2;
    reference=wd.ioVRefNum;
    if (reference>=-32000 || PBOpenWDSync(&wd) || wd.ioVRefNum!=reference) return 3;
    wd.ioWDIndex=0;
    if (PBGetWDInfoSync(&wd) || wd.ioWDVRefNum!=-1 || wd.ioWDDirID!=directory || wd.ioWDProcID!='FUTR') return 4;
    file.ioParam.ioNamePtr=(StringPtr)"\pGame"; file.ioParam.ioVRefNum=reference;
    if (PBCreateSync(&file)) return 5;
    file.ioParam.ioPermssn=3;
    if (PBOpenSync(&file) || PBCloseSync(&file)) return 6;
    file.fileParam.ioFDirIndex=0;
    if (PBGetFInfoSync(&file)) return 7;
    file.ioParam.ioVRefNum=-1;
    if (PBGetFInfoSync(&file)!=fnfErr) return 8;
    if (PBCloseWDSync(&wd)) return 9;
    if (PBGetWDInfoSync(&wd)!=rfNumErr) return 10;
    PostEvent(7,0x600d);
    ExitToShell();
    return 11;
}
