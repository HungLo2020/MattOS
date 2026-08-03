/*
  File: xattrat.c

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
#include <linux/xattr.h>
#include <sys/syscall.h>
#include <unistd.h>
#include <errno.h>

#include "xattrat.h"

#pragma GCC diagnostic ignored "-Wunused-parameter"

ssize_t
getxattrat(int dirfd, const char *path, int at_flags, const char *name,
	   void *value, size_t size)
{
#ifdef __NR_getxattrat
	struct xattr_args uargs = {
		.value = (unsigned long)value,
		.size = size,
	};

	return syscall(__NR_getxattrat, dirfd, path, at_flags, name,
		       &uargs, sizeof(uargs));
#else
	errno = ENOSYS;
	return -1;
#endif
}

int
setxattrat(int dirfd, const char *path, int at_flags, const char *name,
	   const void *value, size_t size, int flags)
{
#ifdef __NR_setxattrat
	struct xattr_args uargs = {
		.value = (unsigned long)value,
		.size = size,
		.flags = flags,
	};

        return syscall(__NR_setxattrat, dirfd, path, at_flags, name,
                       &uargs, sizeof(uargs));
#else
	errno = ENOSYS;
	return -1;
#endif
}

ssize_t listxattrat(int dirfd, const char *path, int at_flags, char *list,
		    size_t size)
{
#ifdef __NR_listxattrat
        return syscall(__NR_listxattrat, dirfd, path, at_flags, list, size);
#else
	errno = ENOSYS;
	return -1;
#endif
}

int
removexattrat(int dirfd, const char *path, int at_flags, const char *name)
{
#ifdef __NR_removexattrat
	return syscall(__NR_removexattrat, dirfd, path, at_flags, name);
#else
	errno = ENOSYS;
	return -1;
#endif
}
