#include <stdio.h>
#include <string.h>

static const char VERSION[] = "1.0.0";

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--version") == 0) {
        printf("fixture-tool %s\n", VERSION);
        return 0;
    }

    puts("fixture-tool: generated test binary");
    return 0;
}
