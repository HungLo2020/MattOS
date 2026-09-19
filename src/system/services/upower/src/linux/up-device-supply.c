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

#include "config.h"

#include <string.h>
#include <math.h>

#include <glib.h>
#include <glib/gstdio.h>
#include <glib/gprintf.h>
#include <glib/gi18n-lib.h>
#include <glib-object.h>
#include <gudev/gudev.h>

#include "up-config.h"
#include "up-types.h"
#include "up-constants.h"
#include "up-device-supply.h"
#include "up-common.h"

struct UpDeviceSupplyPrivate
{
	gboolean		 has_coldplug_values;
	gboolean		 shown_invalid_voltage_warning;
};

G_DEFINE_TYPE_WITH_PRIVATE (UpDeviceSupply, up_device_supply, UP_TYPE_DEVICE)

static gboolean		 up_device_supply_refresh	 	(UpDevice *device,
								 UpRefreshReason reason);
static UpDeviceKind	 up_device_supply_guess_type		(GUdevDevice *native,
								 const char *native_path);

static gboolean
up_device_supply_refresh_line_power (UpDeviceSupply *supply,
				     UpRefreshReason reason)
{
	UpDevice *device = UP_DEVICE (supply);
	GUdevDevice *native;
	gboolean online_old, online_new;

	/* get new AC value */
	native = G_UDEV_DEVICE (up_device_get_native (device));

	g_object_get (device,
		      "online", &online_old,
		      NULL);
	online_new = g_udev_device_get_sysfs_attr_as_int_uncached (native, "online");
	/* Avoid notification if the value did not change. */
	if (online_old != online_new)
		g_object_set (device,
			      "online", online_new,
			      NULL);

	return TRUE;
}

/**
 * up_device_supply_reset_values:
 **/
static void
up_device_supply_reset_values (UpDeviceSupply *supply)
{
	supply->priv->has_coldplug_values = FALSE;

	/* reset to default */
	g_object_set (supply,
		      "vendor", NULL,
		      "model", NULL,
		      "serial", NULL,
		      "update-time", (guint64) 0,
		      "online", FALSE,
		      "energy", (gdouble) 0.0,
		      "is-present", FALSE,
		      "is-rechargeable", FALSE,
		      "has-history", FALSE,
		      "has-statistics", FALSE,
		      "state", UP_DEVICE_STATE_UNKNOWN,
		      "capacity", (gdouble) 0.0,
		      "energy-empty", (gdouble) 0.0,
		      "energy-full", (gdouble) 0.0,
		      "energy-full-design", (gdouble) 0.0,
		      "energy-rate", (gdouble) 0.0,
		      "voltage", (gdouble) 0.0,
		      "time-to-empty", (gint64) 0,
		      "time-to-full", (gint64) 0,
		      "percentage", (gdouble) 0.0,
		      "temperature", (gdouble) 0.0,
		      "technology", UP_DEVICE_TECHNOLOGY_UNKNOWN,
		      "charge-cycles", -1,
		      "charge-start-threshold", 0,
		      "charge-end-threshold", 100,
		      "charge-threshold-enabled", FALSE,
		      "charge-threshold-supported", FALSE,
		      NULL);
}

/**
 * up_device_supply_get_online:
 **/
static gboolean
up_device_supply_get_online (UpDevice *device, gboolean *online)
{
	UpDeviceSupply *supply = UP_DEVICE_SUPPLY (device);
	UpDeviceKind type;
	gboolean online_tmp;

	g_return_val_if_fail (UP_IS_DEVICE_SUPPLY (supply), FALSE);
	g_return_val_if_fail (online != NULL, FALSE);

	g_object_get (device,
		      "type", &type,
		      "online", &online_tmp,
		      NULL);

	if (type != UP_DEVICE_KIND_LINE_POWER)
		return FALSE;

	*online = online_tmp;

	return TRUE;
}

/**
 * up_device_supply_get_string:
 **/
