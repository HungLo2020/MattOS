/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2009 Richard Hughes <richard@hughsie.com>
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

#include "up-common.h"
#include <glib.h>

char*
up_make_safe_string (char *text)
{
	guint i;
	guint idx = 0;

	/* no point checking */
	if (text == NULL)
		return NULL;

	if (g_utf8_validate (text, -1, NULL))
		return text;

	/* shunt up only safe chars */
	for (i=0; text[i] != '\0'; i++) {
		if (g_ascii_isprint (text[i])) {
			/* only copy if the address is going to change */
			if (idx != i)
				text[idx] = text[i];
			idx++;
		} else {
			g_debug ("invalid char: 0x%02X", text[i]);
		}
	}

	/* ensure null terminated */
	text[idx] = '\0';

	return text;
}

UpDeviceTechnology
up_convert_device_technology (const gchar *type)
{
	if (type == NULL)
		return UP_DEVICE_TECHNOLOGY_UNKNOWN;
	/* every case combination of Li-Ion is commonly used.. */
	if (g_ascii_strcasecmp (type, "li-ion") == 0 ||
	    g_ascii_strcasecmp (type, "lion") == 0)
		return UP_DEVICE_TECHNOLOGY_LITHIUM_ION;
	if (g_ascii_strcasecmp (type, "pb") == 0 ||
	    g_ascii_strcasecmp (type, "pbac") == 0)
		return UP_DEVICE_TECHNOLOGY_LEAD_ACID;
	if (g_ascii_strcasecmp (type, "lip") == 0 ||
	    g_ascii_strcasecmp (type, "lipo") == 0 ||
	    g_ascii_strcasecmp (type, "li-poly") == 0)
		return UP_DEVICE_TECHNOLOGY_LITHIUM_POLYMER;
	if (g_ascii_strcasecmp (type, "nimh") == 0)
		return UP_DEVICE_TECHNOLOGY_NICKEL_METAL_HYDRIDE;
	if (g_ascii_strcasecmp (type, "life") == 0)
		return UP_DEVICE_TECHNOLOGY_LITHIUM_IRON_PHOSPHATE;
	return UP_DEVICE_TECHNOLOGY_UNKNOWN;
}
