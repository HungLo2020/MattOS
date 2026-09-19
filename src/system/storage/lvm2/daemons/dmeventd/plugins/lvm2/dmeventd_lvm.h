/*
 * Copyright (C) 2010-2015 Red Hat, Inc. All rights reserved.
 *
 * This file is part of LVM2.
 *
 * This copyrighted material is made available to anyone wishing to use,
 * modify, copy, or redistribute it subject to the terms and conditions
 * of the GNU Lesser General Public License v.2.1.
 *
 * You should have received a copy of the GNU Lesser General Public License
 * along with this program; if not, write to the Free Software Foundation,
 * Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 */

/*
 * Wrappers around liblvm2cmd functions for dmeventd plug-ins.
 *
 * liblvm2cmd is not thread-safe so the locking in this library helps dmeventd
 * threads to co-operate in sharing a single instance.
 *
 * FIXME Either support this properly as a generic liblvm2cmd wrapper or make
 * liblvm2cmd thread-safe so this can go away.
 */

#ifndef DMEVENTD_LVM_H
#define DMEVENTD_LVM_H

#include <stddef.h>

struct dm_pool;

int dmeventd_lvm2_init(void);
void dmeventd_lvm2_exit(void);
int dmeventd_lvm2_run(const char *cmdline);

void dmeventd_lvm2_lock(void);
void dmeventd_lvm2_unlock(void);

struct dm_pool *dmeventd_lvm2_pool(void);

int dmeventd_lvm2_command(struct dm_pool *mem, char *buffer, size_t size,
			  const char *cmd, const char *device);

#define dmeventd_lvm2_run_with_lock(cmdline) \
	({\
		int rc;\
		dmeventd_lvm2_lock();\
		rc = dmeventd_lvm2_run(cmdline);\
		dmeventd_lvm2_unlock();\
		rc;\
	})

static inline void *_dmeventd_lvm2_init_with_pool(const char *name, size_t size)
{
	struct dm_pool *mem;

	if (!dmeventd_lvm2_init())
		return NULL;

	if ((mem = dm_pool_create(name, 2048))) {
		/* dso_state structs always have struct dm_pool *mem as first member */
		struct dm_pool **st = dm_pool_zalloc(mem, size);
		if (st) {
			*st = mem;
			return st;
		}
		dm_pool_destroy(mem);
	}

	dmeventd_lvm2_exit();
	return NULL;
}

#define dmeventd_lvm2_init_with_pool(name, st) \
	(st = _dmeventd_lvm2_init_with_pool(name, sizeof(*st)))

#define dmeventd_lvm2_exit_with_pool(st) \
	do {\
		dm_pool_destroy(st->mem);\
		dmeventd_lvm2_exit();\
	} while(0)

#endif /* DMEVENTD_LVM_H */
