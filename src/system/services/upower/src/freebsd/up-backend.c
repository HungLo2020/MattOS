/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2009 Joe Marcus Clarke <marcus@FreeBSD.org>
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

#include <math.h>
#include <stdio.h>
#include <string.h>
#include <fcntl.h>
#include <limits.h>
#include <unistd.h>
#include <sys/types.h>
#include <glib/gi18n.h>
#include <gio/gio.h>

#include "up-acpi-native.h"
#include "up-backend-acpi.h"
#include "up-devd.h"
#include "up-device-supply.h"
#include "up-util.h"

#include "up-backend.h"
#include "up-daemon.h"
#include "up-device.h"
#include "up-backend-bsd-private.h"

#define UP_BACKEND_REFRESH_TIMEOUT	30	/* seconds */

static void	up_backend_class_init	(UpBackendClass	*klass);
static void	up_backend_init	(UpBackend		*backend);
static void	up_backend_finalize	(GObject		*object);

static gboolean	up_backend_acpi_devd_notify (UpBackend *backend, const gchar *system, const gchar *subsystem, const gchar *type, const gchar *data);
static void	up_backend_create_new_device (UpBackend *backend, UpAcpiNative *native);
static void	up_backend_lid_coldplug (UpBackend *backend);

struct UpBackendPrivate
{
	UpDaemon		*daemon;
	UpDeviceList		*device_list;
	GHashTable		*handle_map;
	UpConfig		*config;
	GDBusProxy		*seat_manager_proxy;
};

enum {
	SIGNAL_DEVICE_ADDED,
	SIGNAL_DEVICE_REMOVED,
	SIGNAL_LAST
};

static guint signals [SIGNAL_LAST] = { 0 };

G_DEFINE_TYPE_WITH_PRIVATE (UpBackend, up_backend, G_TYPE_OBJECT)

static const gchar *handlers[] = {
	"battery",
};

UpDevdHandler up_backend_acpi_devd_handler = {
	.notify = up_backend_acpi_devd_notify
};

/**
 * up_backend_acpi_devd_notify:
 **/
static gboolean
up_backend_acpi_devd_notify (UpBackend *backend, const gchar *system, const gchar *subsystem, const gchar *type, const gchar *data)
{
	GObject *object = NULL;
	UpAcpiNative *native = NULL;

	if (strcmp (system, "ACPI"))
		return FALSE;

	if (!strcmp (subsystem, "ACAD")) {
		native = up_acpi_native_new ("hw.acpi.acline");
		object = up_device_list_lookup (backend->priv->device_list, G_OBJECT (native));
	} else if (!strcmp (subsystem, "CMBAT")) {
		gchar *ptr;
		int unit;

		ptr = strstr (type, ".BAT");

		if (ptr != NULL && sscanf (ptr, ".BAT%i", &unit)) {
			native = up_acpi_native_new_driver_unit ("battery", unit);
			object = up_device_list_lookup (backend->priv->device_list, G_OBJECT (native));
			if (object == NULL) {
				gpointer hptr;

				hptr = g_hash_table_lookup (backend->priv->handle_map, type);
				if (hptr != NULL) {
					object = up_device_list_lookup (backend->priv->device_list, G_OBJECT (hptr));
				}
			}
		}
	} else if (!strcmp (subsystem, "Lid")) {
		gboolean is_present;
		gboolean is_closed;

		g_object_get (backend->priv->daemon,
			      "lid-is-present", &is_present, NULL);
		if (!is_present) {
			g_warning ("received lid event without a configured lid; cold-plugging one");
			up_backend_lid_coldplug (backend);
			/* FALLTHROUGH */
		}

		is_closed = (data != NULL && !strcmp (data, "notify=0x00")) ? TRUE : FALSE;
		up_daemon_set_lid_is_closed (backend->priv->daemon, is_closed);
		goto out;
	}

	if (native == NULL)
		goto out;

	if (object == NULL) {
		g_warning ("did not find existing %s device; cold-plugging a new one", subsystem);
		up_backend_create_new_device (backend, native);
		goto out;
	}

	up_device_refresh_internal (UP_DEVICE (object), UP_REFRESH_EVENT);

	if (object != NULL)
		g_object_unref (object);
out:
	if (native != NULL)
		g_object_unref (native);

	return TRUE;
}

/**
 * up_backend_create_new_device:
 **/
static void
up_backend_create_new_device (UpBackend *backend, UpAcpiNative *native)
{
	g_autoptr(UpDevice) device = NULL;

	device = g_initable_new (UP_TYPE_DEVICE_SUPPLY, NULL, NULL,
	                         "daemon", backend->priv->daemon,
	                         "native", G_OBJECT (native),
	                         "poll-timeout", UP_BACKEND_REFRESH_TIMEOUT,
	                         NULL);
	if (device) {
		if (!strncmp (up_acpi_native_get_path (native), "dev.", strlen ("dev."))) {
			const gchar *path;

			path = up_acpi_native_get_path (native);
			if (up_has_sysctl ("%s.%%location", path)) {
				gchar *location;

				location = up_get_string_sysctl (NULL, "%s.%%location", path);
				if (location != NULL && strstr (location, "handle=") != NULL) {
					gchar *handle;

					handle = strstr (location, "handle=");
					handle += strlen ("handle=");
					g_hash_table_insert (backend->priv->handle_map, g_strdup (handle), g_object_ref (native));
				}
				g_free (location);
			}
		}

		g_signal_emit (backend, signals[SIGNAL_DEVICE_ADDED], 0, device);
	}
}

