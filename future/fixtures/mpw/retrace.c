#include <Types.h>
#include <Retrace.h>
#include <Events.h>
#include <Processes.h>
typedef struct RetraceRecord { VBLTask task; volatile long calls; volatile short active; } RetraceRecord;
static RetraceRecord record;
#if TARGET_CPU_68K
/* IM: Processes/TB35: capture the documented A0 task register with an inline result. */
static pascal VBLTaskPtr currentTask(void) ONEWORDINLINE(0x2e88);
static pascal void refresh(void) {
    VBLTaskPtr task=currentTask();
#else
static void refresh(VBLTaskPtr task) {
#endif
    RetraceRecord *r=(RetraceRecord *)task;
    ++r->calls;
    if ((*(QHdrPtr *)0x0d10)->qFlags&0x40) r->active=1;
    if (r->calls<3) task->vblCount=1;
}
int main(void) {
    EventRecord event;
#if TARGET_CPU_PPC
    RoutineDescriptor descriptor=BUILD_ROUTINE_DESCRIPTOR(uppVBLProcInfo,refresh);
    record.task.vblAddr=(VBLUPP)&descriptor;
#else
    record.task.vblAddr=(VBLUPP)refresh;
#endif
    record.task.qType=vType;record.task.vblCount=0;record.task.vblPhase=0;
    if (SlotVInstall((QElemPtr)&record,1)!=-360) return 1;
    if (SlotVInstall((QElemPtr)&record,0)) return 2;
    WaitNextEvent(0,&event,2,0);if (record.calls) return 3;
    record.task.vblCount=1;
    while (record.calls<3) WaitNextEvent(0,&event,1,0);
    if (!record.active || record.task.vblCount || record.task.qLink) return 4;
    if (SlotVRemove((QElemPtr)&record,0)!=-1) return 5;
    record.calls=0;record.task.vblCount=2;record.task.vblPhase=1;
    if (VInstall((QElemPtr)&record) || record.task.vblCount<1 || record.task.vblCount>3) return 6;
    if (GetVBLQHdr()->qHead!=(QElemPtr)&record || GetVBLQHdr()->qTail!=(QElemPtr)&record) return 7;
    if (VRemove((QElemPtr)&record) || VRemove((QElemPtr)&record)!=-1) return 8;
    WaitNextEvent(0,&event,3,0);if (record.calls) return 9;
    PostEvent(diskEvt,0xb033);ExitToShell();return 0;
}