static gchar *
up_device_supply_get_string (GUdevDevice *native, const gchar *key)
{
	gchar *value;

	/* get value, and strip to remove spaces */
	value = g_strdup (g_udev_device_get_sysfs_attr_uncached (native, key));
	if (value)
		g_strstrip (value);

	/* no value */
	if (value == NULL)
		goto out;

	/* empty value */
	if (value[0] == '\0') {
		g_free (value);
		value = NULL;
		goto out;
	}
out:
	return value;
}

UpDeviceState
up_device_supply_get_state (GUdevDevice *native)
{
	UpDeviceState state;
	gchar *status;

	status = up_device_supply_get_string (native, "status");
	if (status == NULL ||
	    g_ascii_strcasecmp (status, "unknown") == 0 ||
	    *status == '\0') {
		state = UP_DEVICE_STATE_UNKNOWN;
	} else if (g_ascii_strcasecmp (status, "charging") == 0)
		state = UP_DEVICE_STATE_CHARGING;
	else if (g_ascii_strcasecmp (status, "discharging") == 0)
		state = UP_DEVICE_STATE_DISCHARGING;
	else if (g_ascii_strcasecmp (status, "full") == 0)
		state = UP_DEVICE_STATE_FULLY_CHARGED;
	else if (g_ascii_strcasecmp (status, "empty") == 0)
		state = UP_DEVICE_STATE_EMPTY;
	else if (g_ascii_strcasecmp (status, "not charging") == 0)
		state = UP_DEVICE_STATE_PENDING_CHARGE;
	else {
		g_warning ("unknown status string: %s", status);
		state = UP_DEVICE_STATE_UNKNOWN;
	}

	g_free (status);

	return state;
}

static gdouble
sysfs_get_capacity_level (GUdevDevice   *native,
			  UpDeviceLevel *level)
{
	char *str;
	gdouble ret = -1.0;
	guint i;
	struct {
		const char *str;
		gdouble percentage;
		UpDeviceLevel level;
	} levels[] = {
		/* In order of most likely to least likely,
		 * Keep in sync with up_daemon_compute_warning_level() */
		{ "Normal",    55.0, UP_DEVICE_LEVEL_NORMAL },
		{ "High",      70.0, UP_DEVICE_LEVEL_HIGH },
		{ "Low",       10.0, UP_DEVICE_LEVEL_LOW },
		{ "Critical",   5.0, UP_DEVICE_LEVEL_CRITICAL },
		{ "Full",     100.0, UP_DEVICE_LEVEL_FULL },
		{ "Unknown",   50.0, UP_DEVICE_LEVEL_UNKNOWN }
	};

	g_return_val_if_fail (level != NULL, -1.0);

	if (!g_udev_device_has_sysfs_attr_uncached (native, "capacity_level")) {
		g_debug ("capacity_level doesn't exist, skipping");
		*level = UP_DEVICE_LEVEL_NONE;
		return -1.0;
	}

	*level = UP_DEVICE_LEVEL_UNKNOWN;
	str = g_strchomp (g_strdup (g_udev_device_get_sysfs_attr_uncached (native, "capacity_level")));
	if (!str) {
		g_debug ("Failed to read capacity_level!");
		return ret;
	}

	for (i = 0; i < G_N_ELEMENTS(levels); i++) {
		if (strcmp (levels[i].str, str) == 0) {
			ret = levels[i].percentage;
			*level = levels[i].level;
			break;
		}
	}

	if (ret < 0.0)
		g_debug ("Could not find a percentage for capacity level '%s'", str);

	g_free (str);
	return ret;
}

