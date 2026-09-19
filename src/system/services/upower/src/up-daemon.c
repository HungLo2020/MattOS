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
#include <stdlib.h>

#include <glib.h>
#include <glib/gi18n-lib.h>
#include <glib-object.h>

#include "up-config.h"
#include "up-constants.h"
#include "up-polkit.h"
#include "up-device-list.h"
#include "up-device.h"
#include "up-device-kbd-backlight.h"
#include "up-backend.h"
#include "up-daemon.h"

struct UpDaemonPrivate
{
	UpConfig		*config;
	gboolean		 debug;
	UpPolkit		*polkit;
	UpBackend		*backend;
	UpDeviceList		*power_devices;
	UpDeviceList		*kbd_backlight_devices;
	guint			 action_timeout_id;
	guint			 refresh_batteries_id;
	guint			 warning_level_id;
	gboolean                 poll_paused;
	GSource                 *poll_source;
	int			 critical_action_lock_fd;

	/* Display battery properties */
	UpDevice		*display_device;
	UpDeviceKind		 kind;
	UpDeviceState		 state;
	gdouble			 percentage;
	gdouble			 energy;
	gdouble			 energy_full;
	gdouble			 energy_rate;
	gint64			 time_to_empty;
	gint64			 time_to_full;

	/* Composite keyboard backlight properties */
	UpDeviceKbdBacklight	*display_kbd_backlight_device;

	gboolean		 charge_threshold_enabled;
	gboolean		 state_all_discharging;

	/* WarningLevel configuration */
	gboolean		 use_percentage_for_policy;
	gdouble			 low_percentage;
	gdouble			 critical_percentage;
	gdouble			 action_percentage;
	guint			 low_time;
	guint			 critical_time;
	guint			 action_time;

	/* environment variable override */
	const char		*state_dir_override;
};

static void	up_daemon_finalize		(GObject	*object);
static gboolean	up_daemon_get_on_battery_local	(UpDaemon	*daemon);
static UpDeviceLevel up_daemon_get_warning_level_local(UpDaemon	*daemon);
static void	up_daemon_update_warning_level	(UpDaemon	*daemon);
static gboolean	up_daemon_get_on_ac_local 	(UpDaemon	*daemon, gboolean *has_ac);

G_DEFINE_TYPE_WITH_PRIVATE (UpDaemon, up_daemon, UP_TYPE_EXPORTED_DAEMON_SKELETON)

#define UP_DAEMON_ACTION_DELAY				20 /* seconds */
#define UP_INTERFACE_PREFIX				"org.freedesktop.UPower."

/**
 * up_daemon_get_on_battery_local:
 *
 * As soon as _any_ battery goes discharging, this is true
 **/
static gboolean
up_daemon_get_on_battery_local (UpDaemon *daemon)
{
	/* Use the cached composite state cached from the display device */
	return daemon->priv->state == UP_DEVICE_STATE_DISCHARGING;
}

/**
 * up_daemon_get_number_devices_of_type:
 **/
guint
up_daemon_get_number_devices_of_type (UpDaemon *daemon, UpDeviceKind type)
{
	guint i;
	UpDevice *device;
	GPtrArray *array;
	UpDeviceKind type_tmp;
	guint count = 0;

	/* ask each device */
	array = up_device_list_get_array (daemon->priv->power_devices);
	for (i=0; i<array->len; i++) {
		device = (UpDevice *) g_ptr_array_index (array, i);
		g_object_get (device,
			      "type", &type_tmp,
			      NULL);
		if (type == type_tmp &&
		    up_device_get_object_path (device) != NULL)
			count++;
	}
	g_ptr_array_unref (array);
	return count;
}

/**
 * up_daemon_update_display_battery:
 *
 * Update our internal state.
 *
 * Returns: %TRUE if the state changed.
 **/
