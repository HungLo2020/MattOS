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

/* Used in {set,list,remove}xattrat_compat.c */
int slow_empty_paths;

static ssize_t
xgetxattr(const char *path, int at_flags, const char *name, void *value,
	  size_t size)
{
	if (at_flags & AT_SYMLINK_NOFOLLOW)
		return lgetxattr(path, name, value, size);
	return getxattr(path, name, value, size);
}

ssize_t
getxattrat_compat(int dirfd, const char *path, int at_flags, const char *name,
		  void *value, size_t size)
{
	struct proc_self_fd_buffer buffer;
	static int no_getxattrat;
	char *newpath;
	ssize_t ret;
	int fd;

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
		return xgetxattr(path, at_flags, name, value, size);

	if (!no_getxattrat) {
		if (!*path && slow_empty_paths)
			goto proc_self_fd_path;
		ret = getxattrat(dirfd, path, at_flags, name, value, size);
		if (ret != -1)
		       return ret;
		if (need_slow_empty_paths(dirfd, path)) {
			slow_empty_paths = 1;
			goto proc_self_fd_path;
		}
		if (errno != ENOSYS)
			return -1;
		no_getxattrat = 1;
	}

proc_self_fd_path:
	newpath = proc_self_fd_realpath(dirfd, path, &at_flags, &buffer);
	if (!newpath)
		return -1;
	ret = xgetxattr(newpath, at_flags, name, value, size);
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
	ret = getxattr(proc_self_fd_path(fd, &buffer), name, value, size);
	close(fd);
	return ret;
}
