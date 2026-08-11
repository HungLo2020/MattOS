#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static void burn_cpu(long milliseconds) {
    struct timespec start, now;
    volatile unsigned long value = 1;
    clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &start);
    do {
        for (unsigned long i = 0; i < 100000; ++i)
            value = value * 1664525u + 1013904223u;
        clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &now);
    } while ((now.tv_sec - start.tv_sec) * 1000L +
             (now.tv_nsec - start.tv_nsec) / 1000000L < milliseconds);
    (void)value;
}

static void waited_children(int count, long milliseconds, int parallel) {
    pid_t children[8];
    for (int i = 0; i < count; ++i) {
        children[i] = fork();
        if (children[i] == 0) {
            burn_cpu(milliseconds);
            _exit(0);
        }
        if (!parallel)
            waitpid(children[i], NULL, 0);
    }
    if (parallel)
        for (int i = 0; i < count; ++i)
            waitpid(children[i], NULL, 0);
}

static void nested(long milliseconds) {
    pid_t child = fork();
    if (child == 0) {
        pid_t grandchild = fork();
        if (grandchild == 0) {
            burn_cpu(milliseconds);
            _exit(0);
        }
        waitpid(grandchild, NULL, 0);
        burn_cpu(milliseconds);
        _exit(0);
    }
    waitpid(child, NULL, 0);
}

int main(int argc, char **argv) {
    if (argc < 3)
        return 64;
    long milliseconds = strtol(argv[2], NULL, 10);
    if (!strcmp(argv[1], "direct"))
        burn_cpu(milliseconds);
    else if (!strcmp(argv[1], "one"))
        waited_children(1, milliseconds, 0);
    else if (!strcmp(argv[1], "sequential"))
        waited_children(2, milliseconds, 0);
    else if (!strcmp(argv[1], "parallel"))
        waited_children(2, milliseconds, 1);
    else if (!strcmp(argv[1], "nested"))
        nested(milliseconds);
    else if (!strcmp(argv[1], "idle")) {
        struct timespec delay = { .tv_sec = 0, .tv_nsec = milliseconds * 1000000L };
        nanosleep(&delay, NULL);
    } else if (!strcmp(argv[1], "fail")) {
        burn_cpu(milliseconds);
        return 7;
    } else
        return 64;
    return 0;
}
