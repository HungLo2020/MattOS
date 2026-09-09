/*
 * MattOS live-media early userspace.
 *
 * This deliberately small, statically linked PID 1 is the complete early
 * initramfs policy.  It mounts the ISO, attaches the immutable SquashFS root
 * to a loop device, creates a tmpfs-backed writable overlay, moves the kernel
 * API filesystems into the new root, and executes the real MattOS init.
 * General userland belongs in the live root and must never be added here.
 */
#define _GNU_SOURCE

#include <errno.h>
#include <dirent.h>
#include <fcntl.h>
#include <linux/loop.h>
#include <poll.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mount.h>
#include <sys/reboot.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
#include <sys/types.h>
#include <sys/utsname.h>
#include <unistd.h>

#include "module-loader.h"

#define LIVE_ROOT_PATH "/run/mattos/medium/live/rootfs.squashfs"
#define SYSTEMD_PATH "/usr/lib/systemd/systemd"
#define RESCUE_INIT_PATH "/usr/libexec/mattos/rescue-init"
#define LIVE_GUI_TARGET "mattos-live-graphical.target"
#define LIVE_CLI_TARGET "mattos.target"
#define INSTALL_GUI_TARGET "mattos-install-graphical.target"
#define INSTALL_CLI_TARGET "mattos-install-cli.target"

static void message(const char *format, ...)
{
    char text[2048];
    va_list arguments;
    va_start(arguments, format);
    vsnprintf(text, sizeof(text), format, arguments);
    va_end(arguments);
    /* /dev/console can be the last serial console on the kernel command
     * line. printk reaches both registered consoles and retains the report
     * in dmesg, including on physical machines without a serial cable. */
    int log = open("/dev/kmsg", O_WRONLY | O_CLOEXEC | O_NONBLOCK);
    if (log >= 0) {
        dprintf(log, "<6>mattos-live-init: %s\n", text);
        close(log);
    } else {
        dprintf(STDERR_FILENO, "mattos-live-init: %s\n", text);
    }
}

static void fatal(const char *operation)
{
    message("%s failed: %s", operation, strerror(errno));
    message("Emergency console: d = diagnostics, r = reboot. Capture this screen.");
    int screen = open("/dev/tty0", O_RDWR | O_CLOEXEC | O_NONBLOCK);
    struct pollfd inputs[] = {{STDIN_FILENO, POLLIN, 0}, {screen, POLLIN, 0}};
    for (;;) {
        if (poll(inputs, 2, -1) <= 0) continue;
        char input[32];
        int ready = inputs[1].revents & POLLIN ? screen : STDIN_FILENO;
        ssize_t count = read(ready, input, sizeof(input));
        if (count <= 0) { sleep(1); continue; }
        if (input[0] == 'r') { sync(); reboot(RB_AUTOBOOT); }
        const char *paths[] = {"/proc/cmdline", "/proc/partitions", "/proc/modules", "/proc/mounts", NULL};
        for (size_t i = 0; paths[i]; ++i) {
            message("%s", paths[i]);
            int fd = open(paths[i], O_RDONLY | O_CLOEXEC);
            if (fd < 0) continue;
            char buffer[4096];
            ssize_t n;
            while ((n = read(fd, buffer, sizeof(buffer))) > 0) {
                (void)write(STDERR_FILENO, buffer, (size_t)n);
                if (screen >= 0) (void)write(screen, buffer, (size_t)n);
            }
            close(fd);
        }
        message("Check raw ISO checksum and try another USB port; r reboots, d repeats diagnostics.");
    }
}

static void make_directory(const char *path, mode_t mode)
{
    if (mkdir(path, mode) < 0 && errno != EEXIST)
        fatal(path);
}

static void mount_required(const char *source, const char *target,
                           const char *filesystem, unsigned long flags,
                           const char *options)
{
    if (mount(source, target, filesystem, flags, options) < 0)
        fatal(target);
}

static void ensure_devtmpfs(void)
{
    if (mount("devtmpfs", "/dev", "devtmpfs", MS_NOSUID, "mode=0755") < 0 &&
        errno != EBUSY)
        fatal("/dev");
}

static bool file_exists(const char *path)
{
    struct stat status;
    return stat(path, &status) == 0;
}

static bool command_line_has_token(const char *needle)
{
    char command_line[4096];
    int descriptor = open("/proc/cmdline", O_RDONLY | O_CLOEXEC);
    if (descriptor < 0)
        return false;
    ssize_t length = read(descriptor, command_line, sizeof(command_line) - 1);
    close(descriptor);
    if (length < 0)
        return false;
    command_line[length] = '\0';
    size_t needle_length = strlen(needle);
    const char *candidate = command_line;
    while ((candidate = strstr(candidate, needle)) != NULL) {
        bool starts_token = candidate == command_line || candidate[-1] == ' ';
        char terminator = candidate[needle_length];
        bool ends_token = terminator == '\0' || terminator == ' ' ||
                          terminator == '\n' || terminator == '\r' ||
                          terminator == '\t';
        if (starts_token && ends_token)
            return true;
        candidate += needle_length;
    }
    return false;
}

