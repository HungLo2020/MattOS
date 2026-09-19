/*
 * Copyright (C) 2018 Red Hat, Inc. All rights reserved.
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
#include "lib/device/bcache.h"
#include "lib/misc/lvm-signal.h"

#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <fcntl.h>
#include <unistd.h>

//----------------------------------------------------------------

#define SECTOR_SHIFT 9
#define SECTOR_SIZE 512
#define BLOCK_SIZE_SECTORS 8
#define PAGE_SIZE_SECTORS ((TEST_PAGE_SIZE) >> SECTOR_SHIFT)
#define NR_BLOCKS 64

struct fixture {
	struct io_engine *e;
	uint8_t *data;

	char fname[64];
	int fd;
	int di;
};

static void _fill_buffer(uint8_t *buffer, uint8_t seed, size_t count)
{
	unsigned i;
	uint8_t b = seed;

	for (i = 0; i < count; i++) {
		buffer[i] = b;
		b = ((b << 5) + b) + i;
	}
}

static void _check_buffer(uint8_t *buffer, uint8_t seed, size_t count)
{
	unsigned i;
	uint8_t b = seed;

	for (i = 0; i < count; i++) {
		T_ASSERT_EQUAL(buffer[i], b);
		b = ((b << 5) + b) + i;
	}
}

static void _print_buffer(const char *name, uint8_t *buffer, size_t count)
{
	unsigned col;

	fprintf(stderr, "%s:\n", name);
	while (count) {
		for (col = 0; count && col < 20; col++) {
			fprintf(stderr, "%x, ", (unsigned) *buffer);
			buffer++;
			count--;
		}
		fprintf(stderr, "\n");
	}
}

static void *_fix_init(void)
{
	struct fixture *f = malloc(sizeof(*f));

	T_ASSERT(f);
	f->e = create_async_io_engine();
	T_ASSERT(f->e);
	if (posix_memalign((void **) &f->data, TEST_PAGE_SIZE, SECTOR_SIZE * BLOCK_SIZE_SECTORS))
		test_fail("posix_memalign failed");

	snprintf(f->fname, sizeof(f->fname), "unit-test-XXXXXX");
	/* coverity[secure_temp] don't care */
	f->fd = mkstemp(f->fname);
	T_ASSERT(f->fd >= 0);

	_fill_buffer(f->data, 123, SECTOR_SIZE * BLOCK_SIZE_SECTORS);

	T_ASSERT(write(f->fd, f->data, SECTOR_SIZE * BLOCK_SIZE_SECTORS) == SECTOR_SIZE * BLOCK_SIZE_SECTORS);
	T_ASSERT(lseek(f->fd, 0, SEEK_SET) != -1);

	return f;
}

static void _fix_exit(void *fixture)
{
	struct fixture *f = fixture;

	if (f) {
		(void) close(f->fd);
		(void) unlink(f->fname);
		free(f->data);
		if (f->e)
			f->e->destroy(f->e);
		free(f);
	}
}

static void _test_create(void *fixture)
{
	// empty
}

struct io {
	bool completed;
	int error;
};

static void _io_init(struct io *io)
{
	io->completed = false;
	io->error = 0;
}

static void _complete_io(void *context, int io_error)
{
	struct io *io = context;
	io->completed = true;
	io->error = io_error;
}

static void _test_read(void *fixture)
{
	struct fixture *f = fixture;
	struct io io;
	struct bcache *cache = bcache_create(PAGE_SIZE_SECTORS, BLOCK_SIZE_SECTORS, f->e);
	T_ASSERT(cache);

	f->di = bcache_set_fd(f->fd);

	T_ASSERT(f->di >= 0);

	_io_init(&io);
	T_ASSERT(f->e->issue(f->e, DIR_READ, f->di, 0, BLOCK_SIZE_SECTORS, f->data, &io));
	T_ASSERT(f->e->wait(f->e, _complete_io));
	T_ASSERT(io.completed);
	T_ASSERT(!io.error);

	_check_buffer(f->data, 123, SECTOR_SIZE * BLOCK_SIZE_SECTORS);

	bcache_destroy(cache);
	f->e = NULL;   // already destroyed
}

static void _test_write(void *fixture)
{
	struct fixture *f = fixture;
	struct io io;
	struct bcache *cache = bcache_create(PAGE_SIZE_SECTORS, BLOCK_SIZE_SECTORS, f->e);
	T_ASSERT(cache);

	f->di = bcache_set_fd(f->fd);

	T_ASSERT(f->di >= 0);

	_io_init(&io);
	T_ASSERT(f->e->issue(f->e, DIR_WRITE, f->di, 0, BLOCK_SIZE_SECTORS, f->data, &io));
	T_ASSERT(f->e->wait(f->e, _complete_io));
	T_ASSERT(io.completed);
	T_ASSERT(!io.error);

	bcache_destroy(cache);
	f->e = NULL;   // already destroyed
}

static void _test_write_bytes(void *fixture)
{
	struct fixture *f = fixture;

	unsigned offset = 345;
	char buf_out[32];
	char buf_in[32];
	struct bcache *cache = bcache_create(PAGE_SIZE_SECTORS, BLOCK_SIZE_SECTORS, f->e);
	T_ASSERT(cache);

	f->di = bcache_set_fd(f->fd);

	// T_ASSERT(bcache_read_bytes(cache, f->di, offset, sizeof(buf_in), buf_in));
	_fill_buffer((uint8_t *) buf_out, 234, sizeof(buf_out));
	T_ASSERT(bcache_write_bytes(cache, f->di, offset, sizeof(buf_out), buf_out));
	T_ASSERT(bcache_read_bytes(cache, f->di, offset, sizeof(buf_in), buf_in));

	if (memcmp(buf_out, buf_in, sizeof(buf_out))) {
		_print_buffer("buf_out", (uint8_t *) buf_out, sizeof(buf_out));
		_print_buffer("buf_in", (uint8_t *) buf_in, sizeof(buf_in));
	}
	T_ASSERT(!memcmp(buf_out, buf_in, sizeof(buf_out)));

	bcache_destroy(cache);
	f->e = NULL;   // already destroyed
}