static gboolean
up_device_supply_refresh_device (UpDeviceSupply *supply,
				 UpRefreshReason reason)
{
	UpDeviceState state;
	UpDevice *device = UP_DEVICE (supply);
	GUdevDevice *native;
	gdouble percentage = 0.0f;
	UpDeviceLevel level = UP_DEVICE_LEVEL_NONE;
	gboolean is_present = TRUE;

	native = G_UDEV_DEVICE (up_device_get_native (device));

	/* initial values */
	if (!supply->priv->has_coldplug_values) {
		g_autofree gchar *model_name = NULL;
		g_autofree gchar *serial_number = NULL;

		/* get values which may be blank */
		model_name = up_device_supply_get_string (native, "model_name");
		serial_number = up_device_supply_get_string (native, "serial_number");

		/* some vendors fill this with binary garbage */
		up_make_safe_string (model_name);
		up_make_safe_string (serial_number);

		g_object_set (device,
			      "model", model_name,
			      "serial", serial_number,
			      "is-rechargeable", TRUE,
			      "has-history", TRUE,
			      "has-statistics", TRUE,
			      NULL);

		/* we only coldplug once, as these values will never change */
		supply->priv->has_coldplug_values = TRUE;
	}

	/* Some devices change whether they're present or not */
	if (g_udev_device_has_sysfs_attr_uncached (native, "present"))
		is_present = g_udev_device_get_sysfs_attr_as_boolean_uncached (native, "present");

	/* get a precise percentage */
	percentage = g_udev_device_get_sysfs_attr_as_double_uncached (native, "capacity");
	if (percentage == 0.0f)
		percentage = sysfs_get_capacity_level (native, &level);

	if (percentage < 0.0) {
		/* Probably talking to the device over Bluetooth */
		state = UP_DEVICE_STATE_UNKNOWN;
		g_object_set (device,
			      "state", state,
			      "is-present", is_present,
			      NULL);
		return FALSE;
	}

	state = up_device_supply_get_state (native);

	/* Override whatever the device might have told us
	 * because a number of them are always discharging */
	if (percentage == 100.0)
		state = UP_DEVICE_STATE_FULLY_CHARGED;

	g_object_set (device,
		      "percentage", percentage,
		      "battery-level", level,
		      "state", state,
		      "is-present", is_present,
		      NULL);

	return TRUE;
}

