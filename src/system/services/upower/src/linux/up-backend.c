/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2010 Richard Hughes <richard@hughsie.com>
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

#include <stdlib.h>
#include <stdio.h>
#include <math.h>
#include <sys/wait.h>
#include <glib/gi18n.h>
#include <gio/gio.h>
#include <gio/gunixfdlist.h>
#include <gudev/gudev.h>

#include "up-backend.h"
#include "up-daemon.h"
#include "up-device.h"
#include "up-device-kbd-backlight.h"

#include "up-enumerator-udev.h"

#include "up-device-supply.h"
#include "up-device-wup.h"
#include "up-device-hid.h"
#include "up-device-bluez.h"
#include "up-input.h"
#include "up-config.h"
#ifdef HAVE_IDEVICE
#include "up-device-idevice.h"
#endif /* HAVE_IDEVICE */

static void	up_backend_class_init	(UpBackendClass	*klass);
static void	up_backend_init	(UpBackend		*backend);
static void	up_backend_finalize	(GObject		*object);

#define LOGIND_DBUS_NAME                       "org.freedesktop.login1"
#define LOGIND_DBUS_PATH                       "/org/freedesktop/login1"
#define LOGIND_DBUS_INTERFACE                  "org.freedesktop.login1.Manager"

struct UpBackendPrivate
{
	UpDaemon		*daemon;
	UpDeviceList		*device_list;
	GUdevClient		*gudev_client;
	UpInput			*lid_device;
	UpConfig		*config;
	GDBusProxy		*logind_proxy;
	guint                    logind_sleep_id;
	int                      logind_delay_inhibitor_fd;

	UpEnumerator		*udev_enum;

	/* BlueZ */
	guint			 bluez_watch_id;
	GDBusObjectManager	*bluez_client;
};

enum {
	SIGNAL_DEVICE_ADDED,
	SIGNAL_DEVICE_REMOVED,
	SIGNAL_LAST
};

static guint signals [SIGNAL_LAST] = { 0 };

G_DEFINE_TYPE_WITH_PRIVATE (UpBackend, up_backend, G_TYPE_OBJECT)

static void
input_switch_changed_cb (UpInput   *input,
			 gboolean   switch_value,
			 UpBackend *backend)
{
	up_daemon_set_lid_is_closed (backend->priv->daemon, switch_value);
}

static void
up_backend_uevent_signal_handler_cb (GUdevClient *client, const gchar *action,
				      GUdevDevice *device, gpointer user_data)
{
	UpBackend *backend = UP_BACKEND (user_data);
	g_autoptr(UpInput) input = NULL;

	if (backend->priv->lid_device)
		return;

	if (g_strcmp0 (action, "add") != 0)
		return;

	/* check if the input device is a lid */
	input = up_input_new ();
	if (up_input_coldplug (input, device)) {
		up_daemon_set_lid_is_present (backend->priv->daemon, TRUE);
		g_signal_connect (G_OBJECT (input), "switch-changed",
				  G_CALLBACK (input_switch_changed_cb), backend);
		up_daemon_set_lid_is_closed (backend->priv->daemon,
					     up_input_get_switch_value (input));

		backend->priv->lid_device = g_steal_pointer (&input);
	}
}

/**
 * batteries_share_parent:
 * @device: a %UpDevice
 * @other: another %UpDevice
 *
 * Batteries of one device share a parent. One battery reached over two
 * transports has a power supply each, but under different parents.
 *
 * Returns: TRUE if the batteries share one parent, FALSE otherwise.
 **/