static gboolean
up_daemon_update_display_battery (UpDaemon *daemon)
{
	guint i;
	GPtrArray *array;

	UpDeviceKind kind_total = UP_DEVICE_KIND_UNKNOWN;
	/* Abuse LAST to know if any battery had a state. */
	UpDeviceState state_total = UP_DEVICE_STATE_LAST;
	gdouble percentage_total = 0.0;
	gdouble energy_total = 0.0;
	gdouble energy_full_total = 0.0;
	gdouble energy_rate_total = 0.0;
	gint64 time_to_empty_total = 0;
	gint64 time_to_full_total = 0;
	gboolean is_present_total = FALSE;
	gboolean charge_threshold_enabled_total = FALSE;
	guint num_batteries = 0;

	gboolean state_all_discharging = TRUE;
	gboolean state_any_discharging = FALSE;

	/* Gather state from each device */
	array = up_device_list_get_array (daemon->priv->power_devices);
	for (i = 0; i < array->len; i++) {
		UpDevice *device;

		UpDeviceState state = UP_DEVICE_STATE_UNKNOWN;
		UpDeviceKind kind = UP_DEVICE_KIND_UNKNOWN;
		gboolean present = FALSE;
		gdouble percentage = 0.0;
		gdouble energy = 0.0;
		gdouble energy_full = 0.0;
		gdouble energy_rate = 0.0;
		gint64 time_to_empty = 0;
		gint64 time_to_full = 0;
		gboolean power_supply = FALSE;
		gboolean charge_threshold_enabled = FALSE;

		device = g_ptr_array_index (array, i);
		g_object_get (device,
			      "is-present", &present,
			      "type", &kind,
			      "state", &state,
			      "percentage", &percentage,
			      "energy", &energy,
			      "energy-full", &energy_full,
			      "energy-rate", &energy_rate,
			      "time-to-empty", &time_to_empty,
			      "time-to-full", &time_to_full,
			      "power-supply", &power_supply,
			      "charge-threshold-enabled", &charge_threshold_enabled,
			      NULL);

		if (!present)
			continue;

		/* When we have a UPS, it's either a desktop, and
		 * has no batteries, or a laptop, in which case we
		 * ignore the batteries */
		if (kind == UP_DEVICE_KIND_UPS) {
			kind_total = kind;
			state_total = state;
			energy_total = energy;
			energy_full_total = energy_full;
			energy_rate_total = energy_rate;
			time_to_empty_total = time_to_empty;
			time_to_full_total = time_to_full;
			percentage_total = percentage;
			is_present_total = TRUE;
			break;
		}
		if (kind != UP_DEVICE_KIND_BATTERY ||
		    power_supply == FALSE)
			continue;

		/*
		 * If one battery is charging, the composite is charging
		 * If one batteries is discharging, the composite is discharging
		 * If one battery is unknown, and we don't have a charging/discharging state otherwise, mark unknown
		 * If one battery is pending-charge and no other is charging or discharging, then the composite is pending-charge
		 * If all batteries are fully charged, the composite is fully charged
		 * If all batteries are empty, the composite is empty
		 * Everything else is unknown
		 */
		/* Keep a charging/discharging state (warn about conflict) */
		if (state_total == UP_DEVICE_STATE_CHARGING || state_total == UP_DEVICE_STATE_DISCHARGING) {
			if (state != state_total && (state == UP_DEVICE_STATE_CHARGING || state == UP_DEVICE_STATE_DISCHARGING))
				g_warning ("Conflicting charge/discharge state between batteries!");
		} else if (state == UP_DEVICE_STATE_CHARGING)
			state_total = UP_DEVICE_STATE_CHARGING;
		else if (state == UP_DEVICE_STATE_DISCHARGING)
			state_total = UP_DEVICE_STATE_DISCHARGING;
		else if (state == UP_DEVICE_STATE_UNKNOWN)
			state_total = UP_DEVICE_STATE_UNKNOWN;
		else if (state == UP_DEVICE_STATE_PENDING_CHARGE)
			state_total = UP_DEVICE_STATE_PENDING_CHARGE;
		else if (state == UP_DEVICE_STATE_FULLY_CHARGED &&
			 (state_total == UP_DEVICE_STATE_FULLY_CHARGED || state_total == UP_DEVICE_STATE_LAST))
			state_total = UP_DEVICE_STATE_FULLY_CHARGED;
		else if (state == UP_DEVICE_STATE_EMPTY &&
			 (state_total == UP_DEVICE_STATE_EMPTY || state_total == UP_DEVICE_STATE_LAST))
			state_total = UP_DEVICE_STATE_EMPTY;
		else
			state_total = UP_DEVICE_STATE_UNKNOWN;

		/* Update charging state variables by considering any battery that is charging or fully charged to not
		 * be discharging. Additionally, also update state_any_discharge for any batteries explicitly
		 * discharging. */
		if (state == UP_DEVICE_STATE_CHARGING || state == UP_DEVICE_STATE_FULLY_CHARGED) {
			state_all_discharging = FALSE;
		} else if (state == UP_DEVICE_STATE_DISCHARGING)
			state_any_discharging = TRUE;

		/* If at least one battery has charge thresholds enabled, propagate that. */
		charge_threshold_enabled_total = charge_threshold_enabled_total || charge_threshold_enabled;

		/* sum up composite */
		kind_total = UP_DEVICE_KIND_BATTERY;
		is_present_total = TRUE;
		energy_total += energy;
		energy_full_total += energy_full;
		energy_rate_total += energy_rate;
		time_to_empty_total += time_to_empty;
		time_to_full_total += time_to_full;
		/* Will be recalculated for multiple batteries, no worries */
		percentage_total += percentage;
		num_batteries++;
	}

	/* Handle multiple batteries */
	if (num_batteries <= 1)
		goto out;

	g_debug ("Calculating percentage and time to full/to empty for %i batteries", num_batteries);

	/* use percentage weighted for each battery capacity
	 * fall back to averaging the batteries.
	 * ASSUMPTION: If one battery has energy data, then all batteries do
	 */
	if (energy_full_total > 0.0)
		percentage_total = 100.0 * energy_total / energy_full_total;
	else
		percentage_total = percentage_total / num_batteries;

out:
	g_ptr_array_unref (array);

	/* No battery means LAST state. If we have an UNKNOWN state (with
	 * a battery) then try to infer one. */
	if (state_total == UP_DEVICE_STATE_LAST) {
		state_total = UP_DEVICE_STATE_UNKNOWN;
	} else if (state_total == UP_DEVICE_STATE_UNKNOWN) {
		gboolean has_ac, ac_online;

		ac_online = up_daemon_get_on_ac_local (daemon, &has_ac);

		if (has_ac && ac_online) {
			if (percentage_total >= UP_FULLY_CHARGED_THRESHOLD)
				state_total = UP_DEVICE_STATE_FULLY_CHARGED;
			else
				state_total = UP_DEVICE_STATE_CHARGING;
		} else {
			if (percentage_total < 1.0f)
				state_total = UP_DEVICE_STATE_EMPTY;
			else
				state_total = UP_DEVICE_STATE_DISCHARGING;
		}
	}

	/* calculate a quick and dirty time remaining value
	 * NOTE: Keep in sync with per-battery estimation code! */
	if (energy_rate_total > 0) {
		if (state_total == UP_DEVICE_STATE_DISCHARGING)
			time_to_empty_total = SECONDS_PER_HOUR * (energy_total / energy_rate_total);
		else if (state_total == UP_DEVICE_STATE_CHARGING)
			time_to_full_total = SECONDS_PER_HOUR * ((energy_full_total - energy_total) / energy_rate_total);
	}

	/* Compute state_all_discharging by ensuring at least one battery is discharging */
	state_all_discharging = state_all_discharging && state_any_discharging;

	/* Did anything change? */
	if (daemon->priv->kind == kind_total &&
	    daemon->priv->state == state_total &&
	    daemon->priv->energy == energy_total &&
	    daemon->priv->energy_full == energy_full_total &&
	    daemon->priv->energy_rate == energy_rate_total &&
	    daemon->priv->time_to_empty == time_to_empty_total &&
	    daemon->priv->time_to_full == time_to_full_total &&
	    daemon->priv->percentage == percentage_total &&
	    daemon->priv->charge_threshold_enabled == charge_threshold_enabled_total &&
	    daemon->priv->state_all_discharging == state_all_discharging)
		return FALSE;

	daemon->priv->kind = kind_total;
	daemon->priv->state = state_total;
	daemon->priv->energy = energy_total;
	daemon->priv->energy_full = energy_full_total;
	daemon->priv->energy_rate = energy_rate_total;
	daemon->priv->time_to_empty = time_to_empty_total;
	daemon->priv->time_to_full = time_to_full_total;

	daemon->priv->percentage = percentage_total;

	daemon->priv->charge_threshold_enabled = charge_threshold_enabled_total;
	daemon->priv->state_all_discharging = state_all_discharging;

	g_object_set (daemon->priv->display_device,
		      "type", kind_total,
		      "state", state_total,
		      "energy", energy_total,
		      "energy-full", energy_full_total,
		      "energy-rate", energy_rate_total,
		      "time-to-empty", time_to_empty_total,
		      "time-to-full", time_to_full_total,
		      "percentage", percentage_total,
		      "is-present", is_present_total,
		      "charge-threshold-enabled", charge_threshold_enabled_total,
		      "power-supply", TRUE,
		      "update-time", (guint64) g_get_real_time () / G_USEC_PER_SEC,
		      NULL);

	return TRUE;
}

