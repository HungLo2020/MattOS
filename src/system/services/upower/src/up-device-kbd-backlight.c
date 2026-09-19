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

#include "config.h"

#include <glib.h>
#include <glib/gstdio.h>
#include <glib/gi18n-lib.h>
#include <glib-object.h>

#include "up-device-kbd-backlight.h"
#include "up-device-list.h"
#include "up-native.h"
#include "up-stats-item.h"

typedef struct
{
	UpDaemon	*daemon;
	GObject		*native;
	UpDeviceList	*kbd_backlight_devices;

	gint		 max_brightness;
	gint		 brightness;
} UpDeviceKbdBacklightPrivate;

static void up_device_kbd_backlight_initable_iface_init (GInitableIface *iface);

G_DEFINE_TYPE_EXTENDED (UpDeviceKbdBacklight, up_device_kbd_backlight, UP_TYPE_EXPORTED_KBD_BACKLIGHT_SKELETON, 0,
			G_IMPLEMENT_INTERFACE (G_TYPE_INITABLE,
					       up_device_kbd_backlight_initable_iface_init)
			G_ADD_PRIVATE (UpDeviceKbdBacklight))

enum {
	PROP_0,
	PROP_DAEMON,
	PROP_NATIVE,
	PROP_DEVICE_LIST,
	PROP_MAX_BRIGHTNESS,
	PROP_BRIGHTNESS,
	N_PROPS
};


/* Composite Keyboard Backlight D-Bus API (/org/freedesktop/UPower/KbdBacklight/)
 *
 * To maintain backward compatibility, the composite keyboard backlight API extends
 * the legacy interface. The new functions provide normalized brightness values and
 * allow controlling all registered keyboard backlight devices through a single API call.
 *
 * Individual device paths append the device name as a suffix to the base path.
 * For example, the path for the "tpacpi::kbd_backlight" device is:
 * /org/freedesktop/UPower/KbdBacklight/tpacpiookbd_backlight */
#define UP_DEVICES_KBD_BACKLIGHT_DBUS_PATH "/org/freedesktop/UPower/KbdBacklight"
static GParamSpec *properties[N_PROPS];

/**
 * up_kbd_backlight_emit_change:
 **/
void
up_device_kbd_backlight_emit_change(UpDeviceKbdBacklight *kbd_backlight, int value, const char *source)
{
	up_exported_kbd_backlight_emit_brightness_changed (UP_EXPORTED_KBD_BACKLIGHT (kbd_backlight), value);
	up_exported_kbd_backlight_emit_brightness_changed_with_source (UP_EXPORTED_KBD_BACKLIGHT (kbd_backlight), value, source);
}


/**
 * up_kbd_backlight_get_brightness:
 *
 * Gets the current brightness
 **/
static gboolean
up_kbd_backlight_get_brightness (UpExportedKbdBacklight *skeleton,
				 GDBusMethodInvocation *invocation,
				 UpDeviceKbdBacklight *kbd_backlight)
{
	UpDeviceKbdBacklightPrivate *priv;
	UpDeviceKbdBacklightClass *klass;
	gint brightness = -1;

	g_return_val_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (kbd_backlight), FALSE);

	priv = up_device_kbd_backlight_get_instance_private (kbd_backlight);

	if (priv->native == NULL) {
		brightness = priv->brightness;
	} else {
		klass = UP_DEVICE_KBD_BACKLIGHT_GET_CLASS (kbd_backlight);
		if (klass->get_brightness != NULL)
			brightness = klass->get_brightness (kbd_backlight);
	}

	if (brightness >= 0) {
		up_exported_kbd_backlight_complete_get_brightness (skeleton, invocation,
								   brightness);
	} else {
		g_dbus_method_invocation_return_error (invocation,
						       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
						       "error reading brightness");
	}

	return TRUE;
}

/**
 * up_kbd_backlight_get_max_brightness:
 *
 * Gets the max brightness
 **/