static gboolean
batteries_share_parent (UpDevice *device,
			UpDevice *other)
{
	GUdevDevice *native, *other_native;
	g_autoptr(GUdevDevice) parent = NULL;
	g_autoptr(GUdevDevice) other_parent = NULL;

	if (!UP_IS_DEVICE_SUPPLY (device) || !UP_IS_DEVICE_SUPPLY (other))
		return FALSE;

	native = G_UDEV_DEVICE (up_device_get_native (device));
	other_native = G_UDEV_DEVICE (up_device_get_native (other));
	if (native == NULL || other_native == NULL)
		return FALSE;

	parent = g_udev_device_get_parent (native);
	other_parent = g_udev_device_get_parent (other_native);
	if (parent == NULL || other_parent == NULL)
		return FALSE;

	return g_strcmp0 (g_udev_device_get_sysfs_path (parent),
			  g_udev_device_get_sysfs_path (other_parent)) == 0;
}

static UpDevice *
find_duplicate_device (UpBackend *backend,
		       UpDevice  *device)
{
	GPtrArray *array;
	g_autofree char *serial = NULL;
	UpDevice *ret = NULL;
	guint i;

	g_object_get (G_OBJECT (device), "serial", &serial, NULL);
	if (!serial)
		return NULL;

	array = up_device_list_get_array (backend->priv->device_list);
	for (i = 0; i < array->len; i++) {
		g_autofree char *s = NULL;
		UpDevice *d;

		d = UP_DEVICE (g_ptr_array_index (array, i));
		if (d == device)
			continue;
		if (batteries_share_parent (device, d))
			continue;
		g_object_get (G_OBJECT (d), "serial", &s, NULL);
		if (s && g_ascii_strcasecmp (s, serial) == 0) {
			ret = g_object_ref (d);
			break;
		}
	}
	g_ptr_array_unref (array);

	return ret;
}

/* Returns TRUE if the added_device should be visible */
static gboolean
update_added_duplicate_device (UpBackend *backend,
			       UpDevice  *added_device)
{
	g_autoptr(UpDevice) other_device = NULL;
	UpDevice *bluez_device = NULL;
	UpDevice *unreg_device = NULL;
	g_autofree char *serial = NULL;

	other_device = find_duplicate_device (backend, added_device);
	if (!other_device)
		return TRUE;

	if (UP_IS_DEVICE_BLUEZ (added_device))
		bluez_device = added_device;
	else if (UP_IS_DEVICE_BLUEZ (other_device))
		bluez_device = other_device;

	if (bluez_device) {
		UpDevice *non_bluez_device;

		non_bluez_device = bluez_device == added_device ?
			other_device : added_device;
		g_object_bind_property (bluez_device, "model",
					non_bluez_device, "model",
					G_BINDING_SYNC_CREATE);
		unreg_device = bluez_device;
	} else {
		UpDeviceState state;
		UpDevice *tested_device;

		tested_device = added_device;
		g_object_get (G_OBJECT (tested_device), "state", &state, NULL);
		if (state != UP_DEVICE_STATE_UNKNOWN) {
			tested_device = other_device;
			g_object_get (G_OBJECT (tested_device), "state", &state, NULL);
		}
		if (state != UP_DEVICE_STATE_UNKNOWN) {
			g_object_get (G_OBJECT (added_device), "serial", &serial, NULL);
			g_debug ("Device %s is a duplicate, but we don't know if most interesting",
				 serial);
			return TRUE;
		}

		unreg_device = tested_device;
	}

	g_object_get (G_OBJECT (unreg_device), "serial", &serial, NULL);
	if (up_device_is_registered (unreg_device)) {
		g_signal_emit (backend, signals[SIGNAL_DEVICE_REMOVED], 0, unreg_device);
		up_device_unregister (unreg_device);
	}
	g_debug ("Hiding duplicate device %s", serial);
	return unreg_device != added_device;
}

static void
update_removed_duplicate_device (UpBackend *backend,
				 UpDevice  *removed_device)
{
	g_autoptr(UpDevice) other_device = NULL;

	other_device = find_duplicate_device (backend, removed_device);
	if (!other_device)
		return;

	/* Re-add the old duplicate device that got hidden */
	if (up_device_register (other_device))
		g_signal_emit (backend, signals[SIGNAL_DEVICE_ADDED], 0, other_device);
}