/**
 * up_daemon_kbd_backlight_composite_update:
 *
 * Recompute the composite keyboard backlight brightness from the
 * underlying physical devices and update the display device.
 *
 * Returns: %TRUE on success.
 **/
gboolean
up_daemon_kbd_backlight_composite_update (UpDaemon *daemon, gboolean composite_signal)
{
	GPtrArray *array;
	gint composite_brightness = -1;
	gint lowest_max_brightness = G_MAXINT;

	array = up_device_list_get_array (daemon->priv->kbd_backlight_devices);
	for (guint i = 0; i < array->len; i++) {
		UpDeviceKbdBacklight *kbd_device;
		gint max_brightness = -1;
		gint brightness = -1;

		kbd_device = UP_DEVICE_KBD_BACKLIGHT (g_ptr_array_index (array, i));

		g_object_get (kbd_device, "max-brightness", &max_brightness,
			      "brightness", &brightness,
			      NULL);

		if (max_brightness >= 0 && max_brightness < lowest_max_brightness) {
			lowest_max_brightness = max_brightness;
			/* Use the brightness of the device with the lowest max_brightness */
			composite_brightness = brightness;
		}
	}

	g_ptr_array_unref (array);

	g_object_set (daemon->priv->display_kbd_backlight_device,
		      "max-brightness", lowest_max_brightness == G_MAXINT ? 0 : lowest_max_brightness,
		      "brightness", composite_brightness >= 0 ? composite_brightness : 0,
		      NULL);

	if (composite_signal)
		up_device_kbd_backlight_emit_change (daemon->priv->display_kbd_backlight_device,
						     composite_brightness >= 0 ? composite_brightness : 0,
						     "internal");

	return TRUE;
}

/**
 * up_daemon_get_warning_level_local:
 *
 * As soon as _all_ batteries are low, external power is not available or not charging at least one battery, this is true
 **/
static UpDeviceLevel
up_daemon_get_warning_level_local (UpDaemon *daemon)
{
	if (daemon->priv->kind != UP_DEVICE_KIND_UPS &&
	    daemon->priv->kind != UP_DEVICE_KIND_BATTERY)
		return UP_DEVICE_LEVEL_NONE;

	if (daemon->priv->kind == UP_DEVICE_KIND_UPS &&
	    daemon->priv->state != UP_DEVICE_STATE_DISCHARGING)
		return UP_DEVICE_LEVEL_NONE;

	/* Ignore battery level if we have external power and not all batteries are discharging */
	if (daemon->priv->kind == UP_DEVICE_KIND_BATTERY &&
	    up_daemon_get_on_ac_local (daemon, NULL) &&
	    !daemon->priv->state_all_discharging)
		return UP_DEVICE_LEVEL_NONE;

	return up_daemon_compute_warning_level (daemon,
						daemon->priv->state,
						daemon->priv->kind,
						TRUE, /* power_supply */
						daemon->priv->percentage,
						daemon->priv->time_to_empty);
}

/**
 * up_daemon_get_on_ac_local:
 *
 * As soon as _any_ ac supply goes online, this is true
 **/
static gboolean
up_daemon_get_on_ac_local (UpDaemon *daemon, gboolean *has_ac)
{
	guint i;
	gboolean ret;
	gboolean result = FALSE;
	gboolean online;
	UpDevice *device;
	GPtrArray *array;

	if (has_ac)
		*has_ac = FALSE;

	/* ask each device */
	array = up_device_list_get_array (daemon->priv->power_devices);
	for (i=0; i<array->len; i++) {
		device = (UpDevice *) g_ptr_array_index (array, i);
		ret = up_device_get_online (device, &online);
		if (has_ac && ret)
			*has_ac = TRUE;
		if (ret && online) {
			result = TRUE;
			break;
		}
	}
	g_ptr_array_unref (array);
	return result;
}

static gboolean
up_daemon_refresh_battery_devices_idle (UpDaemon *daemon)
{
	guint i;
	GPtrArray *array;
	UpDevice *device;

	/* refresh all devices in array */
	array = up_device_list_get_array (daemon->priv->power_devices);
	for (i=0; i<array->len; i++) {
		UpDeviceKind type;
		gboolean power_supply;

		device = (UpDevice *) g_ptr_array_index (array, i);
		/* only refresh battery devices */
		g_object_get (device,
			      "type", &type,
			      "power-supply", &power_supply,
			      NULL);
		if (type == UP_DEVICE_KIND_BATTERY &&
		    power_supply)
			up_device_refresh_internal (device, UP_REFRESH_LINE_POWER);
	}
	g_ptr_array_unref (array);

	daemon->priv->refresh_batteries_id = 0;
	return G_SOURCE_REMOVE;
}

static void
up_daemon_refresh_battery_devices (UpDaemon *daemon)
{
	if (daemon->priv->refresh_batteries_id)
		return;

	daemon->priv->refresh_batteries_id = g_idle_add ((GSourceFunc) up_daemon_refresh_battery_devices_idle, daemon);
}

/**
 * up_daemon_enumerate_devices:
 **/
