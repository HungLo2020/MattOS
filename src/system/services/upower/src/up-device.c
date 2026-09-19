/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2008 David Zeuthen <davidz@redhat.com>
 * Copyright (C) 2008-2009 Richard Hughes <richard@hughsie.com>
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

#include <glib.h>
#include <glib/gstdio.h>
#include <glib/gi18n-lib.h>
#include <glib-object.h>

#include "up-native.h"
#include "up-device.h"
#include "up-history.h"
#include "up-history-item.h"
#include "up-stats-item.h"

typedef struct
{
	UpDaemon		*daemon;
	/* native == NULL implies display device */
	GObject			*native;

	UpHistory		*history;
	gboolean		 has_ever_refresh;

	gint64			last_refresh;
	int			poll_timeout;

	/* This is TRUE if the wireless_status property is present, and
	 * its value is "disconnected"
	 * See https://www.kernel.org/doc/html/latest/driver-api/usb/usb.html#c.usb_interface */
	gboolean		disconnected;
} UpDevicePrivate;

static void up_device_initable_iface_init (GInitableIface *iface);

G_DEFINE_TYPE_EXTENDED (UpDevice, up_device, UP_TYPE_EXPORTED_DEVICE_SKELETON, 0,
                        G_IMPLEMENT_INTERFACE (G_TYPE_INITABLE,
                                               up_device_initable_iface_init)
                        G_ADD_PRIVATE (UpDevice))

enum {
  PROP_0,
  PROP_DAEMON,
  PROP_NATIVE,
  PROP_LAST_REFRESH,
  PROP_POLL_TIMEOUT,
  PROP_DISCONNECTED,
  N_PROPS
};

static GParamSpec *properties[N_PROPS];

#define UP_DEVICES_DBUS_PATH "/org/freedesktop/UPower/devices"

static gchar * up_device_get_id (UpDevice *device);

/* This needs to be called when one of those properties changes:
 * state
 * power_supply
 * percentage
 * time_to_empty
 * battery_level
 *
 * type should not change for non-display devices
 */
static void
update_warning_level (UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	UpDeviceLevel warning_level, battery_level;
	UpExportedDevice *skeleton = UP_EXPORTED_DEVICE (device);

	if (priv->native == NULL)
		return;

	/* If the battery level is available, and is critical,
	 * we need to fallback to calculations to get the warning
	 * level, as that might be "action" at this point */
	battery_level = up_exported_device_get_battery_level (skeleton);
	if (battery_level != UP_DEVICE_LEVEL_NONE &&
	    battery_level != UP_DEVICE_LEVEL_CRITICAL) {
		if (battery_level == UP_DEVICE_LEVEL_LOW)
			warning_level = battery_level;
		else
			warning_level = UP_DEVICE_LEVEL_NONE;
	} else {
		warning_level = up_daemon_compute_warning_level (priv->daemon,
								 up_exported_device_get_state (skeleton),
								 up_exported_device_get_type_ (skeleton),
								 up_exported_device_get_power_supply (skeleton),
								 up_exported_device_get_percentage (skeleton),
								 up_exported_device_get_time_to_empty (skeleton));
	}

	up_exported_device_set_warning_level (skeleton, warning_level);
}

/* This needs to be called when one of those properties changes:
 * type
 * state
 * percentage
 * is-present
 */