static gboolean
is_interesting_iface_proxy (GDBusProxy *interface_proxy)
{
	const char *iface;

	iface = g_dbus_proxy_get_interface_name (interface_proxy);
	return g_str_equal (iface, "org.bluez.Battery1") ||
		g_str_equal (iface, "org.bluez.Device1");
}

static gboolean
has_battery_iface (GDBusObject *object)
{
	GDBusInterface *iface;

	iface = g_dbus_object_get_interface (object, "org.bluez.Battery1");
	if (!iface)
		return FALSE;
	g_object_unref (iface);
	return TRUE;
}

static void
bluez_proxies_changed (GDBusObjectManagerClient *manager,
		       GDBusObjectProxy         *object_proxy,
		       GDBusProxy               *interface_proxy,
		       GVariant                 *changed_properties,
		       GStrv                     invalidated_properties,
		       gpointer                  user_data)
{
	UpBackend *backend = user_data;
	GObject *object;
	UpDeviceBluez *bluez;

	if (!is_interesting_iface_proxy (interface_proxy))
		return;

	object = up_device_list_lookup (backend->priv->device_list, G_OBJECT (object_proxy));
	if (!object)
		return;

	bluez = UP_DEVICE_BLUEZ (object);
	up_device_bluez_update (bluez, changed_properties);
	g_object_unref (object);
}

static void
bluez_interface_removed (GDBusObjectManager *manager,
			 GDBusObject        *bus_object,
			 GDBusInterface     *interface,
			 gpointer            user_data)
{
	UpBackend *backend = user_data;
	GObject *object;

	/* It might be another iface on another device that got removed */
	if (has_battery_iface (bus_object))
		return;

	object = up_device_list_lookup (backend->priv->device_list, G_OBJECT (bus_object));
	if (!object)
		return;

	g_debug ("emitting device-removed: %s", g_dbus_object_get_object_path (bus_object));
	if (up_device_is_registered (UP_DEVICE (object)))
		g_signal_emit (backend, signals[SIGNAL_DEVICE_REMOVED], 0, UP_DEVICE (object));

	g_object_unref (object);
}

static void
bluez_interface_added (GDBusObjectManager *manager,
		       GDBusObject        *bus_object,
		       GDBusInterface     *interface,
		       gpointer            user_data)
{
	g_autoptr(UpDevice) device = NULL;
	UpBackend *backend = user_data;
	GObject *object;

	if (!has_battery_iface (bus_object))
		return;

	object = up_device_list_lookup (backend->priv->device_list, G_OBJECT (bus_object));
	if (object != NULL) {
		g_object_unref (object);
		return;
	}

	device = g_initable_new (UP_TYPE_DEVICE_BLUEZ, NULL, NULL,
	                         "daemon", backend->priv->daemon,
	                         "native", G_OBJECT (bus_object),
	                         NULL);
	if (device) {
		g_debug ("emitting device-added: %s", g_dbus_object_get_object_path (bus_object));
		if (update_added_duplicate_device (backend, device))
			g_signal_emit (backend, signals[SIGNAL_DEVICE_ADDED], 0, device);
	}
}

