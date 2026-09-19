/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2017 Bastien Nocera <hadess@hadess.net>
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

#include "config.h"

#include <gio/gio.h>

#include "up-types.h"
#include "up-device-bluez.h"

G_DEFINE_TYPE (UpDeviceBluez, up_device_bluez, UP_TYPE_DEVICE)

static UpDeviceKind
appearance_to_kind (guint16 appearance)
{
        switch ((appearance & 0xffc0) >> 6) {
        case 0x01:
                return UP_DEVICE_KIND_PHONE;
        case 0x02:
                return UP_DEVICE_KIND_COMPUTER;
        case 0x05:
                return UP_DEVICE_KIND_MONITOR;
        case 0x0a:
                return UP_DEVICE_KIND_MEDIA_PLAYER;
        case 0x0f: /* HID Generic */
                switch (appearance & 0x3f) {
                case 0x01:
                        return UP_DEVICE_KIND_KEYBOARD;
                case 0x02:
                        return UP_DEVICE_KIND_MOUSE;
                case 0x03:
                case 0x04:
                        return UP_DEVICE_KIND_GAMING_INPUT;
                case 0x05:
                        return UP_DEVICE_KIND_TABLET;
                case 0x0e:
                case 0x0f:
                        return UP_DEVICE_KIND_PEN;
                }
                break;
        }

	return UP_DEVICE_KIND_BLUETOOTH_GENERIC;
}

/**
 * class_to_kind:
 * @class: a Bluetooth device class
 *
 * Returns value: the type of device corresponding to the given @class value.
 **/
static UpDeviceKind
class_to_kind (guint32 class)
{
	/*
	 * See Bluetooth Assigned Numbers for Baseband
	 * https://www.bluetooth.com/specifications/assigned-numbers/baseband/
	 */

	switch ((class & 0x1f00) >> 8) {
	case 0x01:
		return UP_DEVICE_KIND_COMPUTER;
	case 0x02:
		switch ((class & 0xfc) >> 2) {
		case 0x01:
		case 0x02:
		case 0x03:
		case 0x05:
			return UP_DEVICE_KIND_PHONE;
		case 0x04:
			return UP_DEVICE_KIND_MODEM;
		}
		break;
	case 0x03:
		return UP_DEVICE_KIND_NETWORK;
	case 0x04:
		switch ((class & 0xfc) >> 2) {
		case 0x01:
		case 0x02:
			return UP_DEVICE_KIND_HEADSET;
		case 0x05:
			return UP_DEVICE_KIND_SPEAKERS;
		case 0x06:
			return UP_DEVICE_KIND_HEADPHONES;
		case 0x0b: /* VCR */
		case 0x0c: /* Video Camera */
		case 0x0d: /* Camcorder */
			return UP_DEVICE_KIND_VIDEO;
		default:
			return UP_DEVICE_KIND_OTHER_AUDIO;
		}
		break;
	case 0x05:
		switch ((class & 0xc0) >> 6) {
		case 0x00:
			switch ((class & 0x1e) >> 2) {
			case 0x01:
			case 0x02:
				return UP_DEVICE_KIND_GAMING_INPUT;
			case 0x03:
				return UP_DEVICE_KIND_REMOTE_CONTROL;
			}
			break;
		case 0x01:
			return UP_DEVICE_KIND_KEYBOARD;
		case 0x02:
			switch ((class & 0x1e) >> 2) {
			case 0x05:
				return UP_DEVICE_KIND_TABLET;
			default:
				return UP_DEVICE_KIND_MOUSE;
			}
		}
		break;
	case 0x06:
		if (class & 0x80)
			return UP_DEVICE_KIND_PRINTER;
		if (class & 0x40)
			return UP_DEVICE_KIND_SCANNER;
		if (class & 0x20)
			return UP_DEVICE_KIND_CAMERA;
		if (class & 0x10)
			return UP_DEVICE_KIND_MONITOR;
		break;
	case 0x07:
		return UP_DEVICE_KIND_WEARABLE;
	case 0x08:
		return UP_DEVICE_KIND_TOY;
	}

	return UP_DEVICE_KIND_BLUETOOTH_GENERIC;
}

/**
 * up_device_bluez_coldplug:
 *
 * Return %TRUE on success, %FALSE if we failed to get data and should be removed
 **/