static void
update_icon_name (UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	const gchar *icon_name = NULL;
	UpExportedDevice *skeleton = UP_EXPORTED_DEVICE (device);

	/* get the icon from some simple rules */
	if (up_exported_device_get_type_ (skeleton) == UP_DEVICE_KIND_LINE_POWER) {
		icon_name = "ac-adapter-symbolic";
	} else {

		if (!up_exported_device_get_is_present (skeleton)) {
			icon_name = "battery-missing-symbolic";

		} else {
			switch (up_exported_device_get_state (skeleton)) {
			case UP_DEVICE_STATE_EMPTY:
				icon_name = "battery-empty-symbolic";
				break;
			case UP_DEVICE_STATE_FULLY_CHARGED:
				icon_name = "battery-full-charged-symbolic";
				break;
			case UP_DEVICE_STATE_CHARGING:
			case UP_DEVICE_STATE_PENDING_CHARGE:
				icon_name = up_daemon_get_charge_icon (priv->daemon,
								       up_exported_device_get_percentage (skeleton),
								       up_exported_device_get_battery_level (skeleton),
								    TRUE);
				break;
			case UP_DEVICE_STATE_DISCHARGING:
			case UP_DEVICE_STATE_PENDING_DISCHARGE:
				icon_name = up_daemon_get_charge_icon (priv->daemon,
								       up_exported_device_get_percentage (skeleton),
								       up_exported_device_get_battery_level (skeleton),
								    FALSE);
				break;
			default:
				icon_name = "battery-missing-symbolic";
			}
		}
	}

	up_exported_device_set_icon_name (skeleton, icon_name);
}

static void
ensure_history (UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	g_autofree char *id = NULL;

	if (priv->history)
		return;

	priv->history = up_history_new ();
	id = up_device_get_id (device);
	if (id)
		up_history_set_id (priv->history, id);
}

static gboolean
up_device_history_filter (UpDevice *device)
{
	UpExportedDevice *skeleton = UP_EXPORTED_DEVICE (device);

	if (up_exported_device_get_state (skeleton) == UP_DEVICE_STATE_UNKNOWN) {
		g_debug ("device %s has unknown state, not saving history",
			   up_exported_device_get_native_path (skeleton));
		return FALSE;
	}
	return TRUE;
}

static void
update_history (UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	UpExportedDevice *skeleton = UP_EXPORTED_DEVICE (device);

	ensure_history (device);

	if (!up_device_history_filter (device))
		return;

	/* save new history */
	up_history_set_state (priv->history, up_exported_device_get_state (skeleton));
	up_history_set_charge_data (priv->history, up_exported_device_get_percentage (skeleton));
	up_history_set_rate_data (priv->history, up_exported_device_get_energy_rate (skeleton));
	up_history_set_time_full_data (priv->history, up_exported_device_get_time_to_full (skeleton));
	up_history_set_time_empty_data (priv->history, up_exported_device_get_time_to_empty (skeleton));
	up_history_set_voltage_data (priv->history, up_exported_device_get_voltage (skeleton));
}

static void
up_device_notify (GObject *object, GParamSpec *pspec)
{
	UpDevice *device = UP_DEVICE (object);
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	g_autofree gchar *id = NULL;

	/* Not finished setting up the object? */
	if (priv->daemon == NULL)
		return;

	G_OBJECT_CLASS (up_device_parent_class)->notify (object, pspec);

	id = up_device_get_id (device);

	if (g_strcmp0 (pspec->name, "type") == 0 ||
	    g_strcmp0 (pspec->name, "is-present") == 0) {
		update_icon_name (device);
		/* Clearing the history object for lazily loading when device id was changed. */
		if (priv->history != NULL &&
		    !up_history_is_device_id_equal (priv->history, id))
			g_clear_object (&priv->history);
	} else if (g_strcmp0 (pspec->name, "vendor") == 0 ||
		   g_strcmp0 (pspec->name, "model") == 0 ||
		   g_strcmp0 (pspec->name, "serial") == 0) {
		if (priv->history != NULL &&
		    !up_history_is_device_id_equal (priv->history, id))
			g_clear_object (&priv->history);
	} else if (g_strcmp0 (pspec->name, "power-supply") == 0 ||
		   g_strcmp0 (pspec->name, "time-to-empty") == 0) {
		update_warning_level (device);
	} else if (g_strcmp0 (pspec->name, "state") == 0 ||
		   g_strcmp0 (pspec->name, "percentage") == 0 ||
		   g_strcmp0 (pspec->name, "battery-level") == 0) {
		update_warning_level (device);
		update_icon_name (device);
	} else if (g_strcmp0 (pspec->name, "update-time") == 0) {
		update_history (device);
	}
}

