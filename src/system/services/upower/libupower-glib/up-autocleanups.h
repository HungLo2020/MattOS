/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2015 Kalev Lember <klember@redhat.com>
 *
 * Licensed under the GNU General Public License Version 2
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
 */

#if !defined (__UPOWER_H_INSIDE__) && !defined (UP_COMPILATION)
#error "Only <upower.h> can be included directly."
#endif

#ifndef __UP_AUTOCLEANUPS_H
#define __UP_AUTOCLEANUPS_H

#ifdef G_DEFINE_AUTOPTR_CLEANUP_FUNC

G_DEFINE_AUTOPTR_CLEANUP_FUNC(UpClient, g_object_unref)
G_DEFINE_AUTOPTR_CLEANUP_FUNC(UpDevice, g_object_unref)
G_DEFINE_AUTOPTR_CLEANUP_FUNC(UpHistoryItem, g_object_unref)
G_DEFINE_AUTOPTR_CLEANUP_FUNC(UpStatsItem, g_object_unref)

#endif

#endif /* __UP_AUTOCLEANUPS_H */
