/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2017 Bastien Nocera <hadess@hadess.net>
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

#ifndef __UP_CONSTANTS_H
#define __UP_CONSTANTS_H

#include <glib-object.h>

G_BEGIN_DECLS

#define UP_DAEMON_UNKNOWN_TIMEOUT			   1 /* second */
#define UP_DAEMON_UNKNOWN_POLL_TIME			   5 /* second */
#define UP_DAEMON_ESTIMATE_TIMEOUT			   5 /* second */
#define UP_DAEMON_SHORT_TIMEOUT				  30 /* seconds */
#define UP_DAEMON_LONG_TIMEOUT				 120 /* seconds */

#define UP_DAEMON_DISTRUST_RATE_TIMEOUT			  10 /* second */

#define UP_FULLY_CHARGED_THRESHOLD			  90 /* % */
#define UP_DAEMON_EPSILON				0.01 /* I can't believe it's not zero */

#define SECONDS_PER_HOUR				3600 /* seconds in an hour */
#define SECONDS_PER_HOUR_F				3600.0f

G_END_DECLS

#endif /* __UP_CONSTANTS_H */
