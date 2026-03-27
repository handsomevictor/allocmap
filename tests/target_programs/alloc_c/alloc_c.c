#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <time.h>

void c_heavy_function() {
    size_t sz = 150 * 1024 * 1024;
    void* p = malloc(sz);
    if (!p) { perror("malloc"); return; }
    /* touch all pages so they're actually resident */
    char* cp = (char*)p;
    for (size_t i = 0; i < sz; i += 4096) cp[i] = (char)i;
    sleep(3);
    free(p);
}

int main() {
    srand((unsigned)time(NULL));
    printf("[alloc_c] started pid=%d\n", getpid());
    fflush(stdout);
    for (;;) {
        c_heavy_function();
        sleep(1);
    }
    return 0;
}