static gboolean
up_device_bluez_coldplug (UpDevice *device)
{
	GDBusObjectProxy *object_proxy;
	GDBusProxy *proxy;
	GError *error = NULL;
	UpDeviceKind kind;
	const char *uuid;
	const char *model;
	g_autoptr (GVariant) v_appearance = NULL;
	g_autoptr (GVariant) v_class = NULL;
	g_autoptr (GVariant) v_address = NULL;
	g_autoptr (GVariant) v_alias = NULL;
	g_autoptr (GVariant) v_percentage = NULL;
	guchar percentage;

	/* Static device properties */
	object_proxy = G_DBUS_OBJECT_PROXY (up_device_get_native (device));
	proxy = g_dbus_proxy_new_sync (g_dbus_object_proxy_get_connection (object_proxy),
				       G_DBUS_PROXY_FLAGS_NONE,
				       NULL,
				       "org.bluez",
				       g_dbus_object_get_object_path (G_DBUS_OBJECT (object_proxy)),
				       "org.bluez.Device1",
				       NULL,
				       &error);

	if (!proxy) {
		g_warning ("Failed to get proxy for %s (iface org.bluez.Device1)",
			   g_dbus_object_get_object_path (G_DBUS_OBJECT (object_proxy)));
		return FALSE;
	}

	v_appearance = g_dbus_proxy_get_cached_property (proxy, "Appearance");
	v_class = g_dbus_proxy_get_cached_property (proxy, "Class");
	if (v_appearance && g_variant_get_uint16 (v_appearance) != 0) {
		guint16 appearance;

		appearance = g_variant_get_uint16 (v_appearance);
		kind = appearance_to_kind (appearance);
	} else if (v_class) {
		guint32 class;

		class = g_variant_get_uint32 (v_class);
		kind = class_to_kind (class);
	} else {
		kind = UP_DEVICE_KIND_BLUETOOTH_GENERIC;
	}

	v_address = g_dbus_proxy_get_cached_property (proxy, "Address");
	uuid = g_variant_get_string (v_address, NULL);

	v_alias = g_dbus_proxy_get_cached_property (proxy, "Alias");
	model = g_variant_get_string (v_alias, NULL);

	/* hardcode some values */
	g_object_set (device,
		      "type", kind,
		      "serial", uuid,
		      "model", model,
		      "power-supply", FALSE,
		      "has-history", TRUE,
		      NULL);

	g_object_unref (proxy);

	/* Initial battery values */
	proxy = g_dbus_proxy_new_sync (g_dbus_object_proxy_get_connection (object_proxy),
				       G_DBUS_PROXY_FLAGS_NONE,
				       NULL,
				       "org.bluez",
				       g_dbus_object_get_object_path (G_DBUS_OBJECT (object_proxy)),
				       "org.bluez.Battery1",
				       NULL,
				       &error);

	if (!proxy) {
		g_warning ("Failed to get proxy for %s",
			   g_dbus_object_get_object_path (G_DBUS_OBJECT (object_proxy)));
		return FALSE;
	}

	v_percentage = g_dbus_proxy_get_cached_property (proxy, "Percentage");
	percentage = g_variant_get_byte (v_percentage);

	g_object_set (device,
		      "is-present", TRUE,
		      "percentage", (gdouble) percentage,
		      "update-time", (guint64) g_get_real_time () / G_USEC_PER_SEC,
		      NULL);

	g_object_unref (proxy);

	return TRUE;
}

static void
up_device_bluez_init (UpDeviceBluez *bluez)
{
}

void
up_device_bluez_update (UpDeviceBluez *bluez,
			GVariant      *properties)
{
	UpDevice *device = UP_DEVICE (bluez);
	GVariantIter iter;
	const gchar *key;
	GVariant *value;

	g_variant_iter_init (&iter, properties);
	while (g_variant_iter_next (&iter, "{&sv}", &key, &value)) {
		if (g_str_equal (key, "Percentage")) {
			g_object_set (device,
				      "percentage", (gdouble) g_variant_get_byte (value),
				      "update-time", (guint64) g_get_real_time () / G_USEC_PER_SEC,
				      NULL);
		} else if (g_str_equal (key, "Alias")) {
			g_object_set (device,
				      "model", g_variant_get_string (value, NULL),
				      NULL);
		} else {
			char *str = g_variant_print (value, TRUE);

			g_debug ("Unhandled key: %s value: %s", key, str);
			g_free (str);
		}
		g_variant_unref (value);
	}
}

static void
up_device_bluez_class_init (UpDeviceBluezClass *klass)
{
	UpDeviceClass *device_class = UP_DEVICE_CLASS (klass);

	device_class->coldplug = up_device_bluez_coldplug;
}