static void
up_device_supply_sibling_discovered_guess_type (UpDevice *device,
						GObject  *sibling)
{
	GUdevDevice *input, *native_device;
	g_autoptr (GUdevDevice) parent_device = NULL;
	g_autoptr (GUdevDevice) parent_sibling = NULL;
	UpDeviceKind cur_type, new_type;
	gboolean is_same_parent = FALSE;
	g_autofree gchar *new_model_name = NULL;
	g_autofree gchar *model_name = NULL;
	g_autofree gchar *serial_number = NULL;
	int i;
	struct {
		const char *prop;
		UpDeviceKind type;
	} types[] = {
		/* In order of type priority (*within* one input node). */
		{ "SOUND_INITIALIZED", UP_DEVICE_KIND_OTHER_AUDIO },
		{ "ID_INPUT_TABLET", UP_DEVICE_KIND_TABLET },
		{ "ID_INPUT_TOUCHPAD", UP_DEVICE_KIND_TOUCHPAD },
		{ "ID_INPUT_MOUSE", UP_DEVICE_KIND_MOUSE },
		{ "ID_INPUT_JOYSTICK", UP_DEVICE_KIND_GAMING_INPUT },
		{ "ID_INPUT_KEYBOARD", UP_DEVICE_KIND_KEYBOARD },
	};
	/* The type priority if we have multiple siblings,
	 * i.e. we select the first of the current type of the found type.
	 * Give a new priority for device type since the GAMING_INPUT may include
	 * a keyboard, a touchpad, and... etc, for example Sony DualShock4 joystick.
	 * A mouse and a touchpad may include a mouse and a keyboard.
	 * Therefore, the priority is:
	 * 1. Gaming_input
	 * 2. Audio
	 * 3. Keyboard
	 * 4. Tablet
	 * 5. Touchpad
	 * 6. Mouse
	*/
	UpDeviceKind priority[] = {
		UP_DEVICE_KIND_GAMING_INPUT,
		UP_DEVICE_KIND_OTHER_AUDIO,
		UP_DEVICE_KIND_KEYBOARD,
		UP_DEVICE_KIND_TABLET,
		UP_DEVICE_KIND_TOUCHPAD,
		UP_DEVICE_KIND_MOUSE
	};
	/* Form-factors set in rules.d/78-sound-card.rules in systemd */
	struct {
		const char *form_factor;
		UpDeviceKind kind;
	} sound_types[] = {
		{ "webcam", UP_DEVICE_KIND_VIDEO },
		{ "speaker", UP_DEVICE_KIND_SPEAKERS },
		{ "headphone", UP_DEVICE_KIND_HEADPHONES },
		{ "headset", UP_DEVICE_KIND_HEADSET },
		/* unhandled:
		 * - handset
		 * - microphone */
	};

	input = G_UDEV_DEVICE (sibling);

	/* Do not process if we already have a "good" guess for the device type. */
	g_object_get (device, "type", &cur_type, NULL);
	if (cur_type == UP_DEVICE_KIND_LINE_POWER)
		return;

	if (g_strcmp0 (g_udev_device_get_subsystem (input), "input") != 0 &&
	    g_strcmp0 (g_udev_device_get_subsystem (input), "sound") != 0)
		return;

	/* Only process "card" devices, as those are tagged with form-factor */
	if (g_str_equal (g_udev_device_get_subsystem (input), "sound") &&
	    !g_str_has_prefix (g_udev_device_get_name (input), "card"))
		return;

	g_object_get (device,
		      "model", &model_name,
		      "serial", &serial_number,
		      NULL);

	if (model_name == NULL) {
		model_name = up_device_supply_get_string (input, "name");
		up_make_safe_string (model_name);
		g_object_set (device,
			      "model", model_name,
			      NULL);
	}

	if (serial_number == NULL) {
		serial_number = up_device_supply_get_string (input, "uniq");
		up_make_safe_string (serial_number);
		g_object_set (device,
			      "serial", serial_number,
			      NULL);
	}

	new_type = UP_DEVICE_KIND_UNKNOWN;

	for (i = 0; i < G_N_ELEMENTS (types); i++) {
		if (g_udev_device_get_property_as_boolean (input, types[i].prop)) {
			new_type = types[i].type;
			break;
		}
	}

	for (i = 0; i < G_N_ELEMENTS (priority); i++) {
		if (priority[i] == cur_type || priority[i] == new_type) {
			new_type = priority[i];
			break;
		}
	}

	/* Match audio sub-type */
	if (new_type == UP_DEVICE_KIND_OTHER_AUDIO) {
		const char *form_factor = g_udev_device_get_property (input, "SOUND_FORM_FACTOR");
		g_debug ("Guessing audio sub-type from SOUND_FORM_FACTOR='%s'", form_factor);
		for (i = 0; form_factor != NULL && i < G_N_ELEMENTS (sound_types); i++) {
			if (g_strcmp0 (form_factor, sound_types[i].form_factor) == 0) {
				new_type = sound_types[i].kind;
				break;
			}
		}
	}

	/* TODO: Add a heuristic here (and during initial discovery) that uses
	 *       the model name.
	 */

	/* Fall back to "keyboard" if we didn't find anything. */
	if (new_type == UP_DEVICE_KIND_UNKNOWN) {
		if (cur_type != UP_DEVICE_KIND_UNKNOWN) {
			g_debug ("Not overwriting existing type '%s'",
				 up_device_kind_to_string(cur_type));
			return;
		}
		new_type = UP_DEVICE_KIND_KEYBOARD;
	}

	if (cur_type != new_type) {
		native_device = G_UDEV_DEVICE (up_device_get_native (device));
		parent_device = g_udev_device_get_parent (native_device);
		parent_sibling = g_udev_device_get_parent (input);

		g_debug ("Type changed from %s to %s",
			 up_device_kind_to_string (cur_type),
			 up_device_kind_to_string (new_type));

		/* Check if the device and the sibling have the same parent. */
		if (!g_strcmp0 (g_udev_device_get_sysfs_path (parent_device),
				g_udev_device_get_sysfs_path (parent_sibling)))
			is_same_parent = TRUE;

		new_model_name = up_device_supply_get_string (input, "name");
		/* The model name of a device component may be different. For example, DualSense
		 * joystick owns "Sony Interactive Entertainment DualSense Wireless Controller"
		 * for the joystick and "Sony Interactive Entertainment DualSense Wireless Controller
		 * Motion Sensors" for the accelerometer. If the type is change, the corresponding
		 * model name have to be changed too. */
		if (new_model_name != NULL && is_same_parent) {
			up_make_safe_string (new_model_name);
			g_object_set (device,
				      "type", new_type,
				      "model", new_model_name,
				      NULL);
		} else {
			g_object_set (device, "type", new_type, NULL);
		}
	}
}

