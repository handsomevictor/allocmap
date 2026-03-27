#include <cstdio>
#include <vector>
#include <thread>
#include <chrono>
#include <cstdlib>
#include <ctime>
#include <unistd.h>

void cpp_vector_alloc() {
    size_t mb = 100 + rand() % 200;  // 100-300 MB
    printf("[alloc_cpp] allocating %zu MB\n", mb);
    fflush(stdout);
    std::vector<uint8_t> v(mb * 1024 * 1024, 0);
    std::this_thread::sleep_for(std::chrono::seconds(3));
}

int main() {
    srand((unsigned)time(nullptr));
    printf("[alloc_cpp] started pid=%d\n", getpid());
    fflush(stdout);
    for (;;) {
        cpp_vector_alloc();
        std::this_thread::sleep_for(std::chrono::seconds(1));
    }
    return 0;
}