/**
 * up_device_get_on_battery:
 *
 * Note: Only implement for system devices, i.e. ones supplying the system
 **/
gboolean
up_device_get_on_battery (UpDevice *device, gboolean *on_battery)
{
	UpDeviceClass *klass = UP_DEVICE_GET_CLASS (device);

	g_return_val_if_fail (UP_IS_DEVICE (device), FALSE);

	if (klass->get_on_battery == NULL)
		return FALSE;

	return klass->get_on_battery (device, on_battery);
}

/**
 * up_device_get_online:
 *
 * Note: Only implement for system devices, i.e. devices supplying the system
 **/
gboolean
up_device_get_online (UpDevice *device, gboolean *online)
{
	UpDeviceClass *klass = UP_DEVICE_GET_CLASS (device);

	g_return_val_if_fail (UP_IS_DEVICE (device), FALSE);

	if (klass->get_online == NULL)
		return FALSE;

	return klass->get_online (device, online);
}

/**
 * up_device_get_id:
 *
 * Return the id of a Device.
 * Note: The caller of the method takes ownership of the returned data, and is
 * responsible for freeing it.
 **/
static gchar *
up_device_get_id (UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	GString *string;
	gchar *id = NULL;
	const char *model;
	const char *serial;
	UpExportedDevice *skeleton;

	if (priv->native == NULL)
		return NULL;

	skeleton = UP_EXPORTED_DEVICE (device);
	model = up_exported_device_get_model (skeleton);
	serial = up_exported_device_get_serial (skeleton);

	if (up_exported_device_get_type_ (skeleton) == UP_DEVICE_KIND_LINE_POWER) {
		goto out;

	} else if (up_exported_device_get_type_ (skeleton) == UP_DEVICE_KIND_BATTERY) {
		/* we don't have an ID if we are not present */
		if (!up_exported_device_get_is_present (skeleton))
			goto out;

		string = g_string_new ("");

		/* in an ideal world, model-capacity-serial */
		if (model != NULL && strlen (model) > 2) {
			g_string_append (string, model);
			g_string_append_c (string, '-');
		}
		if (up_exported_device_get_energy_full_design (skeleton) > 0) {
			/* FIXME: this may not be stable if we are using voltage_now */
			g_string_append_printf (string, "%i", (guint) up_exported_device_get_energy_full_design (skeleton));
			g_string_append_c (string, '-');
		}
		if (serial != NULL && strlen (serial) > 2) {
			g_string_append (string, serial);
			g_string_append_c (string, '-');
		}

		/* make sure we are sane */
		if (string->len == 0) {
			/* just use something generic */
			g_string_append (string, "generic_id");
		} else {
			/* remove trailing '-' */
			g_string_set_size (string, string->len - 1);
		}

		id = g_string_free (string, FALSE);

	} else {
		/* generic fallback, get what data we can */
		string = g_string_new ("");
		if (up_exported_device_get_vendor (skeleton) != NULL) {
			g_string_append (string, up_exported_device_get_vendor (skeleton));
			g_string_append_c (string, '-');
		}
		if (model != NULL) {
			g_string_append (string, model);
			g_string_append_c (string, '-');
		}
		if (serial != NULL) {
			g_string_append (string, serial);
			g_string_append_c (string, '-');
		}

		/* make sure we are sane */
		if (string->len == 0) {
			/* just use something generic */
			g_string_append (string, "generic_id");
		} else {
			/* remove trailing '-' */
			g_string_set_size (string, string->len - 1);
		}

		id = g_string_free (string, FALSE);
	}

	/* the id may have invalid chars that need to be replaced */
	g_strdelimit (id, "\\\t\"?' /,.", '_');

out:
	return id;
}

/**
 * up_device_get_daemon:
 *
 * Returns a refcounted #UpDaemon instance, or %NULL
 **/
UpDaemon *
up_device_get_daemon (UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);

	if (priv->daemon == NULL)
		return NULL;
	return g_object_ref (priv->daemon);
}

/**
 * up_device_polkit_is_allowed
 **/