static void
bluez_appeared (GDBusConnection *connection,
		const gchar     *name,
		const gchar     *name_owner,
		gpointer         user_data)
{
	UpBackend *backend = user_data;
	GError *error = NULL;
	GList *objects, *l;

	g_assert (backend->priv->bluez_client == NULL);

	backend->priv->bluez_client = g_dbus_object_manager_client_new_for_bus_sync (G_BUS_TYPE_SYSTEM,
										     G_DBUS_OBJECT_MANAGER_CLIENT_FLAGS_DO_NOT_AUTO_START,
										     "org.bluez",
										     "/",
										     NULL, NULL, NULL,
										     NULL, &error);
	if (!backend->priv->bluez_client) {
		g_warning ("Failed to create object manager for BlueZ: %s",
			   error->message);
		g_error_free (error);
		return;
	}

	g_debug ("BlueZ appeared");

	g_signal_connect (backend->priv->bluez_client, "interface-proxy-properties-changed",
			  G_CALLBACK (bluez_proxies_changed), backend);
	g_signal_connect (backend->priv->bluez_client, "interface-removed",
			  G_CALLBACK (bluez_interface_removed), backend);
	g_signal_connect (backend->priv->bluez_client, "interface-added",
			  G_CALLBACK (bluez_interface_added), backend);

	objects = g_dbus_object_manager_get_objects (backend->priv->bluez_client);
	for (l = objects; l != NULL; l = l->next) {
		GDBusObject *object = l->data;
		GList *interfaces, *k;

		interfaces = g_dbus_object_get_interfaces (object);

		for (k = interfaces; k != NULL; k = k->next) {
			GDBusInterface *iface = k->data;

			bluez_interface_added (backend->priv->bluez_client,
					       object,
					       iface,
					       backend);
			g_object_unref (iface);
		}
		g_list_free (interfaces);
		g_object_unref (object);
	}
	g_list_free (objects);
}

static void
bluez_vanished (GDBusConnection *connection,
		const gchar     *name,
		gpointer         user_data)
{
	UpBackend *backend = user_data;
	GPtrArray *array;
	guint i;

	g_debug ("BlueZ disappeared");

	array = up_device_list_get_array (backend->priv->device_list);

	for (i = 0; i < array->len; i++) {
		UpDevice *device = UP_DEVICE (g_ptr_array_index (array, i));
		if (UP_IS_DEVICE_BLUEZ (device)) {
			GDBusObject *object;

			object = G_DBUS_OBJECT (up_device_get_native (device));
			g_debug ("emitting device-removed: %s", g_dbus_object_get_object_path (object));
			if (up_device_is_registered (device))
				g_signal_emit (backend, signals[SIGNAL_DEVICE_REMOVED], 0, device);
		}
	}

	g_ptr_array_unref (array);

	g_clear_object (&backend->priv->bluez_client);
}

static void
up_device_disconnected_cb (GObject    *gobject,
			   GParamSpec *pspec,
			   gpointer    user_data)
{
	UpBackend *backend = user_data;
	g_autofree char *path = NULL;
	gboolean disconnected;

	g_object_get (gobject,
		      "native-path", &path,
		      "disconnected", &disconnected,
		      NULL);
	if (disconnected) {
		g_debug("Device %s became disconnected, hiding device", path);
		if (UP_IS_DEVICE (gobject)) {
			if (up_device_is_registered (UP_DEVICE (gobject))) {
				g_signal_emit (backend, signals[SIGNAL_DEVICE_REMOVED], 0, gobject);
				up_device_unregister (UP_DEVICE (gobject));
			}
		} else if (UP_IS_DEVICE_KBD_BACKLIGHT (gobject)) {
			up_device_kbd_backlight_unregister (UP_DEVICE_KBD_BACKLIGHT (gobject));
		}
	} else {
		g_debug ("Device %s became connected, showing device", path);
		if (up_device_register (UP_DEVICE (gobject)))
			g_signal_emit (backend, signals[SIGNAL_DEVICE_ADDED], 0, gobject);
	}
}

static void
udev_device_added_cb (UpBackend *backend, GObject *device)
{
	g_debug ("Got new device from udev enumerator: %p", device);
	g_signal_connect (device, "notify::disconnected",
			  G_CALLBACK (up_device_disconnected_cb), backend);
	if (UP_IS_DEVICE (device)) {
		if (update_added_duplicate_device (backend, UP_DEVICE (device)))
			g_signal_emit (backend, signals[SIGNAL_DEVICE_ADDED], 0, device);
	} else if (UP_IS_DEVICE_KBD_BACKLIGHT (device)) {
		g_signal_emit (backend, signals[SIGNAL_DEVICE_ADDED], 0, device);
	} else {
		g_warning ("Unknown device type");
	}
}

