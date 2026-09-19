/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2015 Eric Koegel <eric.koegel@gmail.com>
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

#ifndef __UP_BACKEND_BSD_PRIVATE_H
#define __UP_BACKEND_BSD_PRIVATE_H

#include <gio/gio.h>
#include "up-backend.h"
#include "up-config.h"

#define CONSOLEKIT2_DBUS_NAME                  "org.freedesktop.ConsoleKit"
#define CONSOLEKIT2_DBUS_PATH                  "/org/freedesktop/ConsoleKit/Manager"
#define CONSOLEKIT2_DBUS_INTERFACE             "org.freedesktop.ConsoleKit.Manager"

GDBusProxy *up_backend_get_seat_manager_proxy (UpBackend  *backend);

UpConfig   *up_backend_get_config             (UpBackend  *backend);


#endif /* __UP_BACKEND_BSD_PRIVATE_H */
