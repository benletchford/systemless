#include <setjmp.h>
static jmp_buf target;
static volatile int visits;
static void nested_jump(int value) {
    ++visits;
    longjmp(target, value);
}
int main(int argc, char **argv) {
    int result;
    if (argc != 0 || argv == 0 || argv[0] != 0) return 10;
    result = setjmp(target);
    if (result == 0) nested_jump(7);
    if (result != 7 || visits != 1) return 11;
    result = setjmp(target);
    if (result == 0) nested_jump(0);
    if (result != 1 || visits != 2) return 12;
    return 37;
}