static void
udev_device_removed_cb (UpBackend *backend, GObject *device)
{
	g_debug ("Removing device from udev enumerator: %p", device);

	if (UP_IS_DEVICE (device))
		update_removed_duplicate_device (backend, UP_DEVICE (device));
	g_signal_emit (backend, signals[SIGNAL_DEVICE_REMOVED], 0, device);
}

/**
 * up_backend_coldplug:
 * @backend: The %UpBackend class instance
 * @daemon: The %UpDaemon controlling instance
 *
 * Finds all the devices already plugged in, and emits device-add signals for
 * each of them.
 *
 * Return value: %TRUE for success
 **/
gboolean
up_backend_coldplug (UpBackend *backend, UpDaemon *daemon)
{
	g_autolist(GUdevDevice) devices = NULL;
	GList *l;

	backend->priv->daemon = g_object_ref (daemon);
	backend->priv->device_list = up_daemon_get_device_list (daemon);

	/* Watch udev for input devices to find the lid switch */
	backend->priv->gudev_client = g_udev_client_new ((const char *[]){ "input", NULL });
	g_signal_connect (backend->priv->gudev_client, "uevent",
			  G_CALLBACK (up_backend_uevent_signal_handler_cb), backend);

	/* add all subsystems */
	devices = g_udev_client_query_by_subsystem (backend->priv->gudev_client, "input");
	for (l = devices; l != NULL; l = l->next)
		up_backend_uevent_signal_handler_cb (backend->priv->gudev_client,
						     "add",
						     G_UDEV_DEVICE (l->data),
						     backend);

	backend->priv->bluez_watch_id = g_bus_watch_name (G_BUS_TYPE_SYSTEM,
							  "org.bluez",
							  G_BUS_NAME_WATCHER_FLAGS_NONE,
							  bluez_appeared,
							  bluez_vanished,
							  backend,
							  NULL);

	backend->priv->udev_enum = g_object_new (UP_TYPE_ENUMERATOR_UDEV,
						 "daemon", daemon,
						 NULL);

	g_signal_connect_swapped (backend->priv->udev_enum, "device-added",
				  G_CALLBACK (udev_device_added_cb), backend);
	g_signal_connect_swapped (backend->priv->udev_enum, "device-removed",
				  G_CALLBACK (udev_device_removed_cb), backend);

	g_assert (g_initable_init (G_INITABLE (backend->priv->udev_enum), NULL, NULL));

	return TRUE;
}

/**
 * up_backend_unplug:
 * @backend: The %UpBackend class instance
 *
 * Forget about all learned devices, effectively undoing up_backend_coldplug.
 * Resources are released without emitting signals.
 */
void
up_backend_unplug (UpBackend *backend)
{
	g_clear_object (&backend->priv->gudev_client);
	g_clear_object (&backend->priv->udev_enum);
	g_clear_object (&backend->priv->device_list);
	g_clear_object (&backend->priv->lid_device);
	g_clear_object (&backend->priv->daemon);
	if (backend->priv->bluez_watch_id > 0) {
		g_bus_unwatch_name (backend->priv->bluez_watch_id);
		backend->priv->bluez_watch_id = 0;
	}
	g_clear_object (&backend->priv->bluez_client);
}

static gboolean
check_action_result (GVariant *result)
{
	if (result) {
		const char *s;

		g_variant_get (result, "(&s)", &s);
		if (g_strcmp0 (s, "yes") == 0)
			return TRUE;
	}
	return FALSE;
}

/**
 * up_backend_get_critical_action:
 * @backend: The %UpBackend class instance
 *
 * Which action will be taken when %UP_DEVICE_LEVEL_ACTION
 * warning-level occurs.
 **/
