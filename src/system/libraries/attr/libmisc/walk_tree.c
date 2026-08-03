/*
  File: walk_tree.c

  Copyright (C) 2007-2026 Andreas Gruenbacher <a.gruenbacher@computer.org>

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
#include <sys/types.h>
#include <sys/stat.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/time.h>
#include <sys/resource.h>
#include <dirent.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

#include "walk_tree.h"

struct dir_stack_entry {
	struct dir_stack_entry *parent;
	DIR *stream;
	long pos;
	dev_t dev;
	ino_t ino;
};

struct dir_hash_entry {
	struct dir_hash_entry *next;
	dev_t dev;
	ino_t ino;
};

#define DIR_HASH_SIZE 128

struct walk_tree_args {
	enum walk_flags walk_flags;
	int (*func)(int, const char *, const char *, unsigned char,
		    enum walk_flags, void *);
	void *arg;
	int depth;
	unsigned int num_dir_handles;
	dev_t dev;
	struct dir_hash_entry **dir_hash;
};

static int dir_visited(struct dir_stack_entry *entry, dev_t dev, ino_t ino)
{
	while (entry) {
		if (entry->dev == dev && entry->ino == ino)
			  return 1;
		entry = entry->parent;
	}
	return 0;
}

static unsigned int dir_hash(dev_t dev, ino_t ino)
{
	return (dev ^ ino) % DIR_HASH_SIZE;
}

static int dir_hash_insert_unique(struct walk_tree_args *args, struct stat *st)
{
	unsigned int bucket = dir_hash(st->st_dev, st->st_ino);
	struct dir_hash_entry *entry;

	if (!args->dir_hash) {
		/* allocate the directory hash table */
		args->dir_hash = calloc(DIR_HASH_SIZE, sizeof(*args->dir_hash));
		if (!args->dir_hash)
			return -1;
	}

	entry = args->dir_hash[bucket];
	while (entry) {
		if (entry->dev == st->st_dev && entry->ino == st->st_ino)
			return 1;
		entry = entry->next;
	}

	entry = malloc(sizeof(*entry));
	if (!entry)
		return -1;

	entry->dev = st->st_dev;
	entry->ino = st->st_ino;
	entry->next = args->dir_hash[bucket];
	args->dir_hash[bucket] = entry;
	return 0;
}

static char *new_dirname(const char *dirname, const char *pathname)
{
	size_t dirlen, pathlen;
	char *newname, *n;
	int need_slash;

	if (*dirname == '\0' && *pathname == '.' &&
	    (*(pathname+1) == '/' || *(pathname+1) == '\0')) {
		pathname++;
		while (*pathname == '/')
			pathname++;
	}

	dirlen = strlen(dirname);
	pathlen = strlen(pathname);
	need_slash = (pathlen > 0 && pathname[pathlen - 1] != '/');
	n = newname = malloc(dirlen + pathlen + need_slash + 1);
	if (newname == NULL)
		return NULL;
	memcpy(n, dirname, dirlen);
	n += dirlen;
	memcpy(n, pathname, pathlen);
	n += pathlen;
	if (need_slash)
		*n++ = '/';
	*n++ = '\0';
	return newname;
}

static int walk_tree_rec(int dirfd, const char *dirname, const char *pathname,
			 unsigned char dirtype, struct walk_tree_args *args,
			 struct dir_stack_entry *parent_dir)
{
	enum walk_flags walk_flags = args->walk_flags;
	struct dir_stack_entry this_dir = {
		.parent = parent_dir,
	};
	int follow;
	struct stat st;
	char *mydirname = NULL;
	int dir_open_flags;
	int fd = -1;
	struct dirent *entry;
	int err = 0;

	if (args->depth == 0)
		walk_flags |= WALK_TREE_TOPLEVEL;

	follow = (walk_flags & WALK_TREE_LOGICAL) ||
		 (!(walk_flags & WALK_TREE_PHYSICAL) && !args->depth);

	if (!(walk_flags & WALK_TREE_RECURSIVE) ||
	    (dirtype != DT_UNKNOWN &&
	     dirtype != DT_DIR &&
	     (dirtype != DT_LNK ||
	      ((walk_flags & WALK_TREE_PHYSICAL) &&
	       !(walk_flags & WALK_TREE_TOPLEVEL)))))
		goto visit_this;

	if (args->num_dir_handles == 0) {
		struct dir_stack_entry *top;

close_another_parent:
		/* Close the topmost parent directory handle still open. */
		top = parent_dir;
		if (!top || !top->stream) {
			errno = ENFILE;
			goto fail;
		}
		while (top->parent && top->parent->stream)
			top = top->parent;
		top->pos = telldir(top->stream);
		closedir(top->stream);
		top->stream = NULL;
		args->num_dir_handles++;
	}

