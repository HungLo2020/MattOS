/* MattOS installed-system initramfs: discover durable storage and switch_root. */
#define _GNU_SOURCE
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <unistd.h>

#include "../../../boot/module-loader.h"

static void fatal(const char *message) {
    dprintf(2, "mattos-installed-init: %s: %s\n", message, strerror(errno));
    for (;;) pause();
}
static void mkdir_ok(const char *path) {
    if (mkdir(path, 0755) < 0 && errno != EEXIST) fatal(path);
}
static int is_partition(const char *name) {
    char path[PATH_MAX];
    snprintf(path, sizeof(path), "/sys/class/block/%s/partition", name);
    return access(path, R_OK) == 0;
}
static int read_small_file(const char *path, char *buffer, size_t size) {
    int fd = open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0) return 0;
    ssize_t count = read(fd, buffer, size - 1);
    close(fd);
    if (count <= 0) return 0;
    buffer[count] = 0;
    while (count > 0 && (buffer[count - 1] == '\n' || buffer[count - 1] == '\r'))
        buffer[--count] = 0;
    return 1;
}
static int command_line_value(const char *key, char *value, size_t size) {
    char line[4096];
    if (!read_small_file("/proc/cmdline", line, sizeof(line))) return 0;
    size_t key_len = strlen(key);
    for (char *word = strtok(line, " "); word; word = strtok(NULL, " ")) {
        if (strncmp(word, key, key_len) == 0 && word[key_len] == '=') {
            snprintf(value, size, "%s", word + key_len + 1);
            return value[0] != 0;
        }
    }
    return 0;
}
static int storage_identity_matches(const char *expected_uuid) {
    char config[1024], expected[512];
    if (!read_small_file("/newroot/etc/mattos-storage.conf", config, sizeof(config))) return 0;
    snprintf(expected, sizeof(expected), "root_uuid=%s", expected_uuid);
    for (char *line = strtok(config, "\n"); line; line = strtok(NULL, "\n"))
        if (strcmp(line, expected) == 0) return 1;
    return 0;
}
static int try_installed_root(const char *device, const char *expected_uuid, const char *filesystem) {
    const char *options = strcmp(filesystem, "btrfs") == 0 ? "subvol=@,compress=zstd:3" : NULL;
    if (mount(device, "/newroot", filesystem, MS_NOATIME, options) < 0) return 0;
    int valid = access("/newroot/usr/lib/systemd/systemd", X_OK) == 0
        && access("/newroot/etc/mattos-installed-profile", R_OK) == 0
        && storage_identity_matches(expected_uuid);
    if (valid) return 1;
    if (umount("/newroot") < 0) fatal("unmount non-MattOS candidate");
    return 0;
}
static int find_installed_root(char *selected, size_t size, const char *expected_uuid, const char *filesystem) {
    DIR *directory = opendir("/sys/class/block");
    if (!directory) return 0;
    struct dirent *entry;
    while ((entry = readdir(directory))) {
        if (entry->d_name[0] == '.' || !is_partition(entry->d_name)) continue;
        char device[PATH_MAX];
        snprintf(device, sizeof(device), "/dev/%s", entry->d_name);
        if (try_installed_root(device, expected_uuid, filesystem)) {
            snprintf(selected, size, "%s", device);
            closedir(directory);
            return 1;
        }
    }
    closedir(directory);
    return 0;
}
int main(void) {
    mkdir_ok("/dev"); mkdir_ok("/proc"); mkdir_ok("/sys"); mkdir_ok("/newroot");
    if (mount("devtmpfs", "/dev", "devtmpfs", MS_NOSUID, "mode=0755") < 0 && errno != EBUSY) fatal("mount /dev");
    if (mount("proc", "/proc", "proc", MS_NOSUID | MS_NODEV | MS_NOEXEC, NULL) < 0) fatal("mount /proc");
    if (mount("sysfs", "/sys", "sysfs", MS_NOSUID | MS_NODEV | MS_NOEXEC, NULL) < 0) fatal("mount /sys");
    if (mattos_load_boot_modules() < 0) fatal("load boot-critical kernel modules");
    char expected_uuid[256] = {0};
    if (!command_line_value("mattos.root_uuid", expected_uuid, sizeof(expected_uuid))) {
        errno = EINVAL;
        fatal("missing stable mattos.root_uuid boot identity");
    }
    char filesystem[32] = {0};
    if (!command_line_value("mattos.root_fstype", filesystem, sizeof(filesystem))
        || (strcmp(filesystem, "btrfs") != 0 && strcmp(filesystem, "ext4") != 0)) {
        errno = EINVAL;
        fatal("missing or unsupported mattos.root_fstype");
    }
    char root[PATH_MAX] = {0};
    for (int i = 0; i < 100; i++) {
        if (find_installed_root(root, sizeof(root), expected_uuid, filesystem)) break;
        if (i == 99) { errno = ENODEV; fatal("find and mount installed MattOS root"); }
        usleep(100000);
    }
    dprintf(2, "mattos-installed-init: mounted stable UUID %s from %s\n", expected_uuid, root);
    if (mount(NULL, "/", NULL, MS_REC | MS_PRIVATE, NULL) < 0) fatal("private mounts");
    mkdir_ok("/newroot/dev"); mkdir_ok("/newroot/proc"); mkdir_ok("/newroot/sys");
    if (mount("/dev", "/newroot/dev", NULL, MS_MOVE, NULL) < 0) fatal("move /dev");
    if (mount("/proc", "/newroot/proc", NULL, MS_MOVE, NULL) < 0) fatal("move /proc");
    if (mount("/sys", "/newroot/sys", NULL, MS_MOVE, NULL) < 0) fatal("move /sys");
    if (chdir("/newroot") < 0 || mount("/newroot", "/", NULL, MS_MOVE, NULL) < 0
        || chroot(".") < 0 || chdir("/") < 0) fatal("switch root");
    char *argv[] = {"/usr/lib/systemd/systemd", NULL};
    execv(argv[0], argv);
    fatal("exec systemd");
}