const char *
up_backend_get_critical_action (UpBackend *backend)
{
	struct {
		const gchar *method;
		const gchar *can_method;
	} actions[] = {
		/*
		 * The fallback order is
		 * Suspend -> HybridSleep -> Hibernate -> PowerOff -> Sleep -> Ignore
		 */
		{ "Suspend", "CanSuspend" },
		{ "HybridSleep", "CanHybridSleep" },
		{ "Hibernate", "CanHibernate" },
		{ "PowerOff", "CanPowerOff" },
		{ "Sleep", "CanSleep"},
		{ "Ignore", NULL },
	};
	g_autofree gchar *action = NULL;
	gboolean can_risky = FALSE;
	guint i = 1;

	g_return_val_if_fail (backend->priv->logind_proxy != NULL, NULL);

	can_risky = up_config_get_boolean (backend->priv->config,
					   "AllowRiskyCriticalPowerAction");

	/* find the configured action first */
	action = up_config_get_string (backend->priv->config, "CriticalPowerAction");

	/* safeguard for the risky actions */
	if (!can_risky) {
		if (!g_strcmp0 (action, "Suspend") || !g_strcmp0 (action, "Ignore")) {
			g_free (action);
			action = g_strdup_printf ("HybridSleep");
		}
	}

	/* if "Auto", use "Sleep()" */
	if (!g_strcmp0 (action, "Auto")) {
		g_free (action);
		action = g_strdup_printf ("Sleep");
	}

	if (action != NULL) {
		for (i = 0; i < G_N_ELEMENTS (actions); i++)
			if (g_str_equal (actions[i].method, action))
				break;
		if (i >= G_N_ELEMENTS (actions))
			i = 1;
	}

	for (; i < G_N_ELEMENTS (actions); i++) {
		GVariant *result;

		if (actions[i].can_method) {
			gboolean action_available;

			/* Check whether we can use the method */
			result = g_dbus_proxy_call_sync (backend->priv->logind_proxy,
							 actions[i].can_method,
							 NULL,
							 G_DBUS_CALL_FLAGS_NONE,
							 -1, NULL, NULL);
			action_available = check_action_result (result);
			g_variant_unref (result);

			if (!action_available)
				continue;
		}

		return actions[i].method;
	}
	g_assert_not_reached ();
}

/**
 * up_backend_take_action:
 * @backend: The %UpBackend class instance
 *
 * Act upon the %UP_DEVICE_LEVEL_ACTION warning-level.
 **/
void
up_backend_take_action (UpBackend *backend)
{
	GVariant *res = NULL;
	g_autoptr (GError) error = NULL;
	g_autofree gchar *action = NULL;
	const char *method;

	method = up_backend_get_critical_action (backend);
	g_assert (method != NULL);

	/* Take action */
	g_debug ("About to call logind method %s", method);

	/* Do nothing if the action is set to "Ignore" */
	if (g_strcmp0 (method, "Ignore") == 0) {
		return;
	}

	if (!g_strcmp0 (method, "Sleep"))
		action = g_strdup ("Sleep");
	else
		action = g_strdup_printf ("%sWithFlags", method);

	/* flag 16 is SD_LOGIND_SKIP_INHIBITORS */
	res = g_dbus_proxy_call_sync (backend->priv->logind_proxy,
				      action,
				      g_variant_new ("(t)", 16),
				      G_DBUS_CALL_FLAGS_NONE,
				      -1,
				      NULL,
				      &error);

	/* if the new API doesn't work, use old one */
	if (error != NULL) {
		g_autofree gchar *action_old = NULL;
		g_debug ("The new power action API doesn't work, using old one.");

		if (!g_strcmp0 (method, "Sleep")) {
			/* Sleep() is not available, so PowerOff instead */
			g_debug ("Sleep() is not available, using PowerOff instead");
			action_old = g_strdup ("PowerOff");
		} else {
			action_old = g_strdup (method);
		}
		g_dbus_proxy_call (backend->priv->logind_proxy,
				   action_old,
				   g_variant_new ("(b)", FALSE),
				   G_DBUS_CALL_FLAGS_NONE,
				   -1,
				   NULL,
				   NULL,
				   NULL);
	}
	if (res != NULL)
		g_variant_unref (res);
}

