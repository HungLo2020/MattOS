#ifndef MISC_XATTRAT_COMPAT_H
#define MISC_XATTRAT_COMPAT_H

#include <limits.h>
#include "include/visibility-hidden.h"

/*
 * The maximum number of decimal digits of an N-bit number is
 * ceil(N * log10(2)), with log10(2) being approx. 0.30103.  We
 * use 31/100 as a safe and close enough approximation; the
 * overshoot even for 128-bit numbers is only one byte.
 */
#define TYPE_BITS(type) (sizeof(type) * CHAR_BIT)
#define DECIMAL_DIGITS(type) ((TYPE_BITS(type) * 31 + 99) / 100)

struct proc_self_fd_buffer {
	char buffer[sizeof("/proc/self/fd/") +
		    /* sign */ 1 + DECIMAL_DIGITS(int)];
};

hidden char *proc_self_fd_path(int fd, struct proc_self_fd_buffer *buffer);
hidden void proc_self_fd_free(char *path, struct proc_self_fd_buffer *buffer);

hidden extern int slow_empty_paths;

hidden char *proc_self_fd_realpath(int dirfd, const char *path, int *at_flags,
				   struct proc_self_fd_buffer *buffer);
hidden int openat_with_flags(int dirfd, const char *path, int at_flags);
hidden int need_slow_empty_paths(int dirfd, const char *path);

#endif /* MISC_XATTRAT_COMPAT_H */
