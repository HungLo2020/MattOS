/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2025 Kate Hsuan <p.hsuan@gmail.com>
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

#ifndef __UP_KBD_BACKLIGHT_LED_H
#define __UP_KBD_BACKLIGHT_LED_H

#include <glib-object.h>
#include "up-device-kbd-backlight.h"

G_BEGIN_DECLS

#define UP_TYPE_KBD_BACKLIGHT_LED		(up_kbd_backlight_led_get_type ())
#define UP_KBD_BACKLIGHT_LED(o)			(G_TYPE_CHECK_INSTANCE_CAST ((o), UP_TYPE_KBD_BACKLIGHT_LED, UpKbdBacklightLed))
#define UP_KBD_BACKLIGHT_LED_CLASS(k)		(G_TYPE_CHECK_CLASS_CAST((k), UP_TYPE_KBD_BACKLIGHT_LED, UpKbdBacklightLedClass))
#define UP_IS_KBD_BACKLIGHT_LED(o)		(G_TYPE_CHECK_INSTANCE_TYPE ((o), UP_TYPE_KBD_BACKLIGHT_LED))
#define UP_IS_KBD_BACKLIGHT_LED_CLASS(k)	(G_TYPE_CHECK_CLASS_TYPE ((k), UP_TYPE_KBD_BACKLIGHT_LED))
#define UP_KBD_BACKLIGHT_LED_GET_CLASS(o)	(G_TYPE_INSTANCE_GET_CLASS ((o), UP_TYPE_KBD_BACKLIGHT_LED, UpKbdBacklightLedClass))

typedef struct UpKbdBacklightLedPrivate UpKbdBacklightLedPrivate;

typedef struct
{
	UpDeviceKbdBacklight		 parent;
	UpKbdBacklightLedPrivate	*priv;
} UpKbdBacklightLed;

typedef struct
{
	UpDeviceKbdBacklightClass		 parent_class;
} UpKbdBacklightLedClass;

GType		 	 up_kbd_backlight_led_get_type			(void);

G_END_DECLS

#endif	/* __UP_KBD_BACKLIGHT_LED_H */
