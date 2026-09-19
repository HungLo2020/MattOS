/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2025 Kate Hsuan <p.hsuan@gmail.com>
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

#ifndef __UP_DEVICE_KBD_BACKLIGHT_H__
#define __UP_DEVICE_KBD_BACKLIGHT_H__

#include <dbus/up-kbd-backlight-generated.h>
#include "up-daemon.h"

G_BEGIN_DECLS

#define UP_TYPE_DEVICE_KBD_BACKLIGHT		(up_device_kbd_backlight_get_type ())

G_DECLARE_DERIVABLE_TYPE (UpDeviceKbdBacklight, up_device_kbd_backlight, UP, DEVICE_KBD_BACKLIGHT, UpExportedKbdBacklightSkeleton)

struct _UpDeviceKbdBacklightClass
{
	UpExportedKbdBacklightSkeletonClass parent_class;

	gboolean	 (*coldplug)		(UpDeviceKbdBacklight	*device);

	gint		 (*get_max_brightness)	(UpDeviceKbdBacklight	*device);
	gint		 (*get_brightness)	(UpDeviceKbdBacklight	*device);
	gboolean	 (*set_brightness)	(UpDeviceKbdBacklight	*device, gint brightness);

};


GType			 up_device_kbd_backlight_get_type		(void);

void			 up_device_kbd_backlight_emit_change		(UpDeviceKbdBacklight *kbd_backlight,
									 int value,
									 const char *source);
const gchar		*up_device_kbd_backlight_get_object_path	(UpDeviceKbdBacklight	*device);
GObject			*up_device_kbd_backlight_get_native		(UpDeviceKbdBacklight	*device);
UpDeviceKbdBacklight	*up_device_kbd_backlight_new			(UpDaemon		*daemon,
									 GObject		*native,
									 UpDeviceList		*device_list);
void			 up_device_kbd_backlight_report			(UpDeviceKbdBacklight *self,
									 gint max_brightness,
									 gint brightness,
									 gboolean composite_signal);
gboolean		 up_device_kbd_backlight_register		(UpDeviceKbdBacklight	*device);
void			 up_device_kbd_backlight_unregister		(UpDeviceKbdBacklight	*device);

G_END_DECLS

#endif /* __UP_DEVICE_KBD_BACKLIGHT_H__ */
