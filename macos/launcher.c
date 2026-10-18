/* A native bundle entry; no Python or extra GUI runtime is needed.
 * 原生 app 入口；科研缓存位于用户目录，不写入 app 资源。
 */
#include <errno.h>
#include <limits.h>
#include <mach-o/dyld.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static void fail(const char *operation) {
    perror(operation);
    exit(1);
}

static void path_join(char *output, const char *parent, const char *suffix) {
    if (snprintf(output, PATH_MAX, "%s/%s", parent, suffix) >= PATH_MAX) {
        errno = ENAMETOOLONG;
        fail("FitsViewer path");
    }
}

static void directory(const char *path) {
    if (mkdir(path, 0700) != 0 && errno != EEXIST) fail("FitsViewer cache");
}

int main(int argc, char **argv) {
    char executable[PATH_MAX], resolved[PATH_MAX], resources[PATH_MAX];
    char binary[PATH_MAX], web[PATH_MAX], cache[PATH_MAX], runtime[PATH_MAX];
    uint32_t size = sizeof(executable);
    if (_NSGetExecutablePath(executable, &size) != 0) {
        errno = ENAMETOOLONG;
        fail("FitsViewer executable");
    }
    if (!realpath(executable, resolved)) fail("FitsViewer executable");
    char *last = strrchr(resolved, '/');
    if (!last) { errno = EINVAL; fail("FitsViewer bundle"); }
    *last = '\0';
    path_join(executable, resolved, "../Resources");
    if (!realpath(executable, resources)) fail("FitsViewer resources");
    path_join(binary, resources, "bin/fitsviewer");
    path_join(web, resources, "web");
    if (setenv("FITSVIEWER_HOME", resources, 1) != 0) fail("FitsViewer resources");
    if (!getenv("FITSVIEWER_RUNTIME_DIR")) {
        const char *user = getenv("HOME");
        if (!user || !*user) { errno = EINVAL; fail("FitsViewer HOME"); }
        path_join(cache, user, "Library/Caches");
        directory(cache);
        path_join(runtime, cache, "FitsViewer");
        directory(runtime);
        path_join(cache, runtime, "runtime");
        directory(cache);
        if (setenv("FITSVIEWER_RUNTIME_DIR", cache, 1) != 0) fail("FitsViewer cache");
    }
    char **arguments = calloc((size_t)argc + 5, sizeof(char *));
    if (!arguments) fail("FitsViewer arguments");
    arguments[0] = binary;
    int count = 1;
    int explicit_mode = 0;
    int explicit_web = 0;
    for (int i = 1; i < argc; ++i) {
        if (strcmp(argv[i], "--serve") == 0 || strcmp(argv[i], "--browser") == 0) explicit_mode = 1;
        if (strcmp(argv[i], "--web-dir") == 0 || strncmp(argv[i], "--web-dir=", 10) == 0) explicit_web = 1;
    }
    if (!explicit_mode) arguments[count++] = "--native";
    if (!explicit_web) {
        arguments[count++] = "--web-dir";
        arguments[count++] = web;
    }
    for (int i = 1; i < argc; ++i) {
        if (strncmp(argv[i], "-psn_", 5) != 0) arguments[count++] = argv[i];
    }
    execv(binary, arguments);
    fail("FitsViewer launch");
    return 1;
}