/**
 * up_backend_lid_coldplug:
 **/
static void
up_backend_lid_coldplug (UpBackend *backend)
{
	gchar *lid_state;

	lid_state = up_get_string_sysctl (NULL, "hw.acpi.lid_switch_state");
	if (lid_state) {
		up_daemon_set_lid_is_present (backend->priv->daemon, TRUE);
	}
	g_free (lid_state);
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
	UpAcpiNative *acnative;
	int i;

	backend->priv->daemon = g_object_ref (daemon);
	backend->priv->device_list = up_daemon_get_device_list (daemon);

	for (i = 0; i < (int) G_N_ELEMENTS (handlers); i++) {
		int j;

		for (j = 0; up_has_sysctl ("dev.%s.%i.%%driver", handlers[i], j); j++) {
			UpAcpiNative *native;
			UpDevice *device;
			GObject *object;

			native = up_acpi_native_new_driver_unit (handlers[i], j);
			object = up_device_list_lookup (backend->priv->device_list, G_OBJECT (native));
			if (object != NULL) {
				device = UP_DEVICE (object);
				g_warning ("treating add event as change event on %s", up_device_get_object_path (device));
				up_device_refresh_internal (device, UP_REFRESH_EVENT);
			} else {
				up_backend_create_new_device (backend, native);
			}

			if (object != NULL) {
				g_object_unref (object);
			}
			if (native != NULL) {
				g_object_unref (native);
			}
		}
	}

	up_backend_lid_coldplug (backend);

	acnative = up_acpi_native_new ("hw.acpi.acline");
	up_backend_create_new_device (backend, acnative);
	g_object_unref (acnative);

	up_devd_init (backend);

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
	if (backend->priv->device_list != NULL) {
		g_object_unref (backend->priv->device_list);
		backend->priv->device_list = NULL;
	}
	if (backend->priv->daemon != NULL) {
		g_object_unref (backend->priv->daemon);
		backend->priv->daemon = NULL;
	}
}

/**
 * up_backend_get_seat_manager_proxy:
 * @backend: The %UpBackend class instance
 *
 * Returns the seat manager object or NULL on error. [transfer none]
 */
GDBusProxy *
up_backend_get_seat_manager_proxy (UpBackend  *backend)
{
	g_return_val_if_fail (UP_IS_BACKEND (backend), NULL);

	return backend->priv->seat_manager_proxy;
}

/**
 * up_backend_get_config:
 * @backend: The %UpBackend class instance
 *
 * Returns the UpConfig object or NULL on error. [transfer none]
 */
UpConfig *
up_backend_get_config (UpBackend  *backend)
{
	g_return_val_if_fail (UP_IS_BACKEND (backend), NULL);

	return backend->priv->config;
}

/**
 * up_backend_class_init:
 * @klass: The UpBackendClass
 **/
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
			      G_TYPE_NONE, 1, UP_TYPE_DEVICE);
	signals [SIGNAL_DEVICE_REMOVED] =
		g_signal_new ("device-removed",
			      G_TYPE_FROM_CLASS (object_class), G_SIGNAL_RUN_LAST,
			      G_STRUCT_OFFSET (UpBackendClass, device_removed),
			      NULL, NULL, NULL,
			      G_TYPE_NONE, 1, UP_TYPE_DEVICE);
}

/**
 * up_backend_init:
 **/
static void
up_backend_init (UpBackend *backend)
{
	backend->priv = up_backend_get_instance_private (backend);
	backend->priv->handle_map = g_hash_table_new_full (g_str_hash, g_str_equal, (GDestroyNotify) g_free, (GDestroyNotify) g_object_unref);
	backend->priv->config = up_config_new ();
	backend->priv->seat_manager_proxy = g_dbus_proxy_new_for_bus_sync (G_BUS_TYPE_SYSTEM,
									   0,
									   NULL,
									   CONSOLEKIT2_DBUS_NAME,
									   CONSOLEKIT2_DBUS_PATH,
									   CONSOLEKIT2_DBUS_INTERFACE,
									   NULL,
									   NULL);
}

/**
 * up_backend_finalize:
 **/
static void
up_backend_finalize (GObject *object)
{
	UpBackend *backend;

	g_return_if_fail (UP_IS_BACKEND (object));

	backend = UP_BACKEND (object);

	g_object_unref (backend->priv->config);
	if (backend->priv->daemon != NULL)
		g_object_unref (backend->priv->daemon);
	if (backend->priv->device_list != NULL)
		g_object_unref (backend->priv->device_list);
	if (backend->priv->handle_map != NULL)
		g_hash_table_unref (backend->priv->handle_map);
	g_clear_object (&backend->priv->seat_manager_proxy);

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