static gboolean
up_daemon_enumerate_devices (UpExportedDaemon *skeleton,
			     GDBusMethodInvocation *invocation,
			     UpDaemon *daemon)
{
	guint i;
	GPtrArray *array;
	GPtrArray *object_paths;
	UpDevice *device;

	/* build a pointer array of the object paths */
	object_paths = g_ptr_array_new_with_free_func (g_free);
	array = up_device_list_get_array (daemon->priv->power_devices);
	for (i = 0; i < array->len; i++) {
		const char *object_path;
		device = (UpDevice *) g_ptr_array_index (array, i);
		object_path = up_device_get_object_path (device);
		if (object_path != NULL)
			g_ptr_array_add (object_paths, g_strdup (object_path));
	}
	g_ptr_array_unref (array);
	g_ptr_array_add (object_paths, NULL);

	/* return it on the bus */
	up_exported_daemon_complete_enumerate_devices (skeleton, invocation,
						       (const gchar **) object_paths->pdata);

	/* free */
	g_ptr_array_unref (object_paths);
	return TRUE;
}

static gboolean
up_daemon_enumerate_kbd_backlights (UpExportedDaemon *skeleton,
				    GDBusMethodInvocation *invocation,
				    UpDaemon *daemon)
{
	guint i;
	GPtrArray *array;
	GPtrArray *object_paths;
	UpDeviceKbdBacklight *kbd_backlight;

	/* build a pointer array of the object paths */
	object_paths = g_ptr_array_new_with_free_func (g_free);
	array = up_device_list_get_array (daemon->priv->kbd_backlight_devices);
	for (i = 0; i < array->len; i++) {
		const char *object_path;
		kbd_backlight = (UpDeviceKbdBacklight *) g_ptr_array_index (array, i);
		object_path = up_device_kbd_backlight_get_object_path (kbd_backlight);
		if (object_path != NULL)
			g_ptr_array_add (object_paths, g_strdup (object_path));
	}
	g_ptr_array_unref (array);
	g_ptr_array_add (object_paths, NULL);

	/* return it on the bus */
	up_exported_daemon_complete_enumerate_kbd_backlights (skeleton, invocation,
							      (const gchar **) object_paths->pdata);

	/* free */
	g_ptr_array_unref (object_paths);
	return TRUE;
}

/**
 * up_daemon_get_display_device:
 **/
static gboolean
up_daemon_get_display_device (UpExportedDaemon *skeleton,
			      GDBusMethodInvocation *invocation,
			      UpDaemon *daemon)
{
	up_exported_daemon_complete_get_display_device (skeleton, invocation,
							up_device_get_object_path (daemon->priv->display_device));
	return TRUE;
}

/**
 * up_daemon_get_critical_action:
 **/
static gboolean
up_daemon_get_critical_action (UpExportedDaemon *skeleton,
			       GDBusMethodInvocation *invocation,
			       UpDaemon *daemon)
{
	up_exported_daemon_complete_get_critical_action (skeleton, invocation,
							 up_backend_get_critical_action (daemon->priv->backend));
	return TRUE;
}

/**
 * up_daemon_register_power_daemon:
 **/
static gboolean
up_daemon_register_power_daemon (UpDaemon *daemon,
				 GDBusConnection *connection)
{
	GError *error = NULL;

	/* export our interface on the bus */
	g_dbus_interface_skeleton_export (G_DBUS_INTERFACE_SKELETON (daemon),
					  connection,
					  "/org/freedesktop/UPower",
					  &error);

	if (error != NULL) {
		g_critical ("error registering daemon on system bus: %s", error->message);
		g_error_free (error);
		return FALSE;
	}

	/* Register the display device */
	g_initable_init (G_INITABLE (daemon->priv->display_device), NULL, NULL);
	g_initable_init (G_INITABLE (daemon->priv->display_kbd_backlight_device), NULL, NULL);

	return TRUE;
}

/**
 * up_daemon_startup:
 **/
gboolean
up_daemon_startup (UpDaemon *daemon,
		   GDBusConnection *connection)
{
	gboolean ret;
	UpDaemonPrivate *priv = daemon->priv;

	/* register on bus */
	ret = up_daemon_register_power_daemon (daemon, connection);
	if (!ret) {
		g_warning ("failed to register");
		goto out;
	}

	g_debug ("daemon now coldplug");

	/* coldplug backend backend */
	ret = up_backend_coldplug (priv->backend, daemon);
	if (!ret) {
		g_warning ("failed to coldplug backend");
		goto out;
	}

	/* get battery state */
	up_daemon_update_warning_level (daemon);

	/* Run mainloop now to avoid state changes on DBus */
	while (g_main_context_iteration (NULL, FALSE)) { }

	g_debug ("daemon now not coldplug");

out:
	return ret;
}

/**
 * up_daemon_shutdown:
 *
 * Stop the daemon, release all devices and resources.
 **/
void
up_daemon_shutdown (UpDaemon *daemon)
{
	/* stop accepting new devices and clear backend state */
	up_backend_unplug (daemon->priv->backend);

	/* forget about discovered devices */
	up_device_list_clear (daemon->priv->power_devices);
	up_device_list_clear (daemon->priv->kbd_backlight_devices);

	/* release UpDaemon reference */
	g_object_run_dispose (G_OBJECT (daemon->priv->display_device));
	g_object_run_dispose (G_OBJECT (daemon->priv->display_kbd_backlight_device));
}

/**
 * up_daemon_get_device_list:
 **/
UpDeviceList *
up_daemon_get_device_list (UpDaemon *daemon)
{
	return g_object_ref (daemon->priv->power_devices);
}

/**
 * up_daemon_set_lid_is_closed:
 **/
void
up_daemon_set_lid_is_closed (UpDaemon *daemon, gboolean lid_is_closed)
{
	UpDaemonPrivate *priv = daemon->priv;

	/* check if we are ignoring the lid */
	if (up_config_get_boolean (priv->config, "IgnoreLid")) {
		g_debug ("ignoring lid state");
		return;
	}

	g_debug ("lid_is_closed = %s", lid_is_closed ? "yes" : "no");
	up_exported_daemon_set_lid_is_closed (UP_EXPORTED_DAEMON (daemon), lid_is_closed);
}

/**
 * up_daemon_set_lid_is_present:
 **/
void
up_daemon_set_lid_is_present (UpDaemon *daemon, gboolean lid_is_present)
{
	UpDaemonPrivate *priv = daemon->priv;

	/* check if we are ignoring the lid */
	if (up_config_get_boolean (priv->config, "IgnoreLid")) {
		g_debug ("ignoring lid state");
		return;
	}

	g_debug ("lid_is_present = %s", lid_is_present ? "yes" : "no");
	up_exported_daemon_set_lid_is_present (UP_EXPORTED_DAEMON (daemon), lid_is_present);
}