gboolean
up_device_polkit_is_allowed (UpDevice *device, GDBusMethodInvocation *invocation)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);

	if (!up_daemon_polkit_is_allowed (priv->daemon,
					  "org.freedesktop.UPower.enable-charging-limit",
					  invocation))
		return FALSE;

	return TRUE;
}

static void
up_device_export_skeleton (UpDevice *device,
			   const gchar *object_path)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	GError *error = NULL;

	g_dbus_interface_skeleton_export (G_DBUS_INTERFACE_SKELETON (device),
					  g_dbus_interface_skeleton_get_connection (G_DBUS_INTERFACE_SKELETON (priv->daemon)),
					  object_path,
					  &error);

	if (error != NULL) {
		g_critical ("error registering device on system bus: %s", error->message);
		g_error_free (error);
	}
}

static gchar *
up_device_compute_object_path (UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	gchar *basename;
	gchar *id;
	gchar *object_path;
	const gchar *native_path;
	const gchar *type;
	guint i;

	if (priv->native == NULL) {
		return g_build_filename (UP_DEVICES_DBUS_PATH, "DisplayDevice", NULL);
	}

	type = up_device_kind_to_string (up_exported_device_get_type_ (UP_EXPORTED_DEVICE (device)));
	native_path = up_exported_device_get_native_path (UP_EXPORTED_DEVICE (device));
	basename = g_path_get_basename (native_path);
	id = g_strjoin ("_", type, basename, NULL);

	/* make DBUS valid path */
	for (i=0; id[i] != '\0'; i++) {
		if (id[i] == '-')
			id[i] = '_';
		if (id[i] == '.')
			id[i] = 'x';
		if (id[i] == ':')
			id[i] = 'o';
		if (id[i] == '@')
			id[i] = '_';
	}
	object_path = g_build_filename (UP_DEVICES_DBUS_PATH, id, NULL);

	g_free (basename);
	g_free (id);

	return object_path;
}

gboolean
up_device_register (UpDevice *device)
{
	g_autofree char *computed_object_path = NULL;

	if (g_dbus_interface_skeleton_get_object_path (G_DBUS_INTERFACE_SKELETON (device)) != NULL)
		return FALSE;
	computed_object_path = up_device_compute_object_path (device);
	g_debug ("Exported UpDevice with path %s", computed_object_path);
	up_device_export_skeleton (device, computed_object_path);
	return TRUE;
}

void
up_device_unregister (UpDevice *device)
{
	g_autofree char *object_path = NULL;

	object_path = g_strdup (g_dbus_interface_skeleton_get_object_path (G_DBUS_INTERFACE_SKELETON (device)));
	if (object_path != NULL) {
		g_dbus_interface_skeleton_unexport (G_DBUS_INTERFACE_SKELETON (device));
		g_debug ("Unexported UpDevice with path %s", object_path);
	}
}

gboolean
up_device_is_registered (UpDevice *device)
{
	return g_dbus_interface_skeleton_get_object_path (G_DBUS_INTERFACE_SKELETON (device)) != NULL;
}

/**
 * up_device_refresh:
 *
 * Return %TRUE on success, %FALSE if we failed to refresh or no data
 **/
static gboolean
up_device_refresh (UpExportedDevice *skeleton,
		   GDBusMethodInvocation *invocation,
		   UpDevice *device)
{
	up_device_refresh_internal (device, UP_REFRESH_POLL);
	up_exported_device_complete_refresh (skeleton, invocation);
	return TRUE;
}

