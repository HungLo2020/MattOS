/*
 * Copyright (C) 2004-2008 Red Hat, Inc. All rights reserved.
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

#ifndef LIBDM_KDEV_H
#define LIBDM_KDEV_H

#define MAJOR(dev)      (((unsigned)(dev) & 0xfff00u) >> 8)
#define MINOR(dev)      (((unsigned)(dev) & 0xffu) | (((unsigned)(dev) >> 12) & 0xfff00u))
#define MKDEV(ma,mi)    (((dev_t)(mi) & 0xffu) | (((dev_t)(ma) & 0xfffu) << 8) | (((dev_t)(mi) & ~(dev_t)0xffu) << 12))

#endif /* LIBDM_KDEV_H */