/**
 * up_daemon_set_on_battery:
 **/
void
up_daemon_set_on_battery (UpDaemon *daemon, gboolean on_battery)
{
	g_debug ("on_battery = %s", on_battery ? "yes" : "no");
	up_exported_daemon_set_on_battery (UP_EXPORTED_DAEMON (daemon), on_battery);
}

static gboolean
take_action_timeout_cb (UpDaemon *daemon)
{
	/* Release the inhibitor lock first, otherwise our action may be canceled */
	if (daemon->priv->critical_action_lock_fd >= 0) {
		close (daemon->priv->critical_action_lock_fd);
		daemon->priv->critical_action_lock_fd = -1;
	}

	up_backend_take_action (daemon->priv->backend);

	g_debug ("Backend was notified to take action. The timeout will be removed.");
	daemon->priv->action_timeout_id = 0;

	return G_SOURCE_REMOVE;
}

/**
 * up_daemon_set_warning_level:
 **/
void
up_daemon_set_warning_level (UpDaemon *daemon, UpDeviceLevel warning_level)
{
	UpDeviceLevel old_level;

	g_object_get (G_OBJECT (daemon->priv->display_device),
		      "warning-level", &old_level,
		      NULL);

	if (old_level == warning_level)
		return;

	g_debug ("warning_level = %s", up_device_level_to_string (warning_level));

	g_object_set (G_OBJECT (daemon->priv->display_device),
		      "warning-level", warning_level,
		      "update-time", (guint64) g_get_real_time () / G_USEC_PER_SEC,
		      NULL);

	if (warning_level == UP_DEVICE_LEVEL_ACTION) {
		if (daemon->priv->action_timeout_id == 0) {
			g_assert (daemon->priv->critical_action_lock_fd == -1);

			g_debug ("About to take action in %d seconds", UP_DAEMON_ACTION_DELAY);
			daemon->priv->critical_action_lock_fd = up_backend_inhibitor_lock_take (daemon->priv->backend, "Execute critical action", "block");
			daemon->priv->action_timeout_id = g_timeout_add_seconds (UP_DAEMON_ACTION_DELAY,
										 (GSourceFunc) take_action_timeout_cb,
										 daemon);
			g_source_set_name_by_id (daemon->priv->action_timeout_id, "[upower] take_action_timeout_cb");
		} else {
			g_debug ("Not taking action, timeout id already set");
		}
	} else {
		if (daemon->priv->action_timeout_id > 0) {
			g_debug ("Removing timeout as action level changed");
			g_clear_handle_id (&daemon->priv->action_timeout_id, g_source_remove);
		}

		if (daemon->priv->critical_action_lock_fd >= 0) {
			close (daemon->priv->critical_action_lock_fd);
			daemon->priv->critical_action_lock_fd = -1;
		}
	}
}

UpDeviceLevel
up_daemon_compute_warning_level (UpDaemon      *daemon,
				 UpDeviceState  state,
				 UpDeviceKind   kind,
				 gboolean       power_supply,
				 gdouble        percentage,
				 gint64         time_to_empty)
{
	gboolean can_risky = FALSE;
	gboolean expect_battery_recalibration = FALSE;
	gboolean use_percentage = TRUE;
	UpDeviceLevel default_level = UP_DEVICE_LEVEL_NONE;

	if (state != UP_DEVICE_STATE_DISCHARGING)
		return UP_DEVICE_LEVEL_NONE;

	/* Keyboard and mice usually have a coarser
	 * battery level, so this avoids falling directly
	 * into critical (or off) before any warnings */
	if (kind == UP_DEVICE_KIND_MOUSE ||
	    kind == UP_DEVICE_KIND_KEYBOARD ||
	    kind == UP_DEVICE_KIND_TOUCHPAD) {
		if (percentage <= 5.0f)
			return UP_DEVICE_LEVEL_CRITICAL;
		else if (percentage <= 10.0f)
			return  UP_DEVICE_LEVEL_LOW;
		else
			return UP_DEVICE_LEVEL_NONE;
	} else if (kind == UP_DEVICE_KIND_UPS) {
		default_level = UP_DEVICE_LEVEL_DISCHARGING;
	}

	/* Check if the battery is performing the battery recalibration and
	 * AC is online, the battery level is UP_DEVICE_LEVEL_NONE. */
	can_risky = up_config_get_boolean (daemon->priv->config,
					   "AllowRiskyCriticalPowerAction");
	expect_battery_recalibration = up_config_get_boolean (daemon->priv->config,
							      "ExpectBatteryRecalibration");

	if (can_risky && kind == UP_DEVICE_KIND_BATTERY) {
		if (expect_battery_recalibration &&
		    up_daemon_get_on_ac_local (daemon, NULL)) {
			g_debug ("ExpectBatteryRecalibration is enabled and the AC is connected, so the battery level is not critical");
			return UP_DEVICE_LEVEL_NONE;
		}
	}

	if (power_supply &&
	    !daemon->priv->use_percentage_for_policy &&
	    time_to_empty > 0.0)
		use_percentage = FALSE;

	if (use_percentage) {
		if (percentage > daemon->priv->low_percentage)
			return default_level;
		if (percentage > daemon->priv->critical_percentage)
			return UP_DEVICE_LEVEL_LOW;
		if (percentage > daemon->priv->action_percentage)
			return UP_DEVICE_LEVEL_CRITICAL;
		return UP_DEVICE_LEVEL_ACTION;
	} else {
		if (time_to_empty > daemon->priv->low_time)
			return default_level;
		if (time_to_empty > daemon->priv->critical_time)
			return UP_DEVICE_LEVEL_LOW;
		if (time_to_empty > daemon->priv->action_time)
			return UP_DEVICE_LEVEL_CRITICAL;
		return UP_DEVICE_LEVEL_ACTION;
	}
	g_assert_not_reached ();
}

static gboolean
up_daemon_update_warning_level_idle (UpDaemon *daemon)
{
	gboolean ret;
	UpDeviceLevel warning_level;

	up_daemon_update_display_battery (daemon);

	/* Check if the on_battery and warning_level state has changed */
	ret = (up_daemon_get_on_battery_local (daemon) && !up_daemon_get_on_ac_local (daemon, NULL));
	up_daemon_set_on_battery (daemon, ret);

	warning_level = up_daemon_get_warning_level_local (daemon);
	up_daemon_set_warning_level (daemon, warning_level);

	daemon->priv->warning_level_id = 0;
	return G_SOURCE_REMOVE;
}

