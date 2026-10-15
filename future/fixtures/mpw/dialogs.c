#include <Types.h>
#include <Dialogs.h>
#include <Memory.h>
#include <Events.h>
#include <Processes.h>
static unsigned char first[4]={3,'A',0x80,'B'};
static unsigned char second[3]={2,'C','D'};
static unsigned char empty[1]={0};
#if TARGET_CPU_PPC
#define readParams GetParamText
#else
/* The bundled CarbonAccessors.o is PPC-only; classic 68K reads DAStrings. */
static void readParams(unsigned char *a,unsigned char *b,unsigned char *c,unsigned char *d) {
    unsigned char *outputs[4],*source;
    Handle handle;
    int i,j;
    outputs[0]=a; outputs[1]=b; outputs[2]=c; outputs[3]=d;
    for (i=0;i<4;i++) {
        handle=((Handle *)0xaa0)[i];
        if (!handle) outputs[i][0]=0;
        else { source=(unsigned char *)*handle; for (j=0;j<=source[0];j++) outputs[i][j]=source[j]; }
    }
}
#endif
int main(void) {
    Str255 a,b,c,d;
    Handle h;
    *(short *)0xa98=123; *(short *)0xa9a=3; *(short *)0xafa=77;
    InitDialogs((ProcPtr)0x12345678);
    if (*(long *)0xa8c!=0x12345678 || (*(short *)0xa9a) || (*(short *)0xafa) || (*(long *)0xa9c) || (*(short *)0xa98)!=123) return 1;
    ParamText(first,second,0,0);
    h=*(Handle *)0xaa0;
    if (!h || GetHandleSize(h)!=4 || ((unsigned char *)*h)[0]!=3 || ((unsigned char *)*h)[2]!=0x80) return 2;
    readParams(a,b,c,d);
    if (a[0]!=3 || a[1]!='A' || a[2]!=0x80 || b[0]!=2 || b[2]!='D' || c[0] || d[0]) return 3;
    ParamText(0,empty,0,0); readParams(a,b,c,d);
    if (a[0]!=3 || b[0] || *(Handle *)0xaa0!=h) return 4;
    SetDialogFont(9); if (*(short *)0xafa!=9) return 5;
    ErrorSound((SoundUPP)0x87654321); if (*(long *)0xa9c!=0x87654321) return 6;
    InitDialogs(0);
    if (*(long *)0xa8c || *(long *)0xa9c || *(short *)0xafa || *(Handle *)0xaa0 || *(Handle *)0xaa4) return 7;
    readParams(a,b,c,d); if (a[0] || b[0] || c[0] || d[0] || MemError()) return 8;
    if (SetDialogDefaultItem(0,1)!=-50 || SetDialogDefaultItem(0,-1)!=-50) return 9;
    PostEvent(diskEvt,0x600d); ExitToShell(); return 0;
}
