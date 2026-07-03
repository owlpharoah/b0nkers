#include <stdlib.h>
#include <string.h>

int main() {
    while (1) {
        void *p = malloc(1024 * 1024);
        memset(p, 0, 1024 * 1024);
    }
}