static void
up_daemon_update_warning_level (UpDaemon *daemon)
{
	if (daemon->priv->warning_level_id)
		return;

	daemon->priv->warning_level_id = g_idle_add ((GSourceFunc) up_daemon_update_warning_level_idle, daemon);
}

const gchar *
up_daemon_get_charge_icon (UpDaemon     *daemon,
			   gdouble       percentage,
			   UpDeviceLevel battery_level,
			   gboolean      charging)
{
	if (battery_level == UP_DEVICE_LEVEL_NONE && daemon != NULL) {
		if (percentage <= daemon->priv->low_percentage)
			return charging ? "battery-caution-charging-symbolic" : "battery-caution-symbolic";
		else if (percentage < 30)
			return charging ? "battery-low-charging-symbolic" : "battery-low-symbolic";
		else if (percentage < 60)
			return charging ? "battery-good-charging-symbolic" : "battery-good-symbolic";
		return charging ? "battery-full-charging-symbolic" : "battery-full-symbolic";
	} else {
		switch (battery_level) {
		case UP_DEVICE_LEVEL_UNKNOWN:
			/* The lack of symmetry is on purpose */
			return charging ? "battery-good-charging-symbolic" : "battery-caution-symbolic";
		case UP_DEVICE_LEVEL_LOW:
		case UP_DEVICE_LEVEL_CRITICAL:
			return charging ? "battery-caution-charging-symbolic" : "battery-caution-symbolic";
		case UP_DEVICE_LEVEL_NORMAL:
			return charging ? "battery-low-charging-symbolic" : "battery-low-symbolic";
		case UP_DEVICE_LEVEL_HIGH:
			return charging ? "battery-good-charging-symbolic" : "battery-good-symbolic";
		case UP_DEVICE_LEVEL_FULL:
			return charging ? "battery-full-charging-symbolic" : "battery-full-symbolic";
		default:
			g_assert_not_reached ();
		}
	}
}

/**
 * up_daemon_polkit_is_allowed:
 **/
gboolean
up_daemon_polkit_is_allowed (UpDaemon *daemon, const gchar *action_id, GDBusMethodInvocation *invocation)
{
#ifdef HAVE_POLKIT
	g_autoptr (PolkitSubject) subject = NULL;
	g_autoptr (GError) error = NULL;

	subject = up_polkit_get_subject (daemon->priv->polkit, invocation);
	if (subject == NULL) {
		g_debug ("Can't get sender subject");
		return FALSE;
	}

	if (!up_polkit_is_allowed (daemon->priv->polkit, subject, action_id, &error)) {
		if (error != NULL)
			g_debug ("Error on Polkit check authority: %s", error->message);
		return FALSE;
	}
#endif

	return TRUE;
}

/**
 * up_daemon_device_changed_cb:
 **/
static void
up_daemon_device_changed_cb (UpDevice *device, GParamSpec *pspec, UpDaemon *daemon)
{
	UpDeviceKind type;
	const char *prop;

	g_return_if_fail (UP_IS_DAEMON (daemon));
	g_return_if_fail (UP_IS_DEVICE (device));

	prop = g_param_spec_get_name (pspec);
	if (!daemon->priv->poll_paused &&
	    ((g_strcmp0 (prop, "poll-timeout") == 0) ||
	     (g_strcmp0 (prop, "last-refresh") == 0))) {
		g_source_set_ready_time (daemon->priv->poll_source, 0);
		return;
	}

	/* refresh battery devices when AC state changes */
	g_object_get (device,
		      "type", &type,
		      NULL);
	if (type == UP_DEVICE_KIND_LINE_POWER && g_strcmp0 (prop, "online") == 0) {
		/* refresh now */
		up_daemon_refresh_battery_devices (daemon);
	}

	up_daemon_update_warning_level (daemon);
}

static gboolean
up_daemon_poll_dispatch (GSource *source, GSourceFunc callback, gpointer user_data)
{
	UpDaemon *daemon = UP_DAEMON (user_data);
	UpDaemonPrivate *priv = daemon->priv;
	g_autoptr(GPtrArray) array = NULL;
	guint i;
	UpDevice *device;
	gint64 ready_time = G_MAXINT64;
	gint64 now = g_source_get_time (priv->poll_source);
	gint max_dispatch_timeout = 0;

	g_source_set_ready_time (priv->poll_source, -1);
	g_assert (callback == NULL);

	if (daemon->priv->poll_paused)
		return G_SOURCE_CONTINUE;

	/* Find the earliest device that needs a refresh. */
	array = up_device_list_get_array (priv->power_devices);
	for (i = 0; i < array->len; i += 1) {
		gint timeout;
		gint64 last_refresh;
		gint64 poll_time;
		gint64 dispatch_time;
		device = (UpDevice *) g_ptr_array_index (array, i);
		g_object_get (device,
			      "poll-timeout", &timeout,
			      "last-refresh", &last_refresh,
			      NULL);

		if (timeout <= 0)
			continue;

		poll_time = last_refresh + timeout * G_USEC_PER_SEC;

		/* Allow dispatching early if another device got dispatched.
		 * i.e. device polling will synchronize eventually.
		 */
		dispatch_time = poll_time - MIN(timeout, max_dispatch_timeout) * G_USEC_PER_SEC / 2;

		if (now >= dispatch_time) {
			g_debug ("up_daemon_poll_dispatch: refreshing %s", up_exported_device_get_native_path (UP_EXPORTED_DEVICE (device)));
			up_device_refresh_internal (device, UP_REFRESH_POLL);
			max_dispatch_timeout = MAX(max_dispatch_timeout, timeout);

			/* We'll wake up again immediately and then
			 * calculate the correct time to re-poll. */
		}

		ready_time = MIN(ready_time, poll_time);
	}

	if (ready_time == G_MAXINT64)
		ready_time = -1;

	/* Set the ready time (if it was not modified externally) */
	if (g_source_get_ready_time (priv->poll_source) == -1)
		g_source_set_ready_time (priv->poll_source, ready_time);

	return G_SOURCE_CONTINUE;
}