/**
 * up_backend_inhibitor_lock_take:
 * @backend: The %UpBackend class instance
 * @reason: Why the inhibitor lock is taken
 * @mode: The mode of the lock ('delay' or 'block')
 *
 * Acquire a sleep inhibitor lock via systemd's logind that will
 * inhibit going to sleep until the lock is released again by
 * closing the file descriptor.
 */
int
up_backend_inhibitor_lock_take (UpBackend  *backend,
                                const char *reason,
                                const char *mode)
{
	GVariant *out, *input;
	GUnixFDList *fds = NULL;
	int fd;
	GError *error = NULL;

	g_return_val_if_fail (reason != NULL, -1);
	g_return_val_if_fail (mode != NULL, -1);
	g_return_val_if_fail (g_str_equal (mode, "delay") || g_str_equal (mode, "block"), -1);

	input = g_variant_new ("(ssss)",
			       "sleep",  /* what */
			       "UPower", /* who */
			       reason,   /* why */
			       mode);    /* mode */

	out = g_dbus_proxy_call_with_unix_fd_list_sync (backend->priv->logind_proxy,
							"Inhibit",
							input,
							G_DBUS_CALL_FLAGS_NONE,
							-1,
							NULL,
							&fds,
							NULL,
							&error);
	if (out == NULL) {
		g_warning ("Could not acquire inhibitor lock: %s",
			   error ? error->message : "Unknown reason");
		g_clear_error (&error);
		return -1;
	}

	if (g_unix_fd_list_get_length (fds) != 1) {
		g_warning ("Unexpected values returned by logind's 'Inhibit'");
		g_variant_unref (out);
		g_object_unref (fds);
		return -1;
	}

	fd = g_unix_fd_list_get (fds, 0, NULL);

	g_variant_unref (out);
	g_object_unref (fds);

	g_debug ("Acquired inhibitor lock (%i, %s)", fd, mode);

	return fd;
}

/**
 * up_backend_prepare_for_sleep:
 *
 * Callback for logind's PrepareForSleep signal. It receives
 * a boolean that indicates if we are about to sleep (TRUE)
 * or waking up (FALSE).
 * In case of the waking up we refresh the devices so we are
 * up to date, especially w.r.t. battery levels, since they
 * might have changed drastically.
 **/
static void
up_backend_prepare_for_sleep (GDBusConnection *connection,
			      const gchar     *sender_name,
			      const gchar     *object_path,
			      const gchar     *interface_name,
			      const gchar     *signal_name,
			      GVariant        *parameters,
			      gpointer         user_data)
{
	UpBackend *backend = user_data;
	gboolean will_sleep;
	GPtrArray *array;
	guint i;

	if (!g_variant_is_of_type (parameters, G_VARIANT_TYPE ("(b)"))) {
		g_warning ("logind PrepareForSleep has unexpected parameter(s)");
		return;
	}

	g_variant_get (parameters, "(b)", &will_sleep);

	if (will_sleep) {
		up_daemon_pause_poll (backend->priv->daemon);
		if (backend->priv->logind_delay_inhibitor_fd >= 0) {
			close (backend->priv->logind_delay_inhibitor_fd);
			backend->priv->logind_delay_inhibitor_fd = -1;
		}
		return;
	}

	if (backend->priv->logind_delay_inhibitor_fd < 0)
		backend->priv->logind_delay_inhibitor_fd = up_backend_inhibitor_lock_take (backend, "Pause device polling", "delay");

	/* we are waking up, lets refresh all battery devices */
	g_debug ("Woke up from sleep; about to refresh devices");
	array = up_device_list_get_array (backend->priv->device_list);

	for (i = 0; i < array->len; i++) {
		UpDevice *device = UP_DEVICE (g_ptr_array_index (array, i));
		up_device_refresh_internal (device, UP_REFRESH_RESUME);
	}

	g_ptr_array_unref (array);

	up_daemon_resume_poll (backend->priv->daemon);
}


