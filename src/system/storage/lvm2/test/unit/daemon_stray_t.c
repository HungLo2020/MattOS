/*
 * Copyright (C) 2026 Red Hat, Inc. All rights reserved.
 *
 * This file is part of LVM2.
 *
 * This copyrighted material is made available to anyone wishing to use,
 * modify, copy, or redistribute it subject to the terms and conditions
 * of the GNU General Public License v.2.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software Foundation,
 * Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 */

#include "units.h"

/* Provide prerequisites for daemon-stray.h */
#include "lib/misc/lvm-file.h"
#include "libdaemon/server/daemon-stray.h"

#include <errno.h>
#include <fcntl.h>
#include <unistd.h>

/*
 * Test that _daemon_close_descriptor() preserves file descriptors
 * with FD_CLOEXEC (intentionally opened by well-behaved libraries
 * like PKCS#11 modules) and closes those without FD_CLOEXEC (stray).
 */

static void test_close_preserves_cloexec(void *fixture)
{
#ifdef O_CLOEXEC
	int fd;

	/* Open fd with CLOEXEC - simulates well-behaved library */
	fd = open("/dev/null", O_RDONLY | O_CLOEXEC);
	T_ASSERT(fd >= 0);

	_daemon_close_descriptor(fd, 1, "test", getpid(), "");

	/* fd must still be valid - not closed */
	T_ASSERT(is_valid_fd(fd));

	(void) close(fd);
#endif
}

static void test_close_removes_non_cloexec(void *fixture)
{
	int fd;

	/* Open fd without CLOEXEC - stray/leaked descriptor */
	fd = open("/dev/null", O_RDONLY);
	T_ASSERT(fd >= 0);
	T_ASSERT(fcntl(fd, F_SETFD, 0) == 0);

	_daemon_close_descriptor(fd, 1, "test", getpid(), "");

	/* fd must have been closed */
	T_ASSERT(!is_valid_fd(fd));
}

static void test_close_ignores_bad_fd(void *fixture)
{
	/* Must not crash on invalid fd */
	_daemon_close_descriptor(9999, 1, "test", getpid(), "");
}

static void test_close_stray_fds(void *fixture)
{
	int fd_keep, fd_close;
	struct custom_fds cfds = { .out = -1, .err = -1, .report = -1 };

#ifdef O_CLOEXEC
	/* fd with CLOEXEC - well-behaved library, should survive */
	fd_keep = open("/dev/null", O_RDONLY | O_CLOEXEC);
	T_ASSERT(fd_keep >= 0);
#endif
	/* fd without CLOEXEC - stray, should be closed */
	fd_close = open("/dev/null", O_RDONLY);
	T_ASSERT(fd_close >= 0);
	T_ASSERT(fcntl(fd_close, F_SETFD, 0) == 0);

	daemon_close_stray_fds("test", 1, STDERR_FILENO, &cfds);

	T_ASSERT(!is_valid_fd(fd_close));

#ifdef O_CLOEXEC
	T_ASSERT(is_valid_fd(fd_keep));

	(void) close(fd_keep);
#endif
	/* coverity[leaked_handle] fd_close closed by daemon_close_stray_fds */
}

#define T(path, desc, fn) register_test(ts, "/daemon/stray-fds/" path, desc, fn)

void daemon_stray_tests(struct dm_list *all_tests)
{
	struct test_suite *ts = test_suite_create(NULL, NULL);
	if (!ts) {
		fprintf(stderr, "out of memory\n");
		exit(1);
	}

	T("preserve-cloexec", "CLOEXEC fd preserved", test_close_preserves_cloexec);
	T("close-non-cloexec", "non-CLOEXEC fd closed", test_close_removes_non_cloexec);
	T("ignore-bad-fd", "bad fd ignored", test_close_ignores_bad_fd);
	T("full-close-stray", "daemon_close_stray_fds respects CLOEXEC", test_close_stray_fds);

	dm_list_add(all_tests, &ts->list);
}