GSourceFuncs poll_source_funcs = {
	.prepare = NULL,
	.check = NULL,
	.dispatch = up_daemon_poll_dispatch,
	.finalize = NULL,
};

/**
 * up_daemon_pause_poll:
 *
 * Pause, i.e. stop, all registered poll sources. They can be
 * restarted via up_daemon_resume_poll().
 **/
void
up_daemon_pause_poll (UpDaemon *daemon)
{
	g_debug ("Polling will be paused");

	daemon->priv->poll_paused = TRUE;
}

/**
 * up_daemon_resume_poll:
 *
 * Resume all poll sources; the timeout will be recalculated.
 **/
void
up_daemon_resume_poll (UpDaemon *daemon)
{
	g_debug ("Polling will be resumed");

	daemon->priv->poll_paused = FALSE;

	g_source_set_ready_time (daemon->priv->poll_source, 0);
}

void
up_daemon_set_debug (UpDaemon *daemon,
		     gboolean  debug)
{
	daemon->priv->debug = debug;
}

gboolean
up_daemon_get_debug (UpDaemon *daemon)
{
	return daemon->priv->debug;
}

/**
 * up_daemon_get_state_dir_env_override:
 *
 * Get UPOWER_STATE_DIR environment variable.
 **/
const gchar *
up_daemon_get_state_dir_env_override (UpDaemon *daemon)
{
	return daemon->priv->state_dir_override;
}

static void
up_daemon_get_env_override (UpDaemon *self)
{
	self->priv->state_dir_override = g_getenv ("UPOWER_STATE_DIR");
}

/**
 * up_daemon_device_added_cb:
 **/
static void
up_daemon_device_added_cb (UpBackend *backend, GObject *device, UpDaemon *daemon)
{
	const gchar *object_path;
	UpDaemonPrivate *priv = daemon->priv;

	g_return_if_fail (UP_IS_DAEMON (daemon));
	g_return_if_fail (UP_IS_DEVICE (device) || UP_IS_DEVICE_KBD_BACKLIGHT (device));

	if (UP_IS_DEVICE (device)) {
		/* power_supply */
		/* add to device list */
		up_device_list_insert (priv->power_devices, device);

		/* connect, so we get changes */
		g_signal_connect (device, "notify",
				  G_CALLBACK (up_daemon_device_changed_cb), daemon);

		/* emit */
		object_path = up_device_get_object_path (UP_DEVICE (device));
		if (object_path == NULL) {
			g_debug ("Device %s was unregistered before it was on the bus",
				 up_exported_device_get_native_path (UP_EXPORTED_DEVICE (device)));
			return;
		}

		/* Ensure we poll the new device if needed */
		g_source_set_ready_time (daemon->priv->poll_source, 0);

		g_debug ("emitting added: %s", object_path);
		up_daemon_update_warning_level (daemon);
		up_exported_daemon_emit_device_added (UP_EXPORTED_DAEMON (daemon), object_path);
	} else {
		/*leds*/
		g_debug ("Add a led device to the device list");
		/* emit */
		object_path = up_device_kbd_backlight_get_object_path (UP_DEVICE_KBD_BACKLIGHT (device));
		if (object_path == NULL) {
			g_debug ("Device %s was unregistered before it was on the bus",
				 up_exported_kbd_backlight_get_native_path (UP_EXPORTED_KBD_BACKLIGHT (device)));
			return;
		}
		up_device_list_insert (priv->kbd_backlight_devices, G_OBJECT (device));
		up_daemon_kbd_backlight_composite_update (daemon, FALSE);
		up_exported_daemon_emit_device_added (UP_EXPORTED_DAEMON (daemon), object_path);
	}
}

/**
 * up_daemon_device_removed_cb:
 **/
static void
up_daemon_device_removed_cb (UpBackend *backend, GObject *device, UpDaemon *daemon)
{
	const gchar *object_path;
	UpDaemonPrivate *priv = daemon->priv;

	g_return_if_fail (UP_IS_DAEMON (daemon));
	g_return_if_fail (UP_IS_DEVICE (device) || UP_IS_DEVICE_KBD_BACKLIGHT (device));

	g_signal_handlers_disconnect_by_data (device, daemon);

	/* emit */
	if (UP_IS_DEVICE (device)) {
		/* remove from list (device remains valid during the function call) */
		up_device_list_remove (priv->power_devices, device);
		object_path = up_device_get_object_path (UP_DEVICE (device));
	} else if (UP_IS_DEVICE_KBD_BACKLIGHT (device)) {
		/* remove from list (device remains valid during the function call) */
		up_device_list_remove (priv->kbd_backlight_devices, device);
		object_path = up_device_kbd_backlight_get_object_path (UP_DEVICE_KBD_BACKLIGHT (device));
	} else {
		return;
	}

	/* don't crash the session */
	if (object_path == NULL) {
		if (UP_IS_DEVICE (device))
			g_debug ("not emitting device-removed for unregistered device: %s",
				 up_exported_device_get_native_path (UP_EXPORTED_DEVICE (device)));
		else
			g_debug ("not emitting device-removed for unregistered kbd backlight: %s",
				 up_exported_kbd_backlight_get_native_path (UP_EXPORTED_KBD_BACKLIGHT (device)));
		return;
	}
	g_debug ("emitting device-removed: %s", object_path);
	up_exported_daemon_emit_device_removed (UP_EXPORTED_DAEMON (daemon), object_path);

	/* Unregister keyboard backlight dbus path */
	if (UP_IS_DEVICE_KBD_BACKLIGHT (device)) {
		up_device_kbd_backlight_unregister (UP_DEVICE_KBD_BACKLIGHT (device));
		up_daemon_kbd_backlight_composite_update (daemon, TRUE);
	}

	/* In case a battery was removed */
	up_daemon_refresh_battery_devices (daemon);
	up_daemon_update_warning_level (daemon);
}

#define LOAD_OR_DEFAULT(val, str, def) val = (load_default ? def : up_config_get_uint (daemon->priv->config, str))
#define LOAD_OR_DEFAULT_DOUBLE(val, str, def) val = (load_default ? def : up_config_get_double (daemon->priv->config, str))

