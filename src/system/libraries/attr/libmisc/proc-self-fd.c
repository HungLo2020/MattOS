/*
  File: xattrat_compat.c

  Copyright (C) 2026 Andreas Gruenbacher <andreas.gruenbacher@gmail.com>

  This library is free software; you can redistribute it and/or
  modify it under the terms of the GNU Lesser General Public
  License as published by the Free Software Foundation; either
  version 2.1 of the License, or (at your option) any later version.

  This library is distributed in the hope that it will be useful,
  but WITHOUT ANY WARRANTY; without even the implied warranty of
  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
  Lesser General Public License for more details.

  You should have received a copy of the GNU Lesser General Public
  License along with this library; if not, see <https://www.gnu.org/licenses/>.
*/

#include "config.h"
#include <unistd.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include "libmisc/proc-self-fd.h"

char *
proc_self_fd_path(int fd, struct proc_self_fd_buffer *buffer)
{
	sprintf(buffer->buffer, "/proc/self/fd/%d", fd);
	return buffer->buffer;
}

void
proc_self_fd_free(char *path, struct proc_self_fd_buffer *buffer)
{
	if (path != buffer->buffer)
		free(path);
}

char *
proc_self_fd_realpath(int dirfd, const char *path, int *at_flags,
		      struct proc_self_fd_buffer *buffer)
{
	char *newpath;
	int ret;

	if (!*path) {
		*at_flags &= ~AT_SYMLINK_NOFOLLOW;
		return proc_self_fd_path(dirfd, buffer);
	}
	ret = asprintf(&newpath, "/proc/self/fd/%d/%s", dirfd, path);
	if (ret == -1)
		return NULL;
	return newpath;
}

int
openat_with_flags(int dirfd, const char *path, int at_flags)
{
	int flags = O_PATH | O_CLOEXEC;

	if (at_flags & AT_SYMLINK_NOFOLLOW)
		flags |= O_NOFOLLOW;
	return openat(dirfd, path, flags);
}

/*
 * getxattrat(), setxattrat(), listxattrat(), and removexattrat() up to at
 * least kernel version 7.2 fail with EBADF when the dirfd is an O_PATH file
 * descriptor and the pathname is NULL or empty.
 */
int
need_slow_empty_paths(int dirfd, const char *path)
{
	int saved_errno = errno;
	int open_flags;

	if (errno != EBADF)
		return 0;
	if (*path)
	       return 0;
	open_flags = fcntl(dirfd, F_GETFL);
	if (open_flags == -1) {
		errno = saved_errno;
		return 0;
	}
	if (!(open_flags & O_PATH))
		return 0;
	return 1;
}