static gboolean
up_device_initable_init (GInitable     *initable,
                         GCancellable  *cancellable,
                         GError       **error)
{
	UpDevice *device = UP_DEVICE (initable);
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	const gchar *native_path = "DisplayDevice";
	UpDeviceClass *klass = UP_DEVICE_GET_CLASS (device);
	int ret;

	g_return_val_if_fail (UP_IS_DEVICE (device), FALSE);

	if (up_daemon_get_debug (priv->daemon))
		g_signal_connect (device, "handle-refresh",
				  G_CALLBACK (up_device_refresh), device);
	if (priv->native) {
		native_path = up_native_get_native_path (priv->native);
		up_exported_device_set_native_path (UP_EXPORTED_DEVICE (device), native_path);
	}

	/* coldplug source */
	if (klass->coldplug != NULL) {
		ret = klass->coldplug (device);
		if (!ret) {
			g_debug ("failed to coldplug %s", native_path);
			g_propagate_error (error, g_error_new (G_IO_ERROR, G_IO_ERROR_FAILED,
			                                       "Failed to coldplug %s", native_path));

			return FALSE;
		}
	}

	/* force a refresh, although failure isn't fatal */
	ret = up_device_refresh_internal (device, UP_REFRESH_INIT);
	if (!ret) {
		g_debug ("failed to refresh %s", native_path);

		/* XXX: We do not store a history if the initial refresh failed.
		 * This doesn't seem sensible, but it was the case historically. */
		goto register_device;
	}

register_device:
	/* put on the bus */
	up_device_register (device);

	return TRUE;
}

static void
up_device_initable_iface_init (GInitableIface *iface)
{
  iface->init = up_device_initable_init;
}

static gboolean
up_device_get_statistics (UpExportedDevice *skeleton,
			  GDBusMethodInvocation *invocation,
			  const gchar *type,
			  UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	GPtrArray *array = NULL;
	UpStatsItem *item;
	guint i;
	GVariantBuilder builder;

	if (!up_exported_device_get_has_statistics (skeleton)) {
		g_dbus_method_invocation_return_error_literal (invocation,
							       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
							       "device does not support getting stats");
		goto out;
	}

	ensure_history (device);

	/* get the correct data */
	if (g_strcmp0 (type, "charging") == 0)
		array = up_history_get_profile_data (priv->history, TRUE);
	else if (g_strcmp0 (type, "discharging") == 0)
		array = up_history_get_profile_data (priv->history, FALSE);

	/* maybe the device doesn't support histories */
	if (array == NULL) {
		g_dbus_method_invocation_return_error_literal (invocation,
							       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
							       "device has no statistics");
		goto out;
	}

	/* always 101 items of data */
	if (array->len != 101) {
		g_dbus_method_invocation_return_error (invocation,
						       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
						       "statistics invalid as have %i items", array->len);
		goto out;
	}

	/* copy data to dbus struct */
	g_variant_builder_init (&builder, G_VARIANT_TYPE ("a(dd)"));
	for (i = 0; i < array->len; i++) {
		item = (UpStatsItem *) g_ptr_array_index (array, i);
		g_variant_builder_add (&builder, "(dd)",
				       up_stats_item_get_value (item),
				       up_stats_item_get_accuracy (item));
	}

	up_exported_device_complete_get_statistics (skeleton, invocation,
						    g_variant_builder_end (&builder));
out:
	if (array != NULL)
		g_ptr_array_unref (array);
	return TRUE;
}