static gboolean
up_kbd_backlight_get_max_brightness (UpExportedKbdBacklight *skeleton,
				     GDBusMethodInvocation *invocation,
				     UpDeviceKbdBacklight *kbd_backlight)
{
	UpDeviceKbdBacklightPrivate *priv;
	UpDeviceKbdBacklightClass *klass;
	gint max_brightness = -1;

	g_return_val_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (kbd_backlight), FALSE);

	priv = up_device_kbd_backlight_get_instance_private (kbd_backlight);

	if (priv->native == NULL) {
		max_brightness = priv->max_brightness;
	} else {
		klass = UP_DEVICE_KBD_BACKLIGHT_GET_CLASS (kbd_backlight);

		if (klass->get_max_brightness != NULL)
			max_brightness = klass->get_max_brightness (kbd_backlight);
	}

	if (max_brightness >= 0) {
		up_exported_kbd_backlight_complete_get_max_brightness (skeleton, invocation,
								       max_brightness);
	} else {
		g_dbus_method_invocation_return_error (invocation,
						       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
						       "error reading max brightness");
	}

	return TRUE;
}

/**
 * up_device_kbd_backlight_composite_set_brightness:
 *
 * Sets the brightness for the composite kbd backlight device.
 *
 * Returns: %TRUE on success.
 **/
static gboolean
up_device_kbd_backlight_composite_set_brightness (UpDeviceKbdBacklight *device, gint value)
{
	UpDeviceKbdBacklightPrivate *priv = up_device_kbd_backlight_get_instance_private (device);
	GPtrArray *array;
	gboolean any_success = FALSE;

	if (priv->kbd_backlight_devices == NULL)
		return FALSE;

	if (priv->max_brightness <= 0)
		return FALSE;

	value = CLAMP (value, 0, priv->max_brightness);

	array = up_device_list_get_array (priv->kbd_backlight_devices);
	for (guint i = 0; i < array->len; i++) {
		UpDeviceKbdBacklight *kbd_device;
		UpDeviceKbdBacklightClass *klass;
		gint max_brightness;
		gdouble ratio;
		gint scaled;

		kbd_device = UP_DEVICE_KBD_BACKLIGHT (g_ptr_array_index (array, i));

		klass = UP_DEVICE_KBD_BACKLIGHT_GET_CLASS (kbd_device);
		g_object_get (kbd_device, "max-brightness", &max_brightness, NULL);

		if (klass->set_brightness != NULL && max_brightness > 0) {
			ratio = (gdouble) max_brightness / priv->max_brightness;
			scaled = CLAMP ((gint)(ratio * value), 0, max_brightness);

			if (klass->set_brightness (kbd_device, scaled)) {
				any_success = TRUE;
				g_object_set (kbd_device, "brightness", scaled, NULL);
			}
		}
	}

	g_ptr_array_unref (array);

	if (!any_success)
		return FALSE;

	g_object_set (device, "brightness", value, NULL);

	return TRUE;
}

/**
 * up_kbd_backlight_set_brightness:
 *
 * Sets the kbd backlight LED brightness.
 *
 * Returns: %TRUE on success.
 **/