static void
up_backend_class_init (UpBackendClass *klass)
{
	GObjectClass *object_class = G_OBJECT_CLASS (klass);
	object_class->finalize = up_backend_finalize;

	signals [SIGNAL_DEVICE_ADDED] =
		g_signal_new ("device-added",
			      G_TYPE_FROM_CLASS (object_class), G_SIGNAL_RUN_LAST,
			      G_STRUCT_OFFSET (UpBackendClass, device_added),
			      NULL, NULL, NULL,
			      G_TYPE_NONE, 1, G_TYPE_OBJECT);
	signals [SIGNAL_DEVICE_REMOVED] =
		g_signal_new ("device-removed",
			      G_TYPE_FROM_CLASS (object_class), G_SIGNAL_RUN_LAST,
			      G_STRUCT_OFFSET (UpBackendClass, device_removed),
			      NULL, NULL, NULL,
			      G_TYPE_NONE, 1, G_TYPE_OBJECT);
}

static void
up_backend_init (UpBackend *backend)
{
	GDBusConnection *bus;
	guint sleep_id;

	backend->priv = up_backend_get_instance_private (backend);
	backend->priv->config = up_config_new ();
	backend->priv->logind_proxy = g_dbus_proxy_new_for_bus_sync (G_BUS_TYPE_SYSTEM,
								     0,
								     NULL,
								     LOGIND_DBUS_NAME,
								     LOGIND_DBUS_PATH,
								     LOGIND_DBUS_INTERFACE,
								     NULL,
								     NULL);

	bus = g_dbus_proxy_get_connection (backend->priv->logind_proxy);
	sleep_id = g_dbus_connection_signal_subscribe (bus,
						       LOGIND_DBUS_NAME,
						       LOGIND_DBUS_INTERFACE,
						       "PrepareForSleep",
						       LOGIND_DBUS_PATH,
						       NULL,
						       G_DBUS_SIGNAL_FLAGS_NONE,
						       up_backend_prepare_for_sleep,
						       backend,
						       NULL);
	backend->priv->logind_sleep_id = sleep_id;
	backend->priv->logind_delay_inhibitor_fd = -1;

	backend->priv->logind_delay_inhibitor_fd = up_backend_inhibitor_lock_take (backend, "Pause device polling", "delay");
}

static void
up_backend_finalize (GObject *object)
{
	UpBackend *backend;
	GDBusConnection *bus;

	g_return_if_fail (UP_IS_BACKEND (object));

	backend = UP_BACKEND (object);

	if (backend->priv->bluez_watch_id > 0) {
		g_bus_unwatch_name (backend->priv->bluez_watch_id);
		backend->priv->bluez_watch_id = 0;
	}
	g_clear_object (&backend->priv->bluez_client);

	g_clear_object (&backend->priv->config);
	g_clear_object (&backend->priv->daemon);
	g_clear_object (&backend->priv->device_list);
	g_clear_object (&backend->priv->gudev_client);
	g_clear_object (&backend->priv->udev_enum);

	bus = g_dbus_proxy_get_connection (backend->priv->logind_proxy);
	g_dbus_connection_signal_unsubscribe (bus,
					      backend->priv->logind_sleep_id);

	if (backend->priv->logind_delay_inhibitor_fd >= 0)
		close (backend->priv->logind_delay_inhibitor_fd);

	g_clear_object (&backend->priv->logind_proxy);

	g_clear_object (&backend->priv->lid_device);

	G_OBJECT_CLASS (up_backend_parent_class)->finalize (object);
}

/**
 * up_backend_new:
 *
 * Return value: a new %UpBackend object.
 **/
UpBackend *
up_backend_new (void)
{
	return g_object_new (UP_TYPE_BACKEND, NULL);
}