static gboolean
up_device_get_history (UpExportedDevice *skeleton,
		       GDBusMethodInvocation *invocation,
		       const gchar *type_string,
		       guint timespan,
		       guint resolution,
		       UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	GPtrArray *array = NULL;
	UpHistoryItem *item;
	guint i;
	UpHistoryType type = UP_HISTORY_TYPE_UNKNOWN;
	GVariantBuilder builder;

	/* doesn't even try to support this */
	if (!up_exported_device_get_has_history (skeleton)) {
		g_dbus_method_invocation_return_error_literal (invocation,
							       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
							       "device does not support getting history");
		goto out;
	}

	/* get the correct data */
	if (g_strcmp0 (type_string, "rate") == 0)
		type = UP_HISTORY_TYPE_RATE;
	else if (g_strcmp0 (type_string, "charge") == 0)
		type = UP_HISTORY_TYPE_CHARGE;
	else if (g_strcmp0 (type_string, "time-full") == 0)
		type = UP_HISTORY_TYPE_TIME_FULL;
	else if (g_strcmp0 (type_string, "time-empty") == 0)
		type = UP_HISTORY_TYPE_TIME_EMPTY;
	else if (g_strcmp0 (type_string, "voltage") == 0)
		type = UP_HISTORY_TYPE_VOLTAGE;

	/* something recognized */
	if (type != UP_HISTORY_TYPE_UNKNOWN) {
		ensure_history (device);
		array = up_history_get_data (priv->history, type, timespan, resolution);
	}

	/* maybe the device doesn't have any history */
	if (array == NULL) {
		g_dbus_method_invocation_return_error_literal (invocation,
							       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
							       "device has no history");
		goto out;
	}

	/* copy data to dbus struct */
	g_variant_builder_init (&builder, G_VARIANT_TYPE ("a(udu)"));
	for (i = 0; i < array->len; i++) {
		item = (UpHistoryItem *) g_ptr_array_index (array, i);
		g_variant_builder_add (&builder, "(udu)",
				       up_history_item_get_time (item),
				       up_history_item_get_value (item),
				       up_history_item_get_state (item));
	}

	up_exported_device_complete_get_history (skeleton, invocation,
						 g_variant_builder_end (&builder));

out:
	if (array != NULL)
		g_ptr_array_unref (array);
	return TRUE;
}

void
up_device_sibling_discovered (UpDevice *device, GObject *sibling)
{
	UpDeviceClass *klass = UP_DEVICE_GET_CLASS (device);

	if (klass->sibling_discovered)
		klass->sibling_discovered (device, sibling);
}

gboolean
up_device_refresh_internal (UpDevice *device, UpRefreshReason reason)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	gboolean ret = FALSE;
	UpDeviceClass *klass = UP_DEVICE_GET_CLASS (device);

	if (priv->native == NULL)
		return TRUE;

	/* not implemented */
	if (klass->refresh == NULL)
		goto out;

	/* do the refresh, and change the property */
	ret = klass->refresh (device, reason);
	priv->last_refresh = g_get_monotonic_time ();
	g_object_notify_by_pspec (G_OBJECT (device), properties[PROP_LAST_REFRESH]);

	if (!ret) {
		g_debug ("no changes");
		goto out;
	}

	/* the first time, print all properties */
	if (!priv->has_ever_refresh) {
		g_debug ("added native-path: %s", up_exported_device_get_native_path (UP_EXPORTED_DEVICE (device)));
		priv->has_ever_refresh = TRUE;
		goto out;
	}
out:
	return ret;
}

const gchar *
up_device_get_object_path (UpDevice *device)
{
	g_return_val_if_fail (UP_IS_DEVICE (device), NULL);
	return g_dbus_interface_skeleton_get_object_path (G_DBUS_INTERFACE_SKELETON (device));
}

GObject *
up_device_get_native (UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);
	g_return_val_if_fail (UP_IS_DEVICE (device), NULL);
	return priv->native;
}

const gchar *
up_device_get_state_dir_override (UpDevice *device)
{
	UpDevicePrivate *priv = up_device_get_instance_private (device);

	if (priv->daemon == NULL)
		return NULL;

	return up_daemon_get_state_dir_env_override (priv->daemon);
}

static void
up_device_init (UpDevice *device)
{
	UpExportedDevice *skeleton;

	skeleton = UP_EXPORTED_DEVICE (device);
	up_exported_device_set_battery_level (skeleton, UP_DEVICE_LEVEL_NONE);

	g_signal_connect (device, "handle-get-history",
			  G_CALLBACK (up_device_get_history), device);
	g_signal_connect (device, "handle-get-statistics",
			  G_CALLBACK (up_device_get_statistics), device);
}

static void
up_device_finalize (GObject *object)
{
	UpDevicePrivate *priv = up_device_get_instance_private (UP_DEVICE (object));

	g_clear_object (&priv->native);
	g_clear_object (&priv->daemon);
	g_clear_object (&priv->history);

	G_OBJECT_CLASS (up_device_parent_class)->finalize (object);
}