static gboolean
up_kbd_backlight_set_brightness (UpExportedKbdBacklight *skeleton,
				 GDBusMethodInvocation *invocation,
				 gint value,
				 UpDeviceKbdBacklight *kbd_backlight)
{
	UpDeviceKbdBacklightPrivate *priv;
	UpDeviceKbdBacklightClass *klass;
	gboolean ret = FALSE;

	g_return_val_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (kbd_backlight), FALSE);

	priv = up_device_kbd_backlight_get_instance_private (kbd_backlight);

	if (value < 0) {
		g_dbus_method_invocation_return_error (invocation,
						       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
						       "brightness value must be non-negative");
		return TRUE;
	}

	if (priv->native == NULL) {
		/* It is a composite kbd backlight device */
		ret = up_device_kbd_backlight_composite_set_brightness (kbd_backlight, value);
	} else {
		klass = UP_DEVICE_KBD_BACKLIGHT_GET_CLASS (kbd_backlight);

		if (klass->set_brightness == NULL) {
			g_dbus_method_invocation_return_error (invocation,
							       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
							       "setting brightness is unsupported");
			return TRUE;
		}
		if (priv->max_brightness < 0) {
			g_dbus_method_invocation_return_error (invocation,
							       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
							       "max brightness not yet available");
			return TRUE;
		}
		value = CLAMP (value, 0, priv->max_brightness);
		ret = klass->set_brightness (kbd_backlight, value);
		if (ret)
			g_object_set (kbd_backlight, "brightness", value, NULL);
	}

	if (ret) {
		up_exported_kbd_backlight_complete_set_brightness (skeleton, invocation);
		up_device_kbd_backlight_emit_change (kbd_backlight, priv->brightness, "external");
	} else {
		g_dbus_method_invocation_return_error (invocation,
						       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
						       "error writing brightness %d", value);
	}

	return TRUE;
}

GObject *
up_device_kbd_backlight_get_native (UpDeviceKbdBacklight *device)
{
	UpDeviceKbdBacklightPrivate *priv = up_device_kbd_backlight_get_instance_private (device);
	g_return_val_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (device), NULL);
	return priv->native;
}

static gchar *
up_device_kbd_backlight_compute_object_path (UpDeviceKbdBacklight *device)
{
	UpDeviceKbdBacklightPrivate *priv = up_device_kbd_backlight_get_instance_private (device);
	g_autofree gchar *basename = NULL;
	g_autofree gchar *id = NULL;
	gchar *object_path;
	const gchar *native_path;
	guint i;

	if (priv->native == NULL)
		return g_strdup (UP_DEVICES_KBD_BACKLIGHT_DBUS_PATH);

	native_path = up_exported_kbd_backlight_get_native_path (UP_EXPORTED_KBD_BACKLIGHT (device));
	basename = g_path_get_basename (native_path);
	id = g_strjoin ("_", basename, NULL);

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
	object_path = g_build_filename (UP_DEVICES_KBD_BACKLIGHT_DBUS_PATH, id, NULL);

	return object_path;
}

static void
up_device_kbd_backlight_export_skeleton (UpDeviceKbdBacklight *device,
				         const gchar *object_path)
{
	UpDeviceKbdBacklightPrivate *priv = up_device_kbd_backlight_get_instance_private (device);
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

/**
 * up_device_kbd_backlight_report:
 * @self: keyboard backlight device that changed
 * @max_brightness: new maximum brightness value from hardware
 * @brightness: new current brightness value from hardware
 * @composite_signal: whether to emit a brightness-changed signal on the composite device
 *
 * Updates the stored brightness properties for a hardware keyboard backlight
 * device and triggers recomputation of the composite keyboard backlight.
 * Ignored when called on the composite device itself (native == NULL).
 **/
void
up_device_kbd_backlight_report (UpDeviceKbdBacklight *self, gint max_brightness, gint brightness, gboolean composite_signal)
{
	UpDeviceKbdBacklightPrivate *priv = up_device_kbd_backlight_get_instance_private (self);
	gboolean changed = FALSE;

	g_return_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (self));

	/* Don't report for a composite kbd backlight device. */
	if (priv->native == NULL)
		return;

	if (priv->max_brightness != max_brightness) {
		g_object_set (self, "max-brightness", max_brightness, NULL);
		changed = TRUE;
	}

	if (priv->brightness != brightness) {
		g_object_set (self, "brightness", brightness, NULL);
		changed = TRUE;
	}

	if (changed && priv->daemon != NULL)
		up_daemon_kbd_backlight_composite_update (priv->daemon, composite_signal);
}

