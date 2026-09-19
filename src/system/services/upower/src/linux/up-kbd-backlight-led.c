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

#include "config.h"

#include <glib.h>
#include <glib/gi18n.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <string.h>
#include <dirent.h>
#include <errno.h>

#include "up-kbd-backlight-led.h"
#include "up-native.h"
#include "up-types.h"

static void     up_kbd_backlight_led_finalize   (GObject	*object);

struct UpKbdBacklightLedPrivate
{
	gint			 max_brightness;
	gint			 brightness;

	gint			 fd_hw_changed;
	GIOChannel		*channel_hw_changed;
};

G_DEFINE_TYPE_WITH_PRIVATE (UpKbdBacklightLed, up_kbd_backlight_led, UP_TYPE_DEVICE_KBD_BACKLIGHT)


/**
 * up_kbd_backlight_led_brightness_write:
 *
 * Write the brightness value to the LED device with a given path.
 **/
static gboolean
up_kbd_backlight_led_brightness_write (UpDeviceKbdBacklight *kbd_backlight, const gchar *native_path, gint value)
{
	UpKbdBacklightLed *kbd;
	UpKbdBacklightLedPrivate *priv;
	g_autoptr (GString) value_str = g_string_new (NULL);

	g_return_val_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (kbd_backlight), FALSE);

	kbd = UP_KBD_BACKLIGHT_LED (kbd_backlight);
	priv = up_kbd_backlight_led_get_instance_private (kbd);

	g_string_printf (value_str, "%d", CLAMP (value, 0, priv->max_brightness));
	if (!g_file_set_contents_full (native_path, value_str->str, value_str->len,
				       G_FILE_SET_CONTENTS_ONLY_EXISTING, 0644, NULL)) {
		g_debug ("Failed on setting keyboard backlight LED brightness: %s", native_path);
		return FALSE;
	}

	return TRUE;
}

/**
 * up_kbd_backlight_led_brightness_read:
 *
 * Read the brightness value from the LED device with a given path.
 **/
static gint
up_kbd_backlight_led_brightness_read (UpDeviceKbdBacklight *kbd_backlight, const gchar *native_path)
{
	g_autofree gchar *buf = NULL;
	gint64 brightness = -1;

	g_return_val_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (kbd_backlight), brightness);

	if (!g_file_get_contents (native_path, &buf, NULL, NULL))
		return -1;

	g_strstrip (buf);
	g_debug ("brightness: %s", buf);

	brightness = g_ascii_strtoll (buf, NULL, 10);
	if (brightness < 0) {
		g_warning ("failed to convert brightness.");
		return -1;
	}

	return brightness;
}

/**
 * up_kbd_backlight_led_set_brightness:
 *
 * Set the brightness.
 **/
static gboolean
up_kbd_backlight_led_set_brightness (UpDeviceKbdBacklight *kbd_backlight, gint value)
{
	GObject *native;
	g_autofree gchar *filename = NULL;
	const gchar *native_path;
	gboolean ret = FALSE;

	native = up_device_kbd_backlight_get_native (UP_DEVICE_KBD_BACKLIGHT (kbd_backlight));
	g_return_val_if_fail (native != NULL, FALSE);

	native_path = up_native_get_native_path (native);
	g_return_val_if_fail (native_path != NULL, FALSE);

	filename = g_build_filename (native_path, "brightness", NULL);

	ret = up_kbd_backlight_led_brightness_write (kbd_backlight, filename, value);

	return ret;
}

/**
 * up_kbd_backlight_led_get_brightness:
 *
 * Get the brightness.
 **/
static gint
up_kbd_backlight_led_get_brightness (UpDeviceKbdBacklight *kbd_backlight)
{
	GObject *native;
	const gchar *native_path;
	g_autofree gchar *filename = NULL;
	gint brightness = -1;

	native = up_device_kbd_backlight_get_native (UP_DEVICE_KBD_BACKLIGHT (kbd_backlight));
	g_return_val_if_fail (native != NULL, brightness);

	native_path = up_native_get_native_path (native);
	g_return_val_if_fail (native_path != NULL, brightness);

	filename = g_build_filename (native_path, "brightness", NULL);
	brightness = up_kbd_backlight_led_brightness_read (kbd_backlight, filename);

	return brightness;
}

/**
 * up_kbd_backlight_led_get_max_brightness:
 *
 * Gets the max brightness.
 **/
static gint
up_kbd_backlight_led_get_max_brightness (UpDeviceKbdBacklight *kbd_backlight)
{
	UpKbdBacklightLed *kbd = UP_KBD_BACKLIGHT_LED (kbd_backlight);
	UpKbdBacklightLedPrivate *priv = up_kbd_backlight_led_get_instance_private (kbd);
	return priv->max_brightness;
}

/**
 * up_kbd_backlight_led_event_io:
 *
 * This function is called when the brightness of the LED device changes.
 **/
