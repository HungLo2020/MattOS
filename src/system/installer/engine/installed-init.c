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
static int try_installed_root(const char *device, const char *expected_uuid) {
    if (mount(device, "/newroot", "btrfs", MS_NOATIME, "subvol=@,compress=zstd:3") < 0) return 0;
    int valid = access("/newroot/usr/lib/systemd/systemd", X_OK) == 0
        && access("/newroot/etc/mattos-installed-profile", R_OK) == 0
        && storage_identity_matches(expected_uuid);
    if (valid) return 1;
    if (umount("/newroot") < 0) fatal("unmount non-MattOS candidate");
    return 0;
}
static int find_installed_root(char *selected, size_t size, const char *expected_uuid) {
    DIR *directory = opendir("/sys/class/block");
    if (!directory) return 0;
    struct dirent *entry;
    while ((entry = readdir(directory))) {
        if (entry->d_name[0] == '.' || !is_partition(entry->d_name)) continue;
        char device[PATH_MAX];
        snprintf(device, sizeof(device), "/dev/%s", entry->d_name);
        if (try_installed_root(device, expected_uuid)) {
            snprintf(selected, size, "%s", device);
            closedir(directory);
            return 1;
        }
    }
    closedir(directory);
    return 0;
}
static int parent_disk(const char *partition, char *parent, size_t size) {
    char link[PATH_MAX], resolved[PATH_MAX];
    snprintf(link, sizeof(link), "/sys/class/block/%s", partition);
    if (!realpath(link, resolved)) return 0;
    char *slash = strrchr(resolved, '/');
    if (!slash) return 0;
    *slash = 0;
    slash = strrchr(resolved, '/');
    if (!slash || !slash[1]) return 0;
    snprintf(parent, size, "%s", slash + 1);
    return 1;
}
static int partition_number(const char *name) {
    char path[PATH_MAX], value[32];
    snprintf(path, sizeof(path), "/sys/class/block/%s/partition", name);
    if (!read_small_file(path, value, sizeof(value))) return -1;
    return atoi(value);
}
static int find_sibling_partition(const char *root_device, int number, char *device, size_t size) {
    const char *root_name = strrchr(root_device, '/');
    root_name = root_name ? root_name + 1 : root_device;
    char wanted_parent[256];
    if (!parent_disk(root_name, wanted_parent, sizeof(wanted_parent))) return 0;
    DIR *directory = opendir("/sys/class/block");
    if (!directory) return 0;
    struct dirent *entry;
    while ((entry = readdir(directory))) {
        char candidate_parent[256];
        if (entry->d_name[0] == '.' || partition_number(entry->d_name) != number
            || !parent_disk(entry->d_name, candidate_parent, sizeof(candidate_parent))
            || strcmp(candidate_parent, wanted_parent) != 0) continue;
        snprintf(device, size, "/dev/%s", entry->d_name);
        closedir(directory);
        return 1;
    }
    closedir(directory);
    return 0;
}
static void mount_installed_filesystems(const char *root) {
    if (mount(root, "/newroot/home", "btrfs", MS_NOATIME, "subvol=@home,compress=zstd:3") < 0)
        fatal("mount @home");
    if (mount(root, "/newroot/.snapshots", "btrfs", MS_NOATIME, "subvol=@snapshots,compress=zstd:3") < 0)
        fatal("mount @snapshots");
    char efi[PATH_MAX];
    if (!find_sibling_partition(root, 1, efi, sizeof(efi))) {
        errno = ENODEV;
        fatal("find EFI sibling partition");
    }
    if (mount(efi, "/newroot/boot/efi", "vfat", MS_NOSUID | MS_NODEV | MS_NOEXEC, "umask=0077") < 0)
        fatal("mount EFI system partition");
    dprintf(2, "mattos-installed-init: persistent mounts use %s and %s\n", root, efi);
}
int main(void) {
    mkdir_ok("/dev"); mkdir_ok("/proc"); mkdir_ok("/sys"); mkdir_ok("/newroot");
    if (mount("devtmpfs", "/dev", "devtmpfs", MS_NOSUID, "mode=0755") < 0 && errno != EBUSY) fatal("mount /dev");
    if (mount("proc", "/proc", "proc", MS_NOSUID | MS_NODEV | MS_NOEXEC, NULL) < 0) fatal("mount /proc");
    if (mount("sysfs", "/sys", "sysfs", MS_NOSUID | MS_NODEV | MS_NOEXEC, NULL) < 0) fatal("mount /sys");
    char expected_uuid[256] = {0};
    if (!command_line_value("mattos.root_uuid", expected_uuid, sizeof(expected_uuid))) {
        errno = EINVAL;
        fatal("missing stable mattos.root_uuid boot identity");
    }
    char root[PATH_MAX] = {0};
    for (int i = 0; i < 100; i++) {
        if (find_installed_root(root, sizeof(root), expected_uuid)) break;
        if (i == 99) { errno = ENODEV; fatal("find and mount installed MattOS Btrfs root"); }
        usleep(100000);
    }
    dprintf(2, "mattos-installed-init: mounted stable UUID %s from %s\n", expected_uuid, root);
    mount_installed_filesystems(root);
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