gboolean
up_device_kbd_backlight_register (UpDeviceKbdBacklight *device)
{
	g_autofree char *computed_object_path = NULL;

	if (g_dbus_interface_skeleton_get_object_path (G_DBUS_INTERFACE_SKELETON (device)) != NULL)
		return FALSE;

	computed_object_path = up_device_kbd_backlight_compute_object_path (device);
	g_debug ("Exported Keyboard backlight with path %s", computed_object_path);
	up_device_kbd_backlight_export_skeleton (device, computed_object_path);
	return TRUE;
}

void
up_device_kbd_backlight_unregister (UpDeviceKbdBacklight *device)
{
	g_autofree char *object_path = NULL;

	object_path = g_strdup (g_dbus_interface_skeleton_get_object_path (G_DBUS_INTERFACE_SKELETON (device)));
	if (object_path != NULL) {
		g_dbus_interface_skeleton_unexport (G_DBUS_INTERFACE_SKELETON (device));
		g_debug ("Unexported UpDeviceKbdBacklight with path %s", object_path);
	}
}

const gchar *
up_device_kbd_backlight_get_object_path (UpDeviceKbdBacklight *device)
{
	g_return_val_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (device), NULL);
	return g_dbus_interface_skeleton_get_object_path (G_DBUS_INTERFACE_SKELETON (device));
}

static void
up_device_kbd_backlight_set_property (GObject      *object,
				      guint         prop_id,
				      const GValue *value,
				      GParamSpec   *pspec)
{
	UpDeviceKbdBacklight *device = UP_DEVICE_KBD_BACKLIGHT (object);
	UpDeviceKbdBacklightPrivate *priv = up_device_kbd_backlight_get_instance_private (device);

	switch (prop_id)
	{
	case PROP_DAEMON:
		priv->daemon = g_value_dup_object (value);
		break;
	case PROP_NATIVE:
		priv->native = g_value_dup_object (value);
		break;
	case PROP_DEVICE_LIST:
		g_clear_object (&priv->kbd_backlight_devices);
		priv->kbd_backlight_devices = g_value_dup_object (value);
		break;
	case PROP_MAX_BRIGHTNESS:
		priv->max_brightness = g_value_get_int (value);
		break;
	case PROP_BRIGHTNESS:
		priv->brightness = g_value_get_int (value);
		break;
	default:
		G_OBJECT_WARN_INVALID_PROPERTY_ID (object, prop_id, pspec);
	}
}

static void
up_device_kbd_backlight_get_property (GObject      *object,
				      guint         prop_id,
				      GValue       *value,
				      GParamSpec   *pspec)
{
	UpDeviceKbdBacklight *device = UP_DEVICE_KBD_BACKLIGHT (object);
	UpDeviceKbdBacklightPrivate *priv = up_device_kbd_backlight_get_instance_private (device);

	switch (prop_id)
	{
	case PROP_MAX_BRIGHTNESS:
		g_value_set_int (value, priv->max_brightness);
		break;
	case PROP_BRIGHTNESS:
		g_value_set_int (value, priv->brightness);
		break;
	default:
		G_OBJECT_WARN_INVALID_PROPERTY_ID (object, prop_id, pspec);
	}
}

static gboolean
up_device_kbd_backlight_initable_init (GInitable     *initable,
				       GCancellable  *cancellable,
				       GError       **error)
{
	UpDeviceKbdBacklight *device = UP_DEVICE_KBD_BACKLIGHT (initable);
	UpDeviceKbdBacklightPrivate *priv = up_device_kbd_backlight_get_instance_private (device);
	UpDeviceKbdBacklightClass *klass = UP_DEVICE_KBD_BACKLIGHT_GET_CLASS (device);
	const gchar *native_path = "KbdBacklight";
	int ret;

	g_return_val_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (device), FALSE);

	if (priv->native) {
		native_path = up_native_get_native_path (priv->native);
		up_exported_kbd_backlight_set_native_path (UP_EXPORTED_KBD_BACKLIGHT (device), native_path);
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

	up_device_kbd_backlight_register (device);

	return TRUE;
}