static void
up_device_supply_sibling_discovered_handle_wireless_status (UpDevice *device,
							    GObject  *obj)
{
	const char *status;
	GUdevDevice *sibling = G_UDEV_DEVICE (obj);

	status = g_udev_device_get_sysfs_attr_uncached (sibling, "wireless_status");
	if (!status)
		return;

	if (!g_str_equal (status, "connected") &&
	    !g_str_equal (status, "disconnected")) {
		g_warning ("Unhandled wireless_status value '%s' on %s",
			   status, g_udev_device_get_sysfs_path (sibling));
		return;
	}

	g_debug ("Detected wireless_status '%s' on %s",
		 status, g_udev_device_get_sysfs_path (sibling));

	g_object_set (G_OBJECT (device),
		      "disconnected", g_str_equal (status, "disconnected"),
		      NULL);
}

static void
up_device_supply_sibling_discovered (UpDevice *device,
				     GObject  *sibling)
{
	GUdevDevice *native;

	if (!G_UDEV_IS_DEVICE (sibling))
		return;

	native = G_UDEV_DEVICE (up_device_get_native (device));
	g_debug ("up_device_supply_sibling_discovered (device: %s, sibling: %s)",
		 g_udev_device_get_sysfs_path (native),
		 g_udev_device_get_sysfs_path (G_UDEV_DEVICE (sibling)));

	up_device_supply_sibling_discovered_guess_type (device, sibling);
	up_device_supply_sibling_discovered_handle_wireless_status (device, sibling);
}

static UpDeviceKind
up_device_supply_guess_type (GUdevDevice *native,
			     const char *native_path)
{
	g_autofree gchar *device_type = NULL;
	UpDeviceKind type = UP_DEVICE_KIND_UNKNOWN;

	device_type = up_device_supply_get_string (native, "type");
	if (device_type == NULL)
		return type;

	if (g_ascii_strcasecmp (device_type, "mains") == 0) {
		type = UP_DEVICE_KIND_LINE_POWER;
		goto out;
	}

	if (g_ascii_strcasecmp (device_type, "battery") == 0) {
		type = UP_DEVICE_KIND_BATTERY;

	} else if (g_ascii_strcasecmp (device_type, "USB") == 0) {

		/* USB supplies should have a usb_type attribute which we would
		 * ideally decode further.
		 *
		 * For historic reasons, we have a heuristic for wacom tablets
		 * that can be dropped in the future.
		 * As of May 2022, it is expected to be fixed in kernel 5.19.
		 * https://patchwork.kernel.org/project/linux-input/patch/20220407115406.115112-1-hadess@hadess.net/
		 */
		if (g_udev_device_has_sysfs_attr (native, "usb_type") &&
		    g_udev_device_has_sysfs_attr (native, "online"))
			type = UP_DEVICE_KIND_LINE_POWER;
		else if (g_strstr_len (native_path, -1, "wacom_") != NULL)
			type = UP_DEVICE_KIND_TABLET;
		else
			g_warning ("USB power supply %s without usb_type property, please report",
				   native_path);
	} else {
		g_warning ("did not recognize type %s, please report", device_type);
	}

out:
	return type;
}

/**
 * up_device_supply_coldplug:
 *
 * Return %TRUE on success, %FALSE if we failed to get data and should be removed
 **/
