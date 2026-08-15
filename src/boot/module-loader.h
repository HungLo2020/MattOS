/* Minimal initramfs module loader for the generated boot-critical closure. */
#ifndef MATTOS_MODULE_LOADER_H
#define MATTOS_MODULE_LOADER_H

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef MODULE_INIT_COMPRESSED_FILE
#define MODULE_INIT_COMPRESSED_FILE 4
#endif

static int mattos_load_boot_modules(void)
{
    FILE *list = fopen("/modules.load", "re");
    if (list == NULL)
        return -1;
    char path[1024];
    while (fgets(path, sizeof(path), list) != NULL) {
        path[strcspn(path, "\r\n")] = '\0';
        if (path[0] == '\0' || path[0] == '#')
            continue;
        int module = open(path, O_RDONLY | O_CLOEXEC);
        if (module < 0) {
            fclose(list);
            return -1;
        }
        int result = (int)syscall(SYS_finit_module, module, "",
                                  MODULE_INIT_COMPRESSED_FILE);
        int saved_errno = errno;
        close(module);
        if (result < 0 && saved_errno != EEXIST) {
            errno = saved_errno;
            fclose(list);
            return -1;
        }
    }
    if (ferror(list)) {
        fclose(list);
        return -1;
    }
    fclose(list);
    return 0;
}

#endif