static gboolean
up_kbd_backlight_led_event_io (GIOChannel *channel, GIOCondition condition, gpointer data)
{
	UpDeviceKbdBacklight *kbd_backlight = UP_DEVICE_KBD_BACKLIGHT (data);
	UpKbdBacklightLed *kbd = UP_KBD_BACKLIGHT_LED (kbd_backlight);
	UpKbdBacklightLedPrivate *priv = up_kbd_backlight_led_get_instance_private (kbd);
	gint brightness = -1;
	gchar buf[16];
	glong len;
	gchar *end = NULL;

	if (priv->fd_hw_changed < 0)
		return FALSE;

	if (!(condition & G_IO_PRI))
		return FALSE;

	lseek (priv->fd_hw_changed, 0, SEEK_SET);
	len = read (priv->fd_hw_changed, buf, G_N_ELEMENTS (buf) - 1);

	if (len > 0) {
		buf[len] = '\0';
		g_strstrip (buf);
		brightness = g_ascii_strtoll (buf, &end, 10);

		if (brightness < 0 ||
		    brightness > priv->max_brightness ||
		    end == buf) {
			brightness = -1;
			g_warning ("kbd eventio: failed to convert brightness.");
		}
	} else {
		return FALSE;
	}
	g_debug ("kbd eventio: brightness: %d", brightness);

	if (brightness < 0)
		return FALSE;

	up_device_kbd_backlight_emit_change (kbd_backlight, brightness, "internal");
	up_device_kbd_backlight_report (kbd_backlight, priv->max_brightness, brightness, TRUE);

	return TRUE;
}

/**
 * up_kbd_backlight_led_coldplug:
 *
 * Update the LED settings to UpKbdBacklightLed device.
 **/
static gboolean
up_kbd_backlight_led_coldplug (UpDeviceKbdBacklight *kbd_backlight)
{
	UpKbdBacklightLed *kbd;
	UpKbdBacklightLedPrivate *priv;
	GObject *native;
	g_autofree gchar *filename = NULL;
	g_autofree gchar *filename_brightness = NULL;
	g_autofree gchar *path_hw_changed = NULL;
	const gchar *native_path = NULL;

	g_return_val_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (kbd_backlight), FALSE);

	kbd = UP_KBD_BACKLIGHT_LED (kbd_backlight);
	priv = up_kbd_backlight_led_get_instance_private (kbd);

	native = up_device_kbd_backlight_get_native (kbd_backlight);
	if (native == NULL) {
		priv->max_brightness = 0;
		return FALSE;
	}

	native_path = up_native_get_native_path (native);
	filename = g_build_filename (native_path, "max_brightness", NULL);

	priv->max_brightness = up_kbd_backlight_led_brightness_read (kbd_backlight, filename);

	filename_brightness = g_build_filename (native_path, "brightness", NULL);
	priv->brightness = up_kbd_backlight_led_brightness_read (kbd_backlight, filename_brightness);

	/* Set up device watcher */
	path_hw_changed = g_build_filename (native_path, "brightness_hw_changed", NULL);
	priv->fd_hw_changed = open (path_hw_changed, O_RDONLY);
	if (priv->fd_hw_changed >= 0) {
		priv->channel_hw_changed = g_io_channel_unix_new (priv->fd_hw_changed);
		g_io_add_watch (priv->channel_hw_changed,
				G_IO_PRI, up_kbd_backlight_led_event_io, kbd_backlight);
	}

	up_device_kbd_backlight_report (kbd_backlight, priv->max_brightness, priv->brightness, FALSE);

	return TRUE;
}


/**
 * up_kbd_backlight_led_finalize:
 **/
static void
up_kbd_backlight_led_finalize (GObject *object)
{
	UpKbdBacklightLed *kbd_backlight;
	UpKbdBacklightLedPrivate *priv;

	g_return_if_fail (object != NULL);
	g_return_if_fail (UP_IS_KBD_BACKLIGHT_LED (object));

	kbd_backlight = UP_KBD_BACKLIGHT_LED (object);
	priv = up_kbd_backlight_led_get_instance_private (kbd_backlight);

	if (priv->channel_hw_changed) {
		g_io_channel_shutdown (priv->channel_hw_changed, FALSE, NULL);
		g_io_channel_unref (priv->channel_hw_changed);
	}

	if (priv->fd_hw_changed >= 0)
		close (priv->fd_hw_changed);

	G_OBJECT_CLASS (up_kbd_backlight_led_parent_class)->finalize (object);
}

/**
 * up_kbd_backlight_led_class_init:
 **/
static void
up_kbd_backlight_led_class_init (UpKbdBacklightLedClass *klass)
{
	GObjectClass *object_class = G_OBJECT_CLASS (klass);
	UpDeviceKbdBacklightClass *dev_kbd_klass = UP_DEVICE_KBD_BACKLIGHT_CLASS (klass);

	object_class->finalize = up_kbd_backlight_led_finalize;

	dev_kbd_klass->coldplug = up_kbd_backlight_led_coldplug;
	dev_kbd_klass->get_max_brightness = up_kbd_backlight_led_get_max_brightness;
	dev_kbd_klass->get_brightness = up_kbd_backlight_led_get_brightness;
	dev_kbd_klass->set_brightness = up_kbd_backlight_led_set_brightness;
}

/**
 * up_kbd_backlight_led_init:
 **/
static void
up_kbd_backlight_led_init (UpKbdBacklightLed *kbd_backlight)
{
	kbd_backlight->priv = up_kbd_backlight_led_get_instance_private (kbd_backlight);
}