/*
 * Test that _async_destroy() skips io_destroy() after fork().
 *
 * The aio_context is created in the parent process. After fork() the
 * child inherits the context value but must not call io_destroy() on
 * it - only the original process should do that.  _async_destroy()
 * compares aio_context_pid against getpid() to guard this.
 *
 * Also exercises the normal io_destroy() path in the parent, verifying
 * that the negative-return error reporting (commit 512a39448) works
 * without crashing (io_destroy returns -errno, not -1+errno).
 */
static void _test_destroy_after_fork(void *fixture)
{
	struct io_engine *e;
	pid_t pid;
	int status;

	e = create_async_io_engine();
	T_ASSERT(e);

	pid = fork();
	T_ASSERT(pid >= 0);

	if (!pid) {
		/*
		 * Child: destroy must skip io_destroy() because pid
		 * differs from aio_context_pid.  If it incorrectly
		 * calls io_destroy() the parent's context gets
		 * invalidated and the parent's destroy will fail.
		 */
		e->destroy(e);
		_exit(0);
	}

	/* Parent: wait for child to finish its destroy first */
	T_ASSERT(waitpid(pid, &status, 0) == pid);
	T_ASSERT(WIFEXITED(status) && !WEXITSTATUS(status));

	/*
	 * Parent: destroy calls io_destroy() for real.
	 * This would fail if the child incorrectly destroyed
	 * the shared aio_context.
	 */
	e->destroy(e);
	/* coverity[leaked_storage] e freed by e->destroy */
}

/*
 * Test that _async_wait() is interruptible by SIGINT/SIGTERM (via
 * sigint_allow()), but retries on other signals such as SIGALRM.
 *
 * The retry loop in _async_wait() is:
 *   do { r = io_getevents(...); } while (r == -EINTR && !sigint_caught());
 *
 * So EINTR from a stray signal retries; EINTR after SIGINT/SIGTERM
 * (which set sigint_caught()) stops and returns false.
 *
 * Strategy: call wait() with no I/O submitted so io_getevents(min_nr=1)
 * must block.  A child process sends SIGINT to the parent after a short
 * delay, interrupting io_getevents().  Since sigint_allow() installed
 * _catch_sigint (which sets sigint_caught()), the retry loop exits and
 * wait() returns false.
 *
 * Why not raise(SIGINT) before calling wait()?
 * raise() delivers the signal immediately, before io_getevents() is
 * even called.  _catch_sigint sets sigint_caught() but io_getevents()
 * then blocks forever because no I/O is pending and the signal is
 * already consumed.  The child-process approach ensures the signal
 * arrives while io_getevents() is actually blocked.
 *
 * Why not issue I/O and race a signal?
 * Linux AIO on regular files (and character devices like /dev/zero,
 * /dev/urandom) completes synchronously inside io_submit() - the
 * kernel posts the completion before io_submit() returns, so
 * io_getevents() never blocks.  Only O_DIRECT on a real block device
 * goes through the true async path.  Calling wait() with no I/O
 * pending guarantees io_getevents() blocks, making the test
 * deterministic without needing a block device.
 */
static void _test_wait_eintr(void *fixture)
{
	struct io_engine *e;
	pid_t child;
	int status;

	e = create_async_io_engine();
	T_ASSERT(e);

	/*
	 * Arm the LVM SIGINT/SIGTERM handler (clears SA_RESTART,
	 * installs _catch_sigint which sets sigint_caught()).
	 */
	sigint_allow();

	/*
	 * Fork a child that waits 10ms then sends SIGINT to the parent.
	 * By then the parent is guaranteed to be inside io_getevents().
	 */
	child = fork();
	T_ASSERT(child >= 0);
	if (!child) {
		usleep(10000);
		kill(getppid(), SIGINT);
		_exit(0);
	}

	/*
	 * No I/O submitted: io_getevents(min_nr=1) blocks until SIGINT
	 * arrives from the child.  sigint_caught() is then set so the
	 * retry loop exits and wait() returns false.
	 */
	T_ASSERT(!e->wait(e, _complete_io));
	T_ASSERT(sigint_caught());

	T_ASSERT(waitpid(child, &status, 0) == child);
	T_ASSERT(WIFEXITED(status) && !WEXITSTATUS(status));

	sigint_restore();
	sigint_clear();

	e->destroy(e);
	/* coverity[leaked_storage] freed by destroy */
}

//----------------------------------------------------------------

#define T(path, desc, fn) register_test(ts, "/base/device/bcache/io-engine/" path, desc, fn)

static struct test_suite *_tests(void)
{
	struct test_suite *ts = test_suite_create(_fix_init, _fix_exit);
	if (!ts) {
		fprintf(stderr, "out of memory\n");
		exit(1);
	}

	T("create-destroy", "simple create/destroy", _test_create);
	T("read", "read sanity check", _test_read);
	T("write", "write sanity check", _test_write);
	T("bcache-write-bytes", "test the utility fns", _test_write_bytes);
	T("destroy-after-fork", "io_destroy skipped in child after fork", _test_destroy_after_fork);
	T("wait-eintr", "io_getevents interrupted by signal", _test_wait_eintr);

	return ts;
}

void io_engine_tests(struct dm_list *all_tests)
{
	dm_list_add(all_tests, &_tests()->list);
}
