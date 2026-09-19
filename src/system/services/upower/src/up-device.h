/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2008 David Zeuthen <davidz@redhat.com>
 * Copyright (C) 2008 Richard Hughes <richard@hughsie.com>
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

#ifndef __UP_DEVICE_H__
#define __UP_DEVICE_H__

#include <dbus/up-device-generated.h>
#include "up-daemon.h"

G_BEGIN_DECLS

#define UP_TYPE_DEVICE		(up_device_get_type ())

G_DECLARE_DERIVABLE_TYPE (UpDevice, up_device, UP, DEVICE, UpExportedDeviceSkeleton)

typedef enum {
	UP_REFRESH_INIT,
	UP_REFRESH_POLL,
	UP_REFRESH_RESUME,
	UP_REFRESH_EVENT,
	UP_REFRESH_LINE_POWER,
} UpRefreshReason;

struct _UpDeviceClass
{
	UpExportedDeviceSkeletonClass parent_class;

	/* vtable */
	gboolean	 (*coldplug)		(UpDevice	*device);
	void		 (*sibling_discovered)	(UpDevice	*device,
						 GObject	*sibling);
	gboolean	 (*refresh)		(UpDevice	*device,
						 UpRefreshReason reason);
	const gchar	*(*get_id)		(UpDevice	*device);
	gboolean	 (*get_on_battery)	(UpDevice	*device,
						 gboolean	*on_battery);
	gboolean	 (*get_online)		(UpDevice	*device,
						 gboolean	*online);
};

GType		 up_device_get_type		(void);
UpDevice	*up_device_new			(UpDaemon	*daemon,
						 GObject	*native);

UpDaemon	*up_device_get_daemon		(UpDevice	*device);
GObject		*up_device_get_native		(UpDevice	*device);
const gchar	*up_device_get_object_path	(UpDevice	*device);
gboolean	 up_device_get_on_battery	(UpDevice	*device,
						 gboolean	*on_battery);
gboolean	 up_device_get_online		(UpDevice	*device,
						 gboolean	*online);
const gchar	*up_device_get_state_dir_override (UpDevice *device);
gboolean	 up_device_polkit_is_allowed	(UpDevice	*device,
						 GDBusMethodInvocation *invocation);
void		 up_device_sibling_discovered	(UpDevice	*device,
						 GObject	*sibling);
gboolean	 up_device_refresh_internal	(UpDevice	*device,
						 UpRefreshReason reason);
void		 up_device_unregister		(UpDevice	*device);
gboolean	 up_device_register		(UpDevice	*device);
gboolean	 up_device_is_registered	(UpDevice	*device);

G_END_DECLS

#endif /* __UP_DEVICE_H__ */