static bool mattos_iso_descriptor(const unsigned char *data, ssize_t bytes)
{
    return bytes == 2048 && data[0] == 1 && data[6] == 1 &&
        memcmp(data + 1, "CD001", 5) == 0 &&
        memcmp(data + 40, "MATTOS_LIVE                     ", 32) == 0;
}

static void mount_live_medium(void)
{
    /* Inspect the primary ISO9660 descriptor before mounting. No udev or
     * device-name assumptions: sysfs lists both whole disks and partitions. */
    for (unsigned int attempt = 0; attempt < 120; ++attempt) {
        DIR *devices = opendir("/sys/class/block");
        struct dirent *entry;
        while (devices && (entry = readdir(devices)) != NULL) {
            if (entry->d_name[0] == '.')
                continue;
            char device[512];
            snprintf(device, sizeof(device), "/dev/%s", entry->d_name);
            int fd = open(device, O_RDONLY | O_CLOEXEC | O_NONBLOCK);
            if (fd < 0)
                continue;
            unsigned char descriptor[2048];
            ssize_t bytes = pread(fd, descriptor, sizeof(descriptor), 16 * 2048);
            close(fd);
            if (!mattos_iso_descriptor(descriptor, bytes))
                continue;
            if (mount(device, "/run/mattos/medium", "iso9660",
                      MS_RDONLY | MS_NODEV | MS_NOSUID, NULL) == 0) {
                if (file_exists(LIVE_ROOT_PATH)) {
                    message("mounted live medium from %s (label MATTOS_LIVE)", device);
                    FILE *report = fopen("/run/mattos/live-medium", "we");
                    if (report) { fprintf(report, "%s\n", device); fclose(report); }
                    closedir(devices);
                    return;
                }
                umount2("/run/mattos/medium", MNT_DETACH);
                message("rejected %s: missing /live/rootfs.squashfs", device);
            } else {
                message("cannot mount labelled medium %s: %s", device, strerror(errno));
            }
        }
        if (devices) closedir(devices);
        if (attempt % 10 == 0)
            message("waiting for MATTOS_LIVE storage (%u/60 seconds); see /sys/class/block", attempt / 2);
        usleep(500000);
    }
    message("No usable MATTOS_LIVE ISO found. Check USB write/port, storage driver and kernel logs; reboot to retry.");
    errno = ENOENT;
    fatal("locate MattOS live medium");
}

static int attach_live_root_loop(char *loop_path, size_t loop_path_size)
{
    int backing = open(LIVE_ROOT_PATH, O_RDONLY | O_CLOEXEC);
    if (backing < 0)
        fatal("open live root image");

    int control = open("/dev/loop-control", O_RDWR | O_CLOEXEC);
    if (control < 0)
        fatal("open loop-control");
    int number = ioctl(control, LOOP_CTL_GET_FREE);
    close(control);
    if (number < 0)
        fatal("allocate loop device");

    if (snprintf(loop_path, loop_path_size, "/dev/loop%d", number) >=
        (int)loop_path_size) {
        errno = ENAMETOOLONG;
        fatal("format loop device path");
    }
    int loop = open(loop_path, O_RDWR | O_CLOEXEC);
    if (loop < 0)
        fatal("open loop device");
    if (ioctl(loop, LOOP_SET_FD, backing) < 0)
        fatal("attach live root loop device");

    struct loop_info64 info;
    memset(&info, 0, sizeof(info));
    info.lo_flags = LO_FLAGS_READ_ONLY | LO_FLAGS_AUTOCLEAR;
    snprintf((char *)info.lo_file_name, LO_NAME_SIZE, "%s", LIVE_ROOT_PATH);
    if (ioctl(loop, LOOP_SET_STATUS64, &info) < 0)
        fatal("configure live root loop device");
    close(backing);
    return loop;
}

static void move_kernel_mount(const char *old_path, const char *new_root)
{
    char destination[256];
    if (snprintf(destination, sizeof(destination), "%s%s", new_root, old_path) >=
        (int)sizeof(destination)) {
        errno = ENAMETOOLONG;
        fatal("format moved mount path");
    }
    if (mount(old_path, destination, NULL, MS_MOVE, NULL) < 0)
        fatal(destination);
}

