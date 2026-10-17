#include <Types.h>
#include <Timer.h>
#include <Events.h>
#include <Processes.h>
typedef struct TimerRecord { TMTask task; volatile long calls; } TimerRecord;
static TimerRecord record;
#if TARGET_CPU_68K
/* IM: Processes, listing 3-3: capture A1 through a Pascal inline result.
   SC parameter pragmas describe call sites, not a C function entry thunk. */
static pascal TMTaskPtr currentTask(void) ONEWORDINLINE(0x2e89);
static pascal void tick(void) {
    TMTaskPtr task=currentTask();
#else
static void tick(TMTaskPtr task) {
#endif
    TimerRecord *timer=(TimerRecord *)task;
    ++timer->calls;
    if (timer->calls<3) PrimeTime((QElemPtr)task,1);
}
int main(void) {
    TMTask probe;
    EventRecord event;
#if TARGET_CPU_PPC
    RoutineDescriptor descriptor=BUILD_ROUTINE_DESCRIPTOR(uppTimerProcInfo,tick);
    record.task.tmAddr=(TimerUPP)&descriptor;
#else
    record.task.tmAddr=(TimerUPP)tick;
#endif
    probe.qLink=0;probe.qType=0;probe.tmAddr=0;probe.tmCount=0;
    probe.tmWakeUp=0;probe.tmReserved=0;
    InsTime((QElemPtr)&probe);PrimeTime((QElemPtr)&probe,100);
    WaitNextEvent(0,&event,1,0);RmvTime((QElemPtr)&probe);
    if (probe.tmCount>=0 || probe.tmCount<=-100000L || (probe.qType&kTMTaskActive)) return 1;
    InsTime((QElemPtr)&record);PrimeTime((QElemPtr)&record,-1000);
    while (record.calls<3) WaitNextEvent(0,&event,1,0);
    if (record.task.qType&kTMTaskActive) return 2;
    RmvTime((QElemPtr)&record);
    PostEvent(diskEvt,0xa05a);ExitToShell();return 0;
}