static gboolean
up_device_supply_coldplug (UpDevice *device)
{
	UpDeviceSupply *supply = UP_DEVICE_SUPPLY (device);
	GUdevDevice *native;
	const gchar *native_path;
	const gchar *scope;
	UpDeviceKind type;
	gboolean is_power_supply;

	up_device_supply_reset_values (supply);

	/* detect what kind of device we are */
	native = G_UDEV_DEVICE (up_device_get_native (device));
	native_path = g_udev_device_get_sysfs_path (native);
	if (native_path == NULL) {
		g_warning ("could not get native path for %p", device);
		return FALSE;
	}

	/* try to work out if the device is powering the system */
	scope = g_udev_device_get_sysfs_attr (native, "scope");
	if (scope != NULL && g_ascii_strcasecmp (scope, "device") == 0) {
		is_power_supply = FALSE;
	} else if (scope != NULL && g_ascii_strcasecmp (scope, "system") == 0) {
		is_power_supply = TRUE;
	} else {
		g_debug ("taking a guess for power supply scope");
		is_power_supply = TRUE;
	}

	/* we don't use separate ACs for devices */
	if (is_power_supply == FALSE &&
	    !g_udev_device_has_sysfs_attr_uncached (native, "capacity") &&
	    !g_udev_device_has_sysfs_attr_uncached (native, "capacity_level")) {
		g_debug ("Ignoring device AC, we'll monitor the device battery");
		return FALSE;
	}

	/* try to detect using the device type */
	type = up_device_supply_guess_type (native, native_path);

	/* if reading the device type did not work, use heuristic */
	if (type == UP_DEVICE_KIND_UNKNOWN) {
		if (g_udev_device_has_sysfs_attr_uncached (native, "online")) {
			g_debug ("'online' attribute was found. "
				 "Assume it is a line power supply.");
			type = UP_DEVICE_KIND_LINE_POWER;
		} else {
			/*
			 * This might be a battery or a UPS, but it might also
			 * be something else that we really don't know how to
			 * handle (e.g. BMS, exposed by Android-centric vendor
			 * kernels in parallel to actual battery).
			 *
			 * As such, we have no choice but to assume that we
			 * can't handle this device, and ignore it.
			 */
			return FALSE;
		}
	}

	/* set the value */
	g_object_set (device,
		     "type", type,
		     "power-supply", is_power_supply,
		     NULL);

	/* Handled by separate battery class */
	if (is_power_supply)
		g_assert (type == UP_DEVICE_KIND_LINE_POWER);

	if (type != UP_DEVICE_KIND_LINE_POWER)
		g_object_set (device, "poll-timeout", UP_DAEMON_SHORT_TIMEOUT, NULL);

	return TRUE;
}

static gboolean
up_device_supply_refresh (UpDevice *device, UpRefreshReason reason)
{
	gboolean updated;
	UpDeviceSupply *supply = UP_DEVICE_SUPPLY (device);
	UpDeviceKind type;

	g_object_get (device,
		      "type", &type,
		      NULL);
	if (type == UP_DEVICE_KIND_LINE_POWER) {
		updated = up_device_supply_refresh_line_power (supply, reason);
	} else {
		updated = up_device_supply_refresh_device (supply, reason);
	}

	/* reset time if we got new data */
	if (updated)
		g_object_set (device, "update-time", (guint64) g_get_real_time () / G_USEC_PER_SEC, NULL);

	return updated;
}

/**
 * up_device_supply_init:
 **/
static void
up_device_supply_init (UpDeviceSupply *supply)
{
	supply->priv = up_device_supply_get_instance_private (supply);

	supply->priv->shown_invalid_voltage_warning = FALSE;
}

/**
 * up_device_supply_finalize:
 **/
static void
up_device_supply_finalize (GObject *object)
{
	UpDeviceSupply *supply;

	g_return_if_fail (object != NULL);
	g_return_if_fail (UP_IS_DEVICE_SUPPLY (object));

	supply = UP_DEVICE_SUPPLY (object);
	g_return_if_fail (supply->priv != NULL);

	G_OBJECT_CLASS (up_device_supply_parent_class)->finalize (object);
}

/**
 * up_device_supply_class_init:
 **/
static void
up_device_supply_class_init (UpDeviceSupplyClass *klass)
{
	GObjectClass *object_class = G_OBJECT_CLASS (klass);
	UpDeviceClass *device_class = UP_DEVICE_CLASS (klass);

	object_class->finalize = up_device_supply_finalize;

	device_class->get_online = up_device_supply_get_online;
	device_class->coldplug = up_device_supply_coldplug;
	device_class->sibling_discovered = up_device_supply_sibling_discovered;
	device_class->refresh = up_device_supply_refresh;
}
