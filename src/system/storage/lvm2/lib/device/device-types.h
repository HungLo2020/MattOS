/*
 * Copyright (C) 2001-2004 Sistina Software, Inc. All rights reserved.
 * Copyright (C) 2004-2013 Red Hat, Inc. All rights reserved.
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

#ifndef LIB_DEVICE_DEVICE_TYPES_H
#define LIB_DEVICE_DEVICE_TYPES_H

#include <stdint.h>

#define DEV_KNOWN_NAME_LEN 15

typedef struct {
	const char name[DEV_KNOWN_NAME_LEN];
	const uint8_t max_partitions;
	const char * const desc;
} dev_known_type_t;

extern const dev_known_type_t dev_known_types[];

#endif