static void
load_percentage_policy (UpDaemon    *daemon,
			gboolean     load_default)
{
	LOAD_OR_DEFAULT_DOUBLE (daemon->priv->low_percentage, "PercentageLow", 20.0);
	LOAD_OR_DEFAULT_DOUBLE (daemon->priv->critical_percentage, "PercentageCritical", 5.0);
	LOAD_OR_DEFAULT_DOUBLE (daemon->priv->action_percentage, "PercentageAction", 2.0);
}

static void
load_time_policy (UpDaemon    *daemon,
		  gboolean     load_default)
{
	LOAD_OR_DEFAULT (daemon->priv->low_time, "TimeLow", 1200);
	LOAD_OR_DEFAULT (daemon->priv->critical_time, "TimeCritical", 300);
	LOAD_OR_DEFAULT (daemon->priv->action_time, "TimeAction", 120);
}

#define IS_DESCENDING(x, y, z) (x > y && y > z)

static void
policy_config_validate (UpDaemon *daemon)
{
	if (daemon->priv->low_percentage >= 100.0 ||
	    daemon->priv->critical_percentage >= 100.0 ||
	    daemon->priv->action_percentage >= 100.0) {
		load_percentage_policy (daemon, TRUE);
	} else if (!IS_DESCENDING (daemon->priv->low_percentage,
				   daemon->priv->critical_percentage,
				   daemon->priv->action_percentage)) {
		load_percentage_policy (daemon, TRUE);
	}

	if (!IS_DESCENDING (daemon->priv->low_time,
			    daemon->priv->critical_time,
			    daemon->priv->action_time)) {
		load_time_policy (daemon, TRUE);
	}
}

/**
 * up_daemon_init:
 **/
static void
up_daemon_init (UpDaemon *daemon)
{
	daemon->priv = up_daemon_get_instance_private (daemon);

	daemon->priv->critical_action_lock_fd = -1;
	daemon->priv->polkit = up_polkit_new ();
	daemon->priv->config = up_config_new ();
	daemon->priv->power_devices = up_device_list_new ();
	daemon->priv->kbd_backlight_devices = up_device_list_new ();
	daemon->priv->display_device = up_device_new (daemon, NULL);
	daemon->priv->poll_source = g_source_new (&poll_source_funcs, sizeof (GSource));
	daemon->priv->display_kbd_backlight_device = up_device_kbd_backlight_new (daemon, NULL, daemon->priv->kbd_backlight_devices);

	g_source_set_callback (daemon->priv->poll_source, NULL, daemon, NULL);
	g_source_set_name (daemon->priv->poll_source, "up-device-poll");
	g_source_attach (daemon->priv->poll_source, NULL);
	/* g_source_destroy removes the last reference */
	g_source_unref (daemon->priv->poll_source);

	daemon->priv->use_percentage_for_policy = up_config_get_boolean (daemon->priv->config, "UsePercentageForPolicy");
	load_percentage_policy (daemon, FALSE);
	load_time_policy (daemon, FALSE);
	policy_config_validate (daemon);

	up_daemon_get_env_override (daemon);

	daemon->priv->backend = up_backend_new ();
	g_signal_connect (daemon->priv->backend, "device-added",
			  G_CALLBACK (up_daemon_device_added_cb), daemon);
	g_signal_connect (daemon->priv->backend, "device-removed",
			  G_CALLBACK (up_daemon_device_removed_cb), daemon);

	up_exported_daemon_set_daemon_version (UP_EXPORTED_DAEMON (daemon), PACKAGE_VERSION);

	g_signal_connect (daemon, "handle-enumerate-devices",
			  G_CALLBACK (up_daemon_enumerate_devices), daemon);
	g_signal_connect (daemon, "handle-enumerate-kbd_backlights",
				G_CALLBACK (up_daemon_enumerate_kbd_backlights), daemon);
	g_signal_connect (daemon, "handle-get-critical-action",
			  G_CALLBACK (up_daemon_get_critical_action), daemon);
	g_signal_connect (daemon, "handle-get-display-device",
			  G_CALLBACK (up_daemon_get_display_device), daemon);
}

static const GDBusErrorEntry up_daemon_error_entries[] = {
	{ UP_DAEMON_ERROR_GENERAL, UP_INTERFACE_PREFIX "GeneralError" },
	{ UP_DAEMON_ERROR_NOT_SUPPORTED, UP_INTERFACE_PREFIX "NotSupported" },
	{ UP_DAEMON_ERROR_NO_SUCH_DEVICE, UP_INTERFACE_PREFIX "NoSuchDevice" },
};

/**
 * up_daemon_error_quark:
 **/
GQuark
up_daemon_error_quark (void)
{
	static volatile gsize quark_volatile = 0;

	g_dbus_error_register_error_domain ("up_daemon_error",
					    &quark_volatile,
					    up_daemon_error_entries,
					    G_N_ELEMENTS (up_daemon_error_entries));
	return quark_volatile;
}

/**
 * up_daemon_class_init:
 **/
static void
up_daemon_class_init (UpDaemonClass *klass)
{
	GObjectClass *object_class = G_OBJECT_CLASS (klass);
	object_class->finalize = up_daemon_finalize;
}

/**
 * up_daemon_finalize:
 **/
static void
up_daemon_finalize (GObject *object)
{
	UpDaemon *daemon = UP_DAEMON (object);
	UpDaemonPrivate *priv = daemon->priv;

	g_clear_handle_id (&priv->action_timeout_id, g_source_remove);
	g_clear_handle_id (&priv->refresh_batteries_id, g_source_remove);
	g_clear_handle_id (&priv->warning_level_id, g_source_remove);

	if (priv->critical_action_lock_fd >= 0) {
		close (priv->critical_action_lock_fd);
		priv->critical_action_lock_fd = -1;
	}

	g_clear_pointer (&daemon->priv->poll_source, g_source_destroy);

	g_object_unref (priv->power_devices);
	g_object_unref (priv->display_kbd_backlight_device);
	g_object_unref (priv->kbd_backlight_devices);
	g_object_unref (priv->display_device);
	g_object_unref (priv->polkit);
	g_object_unref (priv->config);
	g_object_unref (priv->backend);

	G_OBJECT_CLASS (up_daemon_parent_class)->finalize (object);
}

/**
 * up_daemon_new:
 **/
UpDaemon *
up_daemon_new (void)
{
	return UP_DAEMON (g_object_new (UP_TYPE_DAEMON, NULL));
}