static void
up_device_dispose (GObject *object)
{
	UpDevicePrivate *priv = up_device_get_instance_private (UP_DEVICE (object));

	g_clear_object (&priv->daemon);

	G_OBJECT_CLASS (up_device_parent_class)->dispose (object);
}

static void
up_device_set_property (GObject      *object,
                        guint         prop_id,
                        const GValue *value,
                        GParamSpec   *pspec)
{
	UpDevice *device = UP_DEVICE (object);
	UpDevicePrivate *priv = up_device_get_instance_private (device);

	switch (prop_id)
	{
	case PROP_DAEMON:
		priv->daemon = g_value_dup_object (value);
		break;

	case PROP_NATIVE:
		priv->native = g_value_dup_object (value);
		break;

	case PROP_POLL_TIMEOUT:
		priv->poll_timeout = g_value_get_int (value);
		break;

	case PROP_DISCONNECTED:
		priv->disconnected = g_value_get_boolean (value);
		break;

	default:
		G_OBJECT_WARN_INVALID_PROPERTY_ID (object, prop_id, pspec);
	}
}

static void
up_device_get_property (GObject      *object,
                        guint         prop_id,
                        GValue       *value,
                        GParamSpec   *pspec)
{
	UpDevice *device = UP_DEVICE (object);
	UpDevicePrivate *priv = up_device_get_instance_private (device);

	switch (prop_id)
	{
	case PROP_POLL_TIMEOUT:
		g_value_set_int (value, priv->poll_timeout);
		break;

	case PROP_LAST_REFRESH:
		g_value_set_int64 (value, priv->last_refresh);
		break;

	case PROP_DISCONNECTED:
		g_value_set_boolean (value, priv->disconnected);
		break;

	default:
		G_OBJECT_WARN_INVALID_PROPERTY_ID (object, prop_id, pspec);
	}
}

static void
up_device_class_init (UpDeviceClass *klass)
{
	GObjectClass *object_class = G_OBJECT_CLASS (klass);

	object_class->notify = up_device_notify;
	object_class->finalize = up_device_finalize;
	object_class->dispose = up_device_dispose;

	object_class->set_property = up_device_set_property;
	object_class->get_property = up_device_get_property;

	properties[PROP_DAEMON] =
		g_param_spec_object ("daemon",
		                     "UpDaemon",
		                     "UpDaemon reference",
		                     UP_TYPE_DAEMON,
		                     G_PARAM_STATIC_STRINGS | G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY);

	properties[PROP_NATIVE] =
		g_param_spec_object ("native",
		                     "Native",
		                     "Native Object",
		                     G_TYPE_OBJECT,
		                     G_PARAM_STATIC_STRINGS | G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY);

	properties[PROP_POLL_TIMEOUT] =
		g_param_spec_int ("poll-timeout",
		                  "Poll timeout",
		                  "Time in seconds between polls",
		                  0,
		                  3600,
		                  0,
		                  G_PARAM_STATIC_STRINGS | G_PARAM_WRITABLE | G_PARAM_READABLE);

	properties[PROP_LAST_REFRESH] =
		g_param_spec_int64 ("last-refresh",
		                    "Last Refresh",
		                    "Time of last refresh (in monotonic clock)",
		                    -1,
		                    G_MAXINT64,
		                    0,
		                    G_PARAM_STATIC_STRINGS | G_PARAM_READABLE);

	properties[PROP_DISCONNECTED] =
		g_param_spec_boolean ("disconnected",
		                      "Disconnected",
		                      "Whether wireless device is disconnected",
		                      FALSE,
		                      G_PARAM_STATIC_STRINGS | G_PARAM_WRITABLE | G_PARAM_READABLE);

	g_object_class_install_properties (object_class, N_PROPS, properties);
}

UpDevice *
up_device_new (UpDaemon	*daemon,
               GObject	*native)
{
	return UP_DEVICE (g_object_new (UP_TYPE_DEVICE,
	                                "daemon", daemon,
	                                "native", native,
	                                NULL));
}