static void
up_device_kbd_backlight_initable_iface_init (GInitableIface *iface)
{
	iface->init = up_device_kbd_backlight_initable_init;
}

/**
 * up_device_kbd_backlight_init:
 **/
static void
up_device_kbd_backlight_init (UpDeviceKbdBacklight *kbd_backlight)
{
	g_signal_connect (kbd_backlight, "handle-get-brightness",
			  G_CALLBACK (up_kbd_backlight_get_brightness), kbd_backlight);
	g_signal_connect (kbd_backlight, "handle-get-max-brightness",
			  G_CALLBACK (up_kbd_backlight_get_max_brightness), kbd_backlight);
	g_signal_connect (kbd_backlight, "handle-set-brightness",
			  G_CALLBACK (up_kbd_backlight_set_brightness), kbd_backlight);
}

/**
 * up_device_kbd_backlight_dispose:
 **/
static void
up_device_kbd_backlight_dispose (GObject *object)
{
	UpDeviceKbdBacklight *kbd_backlight = UP_DEVICE_KBD_BACKLIGHT (object);
	UpDeviceKbdBacklightPrivate *priv = up_device_kbd_backlight_get_instance_private (kbd_backlight);

	g_clear_object (&priv->daemon);

	G_OBJECT_CLASS (up_device_kbd_backlight_parent_class)->dispose (object);
}

/**
 * up_device_kbd_backlight_finalize:
 **/
static void
up_device_kbd_backlight_finalize (GObject *object)
{
	UpDeviceKbdBacklight *kbd_backlight = UP_DEVICE_KBD_BACKLIGHT (object);
	UpDeviceKbdBacklightPrivate *priv = up_device_kbd_backlight_get_instance_private (kbd_backlight);

	g_return_if_fail (object != NULL);
	g_return_if_fail (UP_IS_DEVICE_KBD_BACKLIGHT (object));

	g_clear_object (&priv->native);
	g_clear_object (&priv->kbd_backlight_devices);

	G_OBJECT_CLASS (up_device_kbd_backlight_parent_class)->finalize (object);
}

static void
up_device_kbd_backlight_class_init (UpDeviceKbdBacklightClass *klass)
{
	GObjectClass *object_class = G_OBJECT_CLASS (klass);

	object_class->dispose = up_device_kbd_backlight_dispose;
	object_class->finalize = up_device_kbd_backlight_finalize;

	object_class->set_property = up_device_kbd_backlight_set_property;
	object_class->get_property = up_device_kbd_backlight_get_property;

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
	properties[PROP_DEVICE_LIST] =
		g_param_spec_object ("device-list",
				     "DeviceList",
				     "Device List",
				     UP_TYPE_DEVICE_LIST,
				     G_PARAM_STATIC_STRINGS | G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY);
	properties[PROP_MAX_BRIGHTNESS] =
		g_param_spec_int ("max-brightness",
				  "MaxBrightness",
				  "Max brightness",
				  -1, G_MAXINT, -1,
				  G_PARAM_STATIC_STRINGS | G_PARAM_READABLE | G_PARAM_WRITABLE);
	properties[PROP_BRIGHTNESS] =
		g_param_spec_int ("brightness",
				  "Brightness",
				  "Current brightness",
				  -1, G_MAXINT, -1,
				  G_PARAM_STATIC_STRINGS | G_PARAM_READABLE | G_PARAM_WRITABLE);

	g_object_class_install_properties (object_class, N_PROPS, properties);
}

/**
 * up_device_kbd_backlight_new:
 **/
UpDeviceKbdBacklight *
up_device_kbd_backlight_new (UpDaemon *daemon, GObject *native, UpDeviceList *device_list)
{
	return UP_DEVICE_KBD_BACKLIGHT (g_object_new (UP_TYPE_DEVICE_KBD_BACKLIGHT,
					"daemon", daemon,
					"native", native,
					"device-list", device_list,
					NULL));
}
