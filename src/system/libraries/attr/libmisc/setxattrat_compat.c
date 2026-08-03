/*
  File: getxattrat_compat.c

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
#include <sys/xattr.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <stdlib.h>
#include "xattrat.h"
#include "libmisc/proc-self-fd.h"

#define DECLARATIONS_ONLY
#include "xattrat_compat.h"

/* Defined in getxattrat_compat.c */
extern int slow_empty_paths;

static int
xsetxattr(const char *path, int at_flags, const char *name, const void *value,
	  size_t size, int flags)
{
	if (at_flags & AT_SYMLINK_NOFOLLOW)
		return lsetxattr(path, name, value, size, flags);
	return setxattr(path, name, value, size, flags);
}

int
setxattrat_compat(int dirfd, const char *path, int at_flags, const char *name,
		  const void *value, size_t size, int flags)
{
	struct proc_self_fd_buffer buffer;
	static int no_setxattrat;
	char *newpath;
	int fd, ret;

	if (at_flags & ~(AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW)) {
		errno = EINVAL;
		return -1;
	}
	if (!*path) {
		if (!(at_flags & AT_EMPTY_PATH)) {
			errno = ENOENT;
			return -1;
		}
	}

	if (dirfd == AT_FDCWD || *path == '/')
		return xsetxattr(path, at_flags, name, value, size, flags);

	if (!no_setxattrat) {
		if (!*path && slow_empty_paths)
			goto proc_self_fd_path;
		ret = setxattrat(dirfd, path, at_flags, name, value, size,
				 flags);
		if (ret != -1)
			return ret;
		if (need_slow_empty_paths(dirfd, path)) {
			slow_empty_paths = 1;
			goto proc_self_fd_path;
		}
		if (errno != ENOSYS)
			return ret;
		no_setxattrat = 1;
	}

proc_self_fd_path:
	newpath = proc_self_fd_realpath(dirfd, path, &at_flags, &buffer);
	if (!newpath)
		return -1;
	ret = xsetxattr(newpath, at_flags, name, value, size, flags);
	proc_self_fd_free(newpath, &buffer);
	if (ret != -1)
		return ret;
	if (errno == ENOENT) {
		if (!*path) {
			errno = ENOSYS;
			return -1;
		}
		if (fcntl(dirfd, F_GETFL) != -1)
			errno = ENOENT;
		return -1;
	} else if (errno != ENAMETOOLONG)
		return ret;

	fd = openat_with_flags(dirfd, path, at_flags);
	if (fd == -1)
		return -1;
	ret = setxattr(proc_self_fd_path(fd, &buffer), name, value, size, flags);
	close(fd);
	return ret;
}