	dir_open_flags = O_RDONLY | O_DIRECTORY | (follow ? 0 : O_NOFOLLOW);
	fd = openat(dirfd, pathname, dir_open_flags);
	if (fd == -1) {
		if (errno == ENOTDIR || errno == ENOENT || errno == ELOOP)
			goto visit_this;
		if (errno != ENFILE)
			goto fail;
		/* Ran out of file descriptors. */
		args->num_dir_handles = 0;
		goto close_another_parent;
	}
	args->num_dir_handles--;
	if (dirtype == DT_UNKNOWN && !follow)
		dirtype = DT_DIR;

	if (fstat(fd, &st) == -1)
		goto fail;
	if (walk_flags & WALK_TREE_ONE_FILESYSTEM) {
		if (args->dev == 0)
			args->dev = st.st_dev;
		else if (st.st_dev != args->dev)
			goto dir_visited;
	}

	if (walk_flags & WALK_TREE_LOGICAL) {
		if (dir_visited(parent_dir, st.st_dev, st.st_ino))
			goto dir_visited;
		this_dir.dev = st.st_dev;
		this_dir.ino = st.st_ino;
	} else {
		int ret = dir_hash_insert_unique(args, &st);
		if (ret == 1)
			goto dir_visited;
		if (ret == -1)
			goto fail;
	}
	goto visit_this;

dir_visited:
	close(fd);
	fd = -1;
	if (dirtype == DT_UNKNOWN) {
		if (fstatat(dirfd, pathname, &st, AT_SYMLINK_NOFOLLOW) == -1)
			goto fail;
		dirtype = IFTODT(st.st_mode);
	}
	if (dirtype == DT_DIR)
		goto out;

visit_this:
	err += args->func(dirfd, dirname, pathname, dirtype,
			  walk_flags, args->arg);

	if (fd == -1)
		goto out;
	this_dir.stream = fdopendir(fd);
	if (this_dir.stream == NULL)
		goto fail;

	mydirname = new_dirname(dirname, pathname);
	if (mydirname == NULL)
		goto fail;

	args->depth++;
	while ((entry = readdir(this_dir.stream)) != NULL) {
		if (!strcmp(entry->d_name, ".") ||
		    !strcmp(entry->d_name, ".."))
			continue;

		err += walk_tree_rec(fd, mydirname, entry->d_name,
				     entry->d_type, args, &this_dir);

		if (!this_dir.stream) {
			/* Reopen the directory handle. */
			fd = openat(dirfd, pathname, dir_open_flags);
			if (fd == -1)
				goto fail_depth;
			args->num_dir_handles--;
			this_dir.stream = fdopendir(fd);
			if (!this_dir.stream)
				goto fail_depth;
			seekdir(this_dir.stream, this_dir.pos);
		}
	}
	args->depth--;

	if (closedir(this_dir.stream) != 0) {
		this_dir.stream = NULL;
		goto fail;
	}
	this_dir.stream = NULL;

out:
	free(mydirname);
	if (this_dir.stream) {
		closedir(this_dir.stream);
		args->num_dir_handles++;
		fd = -1;
	}
	if (fd != -1) {
		close(fd);
		args->num_dir_handles++;
	}
	return err;

fail:
	err += args->func(dirfd, dirname, pathname, dirtype,
			  walk_flags | WALK_TREE_FAILED, args->arg);
	goto out;

fail_depth:
	args->depth--;
	goto fail;
}

/*
 * If (walk_flags & WALK_TREE_PHYSICAL), do not traverse symlinks.
 * If (walk_flags & WALK_TREE_LOGICAL), traverse all symlinks.
 * Otherwise, traverse only top-level symlinks.
 */
int walk_tree(const char *pathname, enum walk_flags walk_flags,
	      int (*func)(int, const char *, const char *, unsigned char,
			  enum walk_flags, void *),
	      void *arg)
{
	struct walk_tree_args args = {
		.walk_flags = walk_flags,
		.func = func,
		.arg = arg,
	};
	struct rlimit rlimit;
	int err;

	/* number of directory file descriptors to keep open at a time */
	if (getrlimit(RLIMIT_NOFILE, &rlimit) != 0)
		rlimit.rlim_cur = 0;
	args.num_dir_handles = rlimit.rlim_cur / 4 + 1;

	err = walk_tree_rec(AT_FDCWD, "", pathname, DT_UNKNOWN, &args, NULL);

	if (args.dir_hash) {
		/* destroy the directory hash table */
		int n;

		for (n = 0; n < DIR_HASH_SIZE; n++) {
			struct dir_hash_entry *entry = args.dir_hash[n];
			while (entry) {
				struct dir_hash_entry *next = entry->next;
				free(entry);
				entry = next;
			}
		}
		free(args.dir_hash);
	}

	return err;
}
