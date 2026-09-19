/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2022 Benjamin Berg <bberg@redhat.com>
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
 * Foundation, Inc., 51 Franklin St, Fifth Floor, Boston, MA  02110-1301  USA
 *
 */

#pragma once

#include <glib-object.h>

G_BEGIN_DECLS

#define UP_TYPE_ENUMERATOR	(up_enumerator_get_type ())

G_DECLARE_DERIVABLE_TYPE (UpEnumerator, up_enumerator, UP, ENUMERATOR, GObject)

struct _UpEnumeratorClass
{
  GObjectClass parent_class;

  void (*initable_init) (UpEnumerator *self);
};

UpDaemon *up_enumerator_get_daemon (UpEnumerator *self);

G_END_DECLS