static void switch_to_live_root(const char *new_root, const char *real_init,
                                const char *systemd_target)
{
    if (mount(NULL, "/", NULL, MS_REC | MS_PRIVATE, NULL) < 0)
        fatal("make early mount tree private");

    move_kernel_mount("/dev", new_root);
    move_kernel_mount("/proc", new_root);
    move_kernel_mount("/sys", new_root);
    move_kernel_mount("/run", new_root);

    if (chdir(new_root) < 0)
        fatal("chdir new root");
    if (mount(new_root, "/", NULL, MS_MOVE, NULL) < 0)
        fatal("move live root to /");
    if (chroot(".") < 0 || chdir("/") < 0)
        fatal("chroot live root");

    int console = open("/dev/console", O_RDWR | O_CLOEXEC);
    if (console >= 0) {
        dup2(console, STDIN_FILENO);
        dup2(console, STDOUT_FILENO);
        dup2(console, STDERR_FILENO);
        if (console > STDERR_FILENO)
            close(console);
    }

    message("switch_root complete; executing %s", real_init);
    if (systemd_target != NULL) {
        char unit_argument[128];
        if (snprintf(unit_argument, sizeof(unit_argument), "--unit=%s",
                     systemd_target) >= (int)sizeof(unit_argument)) {
            errno = ENAMETOOLONG;
            fatal("format systemd target");
        }
        char *const arguments[] = {(char *)real_init, unit_argument, NULL};
        execv(real_init, arguments);
    } else {
        char *const arguments[] = {(char *)real_init, NULL};
        execv(real_init, arguments);
    }
    fatal("execute real init");
}

int main(void)
{
    message("starting minimal live-media early userspace");
    struct utsname kernel;
    if (uname(&kernel) == 0) message("kernel %s %s", kernel.release, kernel.machine);

    make_directory("/dev", 0755);
    make_directory("/proc", 0555);
    make_directory("/sys", 0555);
    make_directory("/run", 0755);
    /* CONFIG_DEVTMPFS_MOUNT may have mounted /dev before rdinit executes. */
    ensure_devtmpfs();
    mount_required("proc", "/proc", "proc", MS_NOSUID | MS_NODEV | MS_NOEXEC, NULL);
    mount_required("sysfs", "/sys", "sysfs", MS_NOSUID | MS_NODEV | MS_NOEXEC, NULL);
    if (mattos_load_boot_modules() < 0)
        fatal("load boot-critical kernel modules");
    mount_required("tmpfs", "/run", "tmpfs", MS_NOSUID | MS_NODEV, "mode=0755,size=90%");

    make_directory("/run/mattos", 0755);
    make_directory("/run/mattos/medium", 0755);
    make_directory("/run/mattos/lower", 0755);
    make_directory("/run/mattos/writable", 0755);
    make_directory("/newroot", 0755);
    mount_live_medium();

    char loop_path[64];
    int loop_descriptor = attach_live_root_loop(loop_path, sizeof(loop_path));
    mount_required(loop_path, "/run/mattos/lower", "squashfs",
                   MS_RDONLY | MS_NODEV | MS_NOSUID, NULL);
    /* AUTOCLEAR is safe only after the mount itself holds the loop device. */
    close(loop_descriptor);
    mount_required("tmpfs", "/run/mattos/writable", "tmpfs",
                   MS_NOSUID | MS_NODEV, "mode=0755,size=75%");
    make_directory("/run/mattos/writable/upper", 0755);
    make_directory("/run/mattos/writable/work", 0755);
    mount_required("overlay", "/newroot", "overlay", 0,
                   "lowerdir=/run/mattos/lower,upperdir=/run/mattos/writable/upper,workdir=/run/mattos/writable/work");

    /* Package payloads do not own runtime API filesystem mountpoints. */
    make_directory("/newroot/dev", 0755);
    make_directory("/newroot/proc", 0555);
    make_directory("/newroot/sys", 0555);
    make_directory("/newroot/run", 0755);

    if (!file_exists("/newroot" SYSTEMD_PATH)) {
        errno = ENOENT;
        fatal("validate live root systemd");
    }
    bool rescue_mode = command_line_has_token("mattos.rescue=1");
    const char *real_init = rescue_mode ? RESCUE_INIT_PATH : SYSTEMD_PATH;
    const char *systemd_target = NULL;
    if (!rescue_mode) {
        if (command_line_has_token("mattos.mode=install-gui"))
            systemd_target = INSTALL_GUI_TARGET;
        else if (command_line_has_token("mattos.mode=install-cli"))
            systemd_target = INSTALL_CLI_TARGET;
        else if (command_line_has_token("mattos.mode=live"))
            systemd_target = LIVE_GUI_TARGET;
        else
            systemd_target = LIVE_CLI_TARGET;
        message("boot mode selects systemd target %s", systemd_target);
    }
    message("live root mounted read-only with writable tmpfs overlay");
    switch_to_live_root("/newroot", real_init, systemd_target);
    return 1;
}
