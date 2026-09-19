/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2011 Landry Breuil <landry@openbsd.org>
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

#include "up-apm-native.h"

#include "up-backend.h"
#include "up-daemon.h"
#include "up-device.h"
#include "up-config.h"
#include "up-backend-bsd-private.h"
#include <string.h> /* strcmp() */

static void	up_backend_class_init	(UpBackendClass	*klass);
static void	up_backend_init	(UpBackend		*backend);
static void	up_backend_finalize	(GObject		*object);

static gboolean	up_backend_apm_get_power_info(struct apm_power_info*);
static gpointer	up_backend_apm_event_thread(gpointer object);
UpDeviceState up_backend_apm_get_battery_state_value(u_char battery_state);
static void	up_backend_update_acpibat_state(UpDevice*, struct sensordev);
static void	up_backend_update_lid_status(UpDaemon*);

static gboolean		up_apm_device_get_on_battery	(UpDevice *device, gboolean *on_battery);
static gboolean		up_apm_device_get_online		(UpDevice *device, gboolean *online);
static gboolean		up_apm_device_refresh		(UpDevice *device, UpRefreshReason reason);

struct UpBackendPrivate
{
	UpDaemon		*daemon;
	UpDevice		*ac;
	UpDevice		*battery;
	GThread			*apm_thread;
	gboolean		is_laptop;
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

/**
 * functions called by upower daemon
 **/


/* those three ripped from freebsd/up-device-supply.c */
gboolean
up_apm_device_get_on_battery (UpDevice *device, gboolean * on_battery)
{
	UpDeviceKind type;
	UpDeviceState state;
	gboolean is_present;

	g_return_val_if_fail (on_battery != NULL, FALSE);

	g_object_get (device,
		      "type", &type,
		      "state", &state,
		      "is-present", &is_present,
		      (void*) NULL);

	if (type != UP_DEVICE_KIND_BATTERY)
		return FALSE;
	if (state == UP_DEVICE_STATE_UNKNOWN)
		return FALSE;
	if (!is_present)
		return FALSE;

	*on_battery = (state == UP_DEVICE_STATE_DISCHARGING);
	return TRUE;
}

gboolean
up_apm_device_get_online (UpDevice *device, gboolean * online)
{
	UpDeviceKind type;
	gboolean online_tmp;

	g_return_val_if_fail (online != NULL, FALSE);

	g_object_get (device,
		      "type", &type,
		      "online", &online_tmp,
		      (void*) NULL);

	if (type != UP_DEVICE_KIND_LINE_POWER)
		return FALSE;

	*online = online_tmp;

	return TRUE;
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
	GError *err = NULL;
	UpDeviceClass *device_class;
	gint64 current_time;

	backend->priv = up_backend_get_instance_private (backend);
	backend->priv->daemon = g_object_ref (daemon);
	backend->priv->is_laptop = up_native_is_laptop();
	g_debug("is_laptop:%d",backend->priv->is_laptop);
	if (backend->priv->is_laptop)
	{
		UpApmNative *acnative = NULL;
		UpApmNative *battnative = NULL;

		acnative = up_apm_native_new("/ac");
		battnative = up_apm_native_new("/batt");

		backend->priv->ac = UP_DEVICE(up_device_new (backend->priv->daemon, G_OBJECT(acnative)));
		backend->priv->battery = UP_DEVICE(up_device_new (backend->priv->daemon, G_OBJECT(battnative)));

		g_object_unref (acnative);
		g_object_unref (battnative);

		device_class = UP_DEVICE_GET_CLASS (backend->priv->battery);
		device_class->get_on_battery = up_apm_device_get_on_battery;
		device_class->get_online = up_apm_device_get_online;
		device_class->refresh = up_apm_device_refresh;
		device_class = UP_DEVICE_GET_CLASS (backend->priv->ac);
		device_class->get_on_battery = up_apm_device_get_on_battery;
		device_class->get_online = up_apm_device_get_online;
		device_class->refresh = up_apm_device_refresh;
		/* creates thread */
		if((backend->priv->apm_thread = (GThread*) g_thread_try_new("apm-poller",(GThreadFunc)up_backend_apm_event_thread, (void*) backend, &err)) == NULL)
		{
			g_warning("Thread create failed: %s", err->message);
			g_error_free (err);
		}

		/* setup dummy */
		current_time = g_get_real_time () / G_USEC_PER_SEC;
		g_object_set (backend->priv->battery,
			      "type", UP_DEVICE_KIND_BATTERY,
			      "power-supply", TRUE,
			      "is-present", TRUE,
			      "is-rechargeable", TRUE,
			      "has-history", TRUE,
			      "state", UP_DEVICE_STATE_UNKNOWN,
			      "percentage", 0.0f,
			      "time-to-empty", (gint64) 0,
			      "update-time", (guint64) current_time,
			      (void*) NULL);
		g_object_set (backend->priv->ac,
			      "type", UP_DEVICE_KIND_LINE_POWER,
			      "online", TRUE,
			      "power-supply", TRUE,
			      "update-time", (guint64) current_time,
			      (void*) NULL);

		up_backend_update_lid_status(daemon);
		if (!g_initable_init (G_INITABLE (backend->priv->ac), NULL, NULL))
			g_warning ("failed to coldplug ac");
		else
			g_signal_emit (backend, signals[SIGNAL_DEVICE_ADDED], 0, backend->priv->ac);

		if (!g_initable_init (G_INITABLE (backend->priv->battery), NULL, NULL))
			g_warning ("failed to coldplug battery");
		else
			g_signal_emit (backend, signals[SIGNAL_DEVICE_ADDED], 0, backend->priv->battery);
	}
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
 * OpenBSD specific code
 **/

static gboolean
up_backend_apm_get_power_info(struct apm_power_info *bstate) {
	bstate->battery_state = 255;
	bstate->ac_state = 255;
	bstate->battery_life = 0;
	bstate->minutes_left = -1;

#ifndef UPOWER_CI_DISABLE_PLATFORM_CODE
	if (-1 == ioctl(up_apm_get_fd(), APM_IOC_GETPOWER, bstate)) {
		g_error("ioctl on apm fd failed : %s", g_strerror(errno));
		return FALSE;
	}
#endif
	return TRUE;
}

UpDeviceState up_backend_apm_get_battery_state_value(u_char battery_state) {
	switch(battery_state) {
		case APM_BATT_HIGH:
			return UP_DEVICE_STATE_FULLY_CHARGED;
		case APM_BATT_LOW:
			return UP_DEVICE_STATE_DISCHARGING; // XXXX
		case APM_BATT_CRITICAL:
			return UP_DEVICE_STATE_EMPTY;
		case APM_BATT_CHARGING:
			return UP_DEVICE_STATE_CHARGING;
		case APM_BATTERY_ABSENT:
			return UP_DEVICE_STATE_EMPTY;
		case APM_BATT_UNKNOWN:
			return UP_DEVICE_STATE_UNKNOWN;
	}
	return -1;
}

static gboolean
up_backend_update_ac_state(UpDevice* device)
{
	gboolean ret, new_is_online, cur_is_online;
	struct apm_power_info a;

	up_backend_update_lid_status(up_device_get_daemon(device));
	ret = up_backend_apm_get_power_info(&a);
	if (!ret)
		return ret;

	g_object_get (device, "online", &cur_is_online, (void*) NULL);
	/* XXX use acpiac0.indicator0 if available */
	new_is_online = (a.ac_state == APM_AC_ON ? TRUE : FALSE);
	if (cur_is_online != new_is_online)
	{
		g_object_set (device,
			"online", new_is_online,
			(void*) NULL);
		return TRUE;
	}
	return FALSE;
}

static gboolean
up_backend_update_battery_state(UpDevice* device)
{
	gdouble percentage;
	gboolean ret, is_present;
	struct sensordev sdev;
	UpDeviceState cur_state, new_state = UP_DEVICE_STATE_UNKNOWN;
	gint64 cur_time_to_empty, new_time_to_empty;
	struct apm_power_info a;

	ret = up_backend_apm_get_power_info(&a);
	if (!ret)
		return ret;

	g_object_get (device,
		"state", &cur_state,
		"percentage", &percentage,
		"time-to-empty", &cur_time_to_empty,
		"is-present", &is_present,
		(void*) NULL);

	// zero out new_time_to empty if we're not discharging or minutes_left is negative
	new_time_to_empty = (new_state == UP_DEVICE_STATE_DISCHARGING && a.minutes_left > 0 ? a.minutes_left : 0);

	/* XXX use acpibat0.raw0 if available */
	/*
	 * XXX: Stop having a split brain regarding
	 * up_backend_apm_get_battery_state_value(). Either move the state
	 * setting code below into that function, or inline that function here.
	 */
	new_state = up_backend_apm_get_battery_state_value(a.battery_state);
	// if percentage/minutes goes down or ac is off, we're likely discharging..
	if (percentage < a.battery_life || cur_time_to_empty < new_time_to_empty || a.ac_state == APM_AC_OFF)
		new_state = UP_DEVICE_STATE_DISCHARGING;
	/*
	 * If we're on AC, we may either be charging, or the battery is already
	 * fully charged. Figure out which.
	 */
	if (a.ac_state == APM_AC_ON) {
		if ((gdouble) a.battery_life >= 99.0)
			new_state = UP_DEVICE_STATE_FULLY_CHARGED;
		else
			new_state = UP_DEVICE_STATE_CHARGING;
	}

	if ((a.battery_state == APM_BATTERY_ABSENT) ||
	    (a.battery_state == APM_BATT_UNKNOWN)) {
		/* Reset some known fields which remain untouched below. */
		g_object_set(device,
			     "is-rechargeable", FALSE,
			     "energy", (gdouble) 0.0,
			     "energy-empty", (gdouble) 0.0,
			     "energy-full", (gdouble) 0.0,
			     "energy-full-design", (gdouble) 0.0,
			     "energy-rate", (gdouble) 0.0,
			     NULL);
		is_present = FALSE;
		if (a.battery_state == APM_BATTERY_ABSENT)
			new_state = UP_DEVICE_STATE_EMPTY;
		else
			new_state = UP_DEVICE_STATE_UNKNOWN;
	} else {
		is_present = TRUE;
	}

	if (cur_state != new_state ||
		percentage != (gdouble) a.battery_life ||
		cur_time_to_empty != new_time_to_empty)
	{
		g_object_set (device,
			"state", new_state,
			"percentage", (gdouble) a.battery_life,
			"time-to-empty", new_time_to_empty * 60,
			"is-present", is_present,
			(void*) NULL);
		if(up_native_get_sensordev("acpibat0", &sdev))
			up_backend_update_acpibat_state(device, sdev);
		else if(up_native_get_sensordev("qcpas0", &sdev))
			up_backend_update_acpibat_state(device, sdev);
		return TRUE;
	}
	return FALSE;
}

/* update acpibat properties */
static void
up_backend_update_acpibat_state(UpDevice* device, struct sensordev s)
{
#ifndef UPOWER_CI_DISABLE_PLATFORM_CODE
	enum sensor_type type;
	int numt;
	gdouble bst_volt, bst_rate, bif_cap, bif_lastfullcap, bst_cap, bif_lowcap, capacity;
	/* gdouble bif_dvolt; */
	struct sensor sens;
	size_t slen = sizeof(sens);
	int mib[] = {CTL_HW, HW_SENSORS, 0, 0, 0};

	mib[2] = s.num;
	for (type = 0; type < SENSOR_MAX_TYPES; type++) {
		mib[3] = type;
		for (numt = 0; numt < s.maxnumt[type]; numt++) {
			mib[4] = numt;
			if (sysctl(mib, 5, &sens, &slen, NULL, 0) < 0)
				g_error("failed to get sensor type %d(%s) numt %d on %s", type, sensor_type_s[type], numt, s.xname);
			else if (slen > 0 && (sens.flags & SENSOR_FINVALID) == 0) {
				if (sens.type == SENSOR_VOLTS_DC && !strcmp(sens.desc, "current voltage"))
					bst_volt = sens.value / 1000000.0f;
				if ((sens.type == SENSOR_AMPHOUR || sens.type == SENSOR_WATTHOUR) && !strcmp(sens.desc, "design capacity")) {
					bif_cap = (sens.type == SENSOR_AMPHOUR ? bst_volt : 1) * sens.value / 1000000.0f;
				}
				if ((sens.type == SENSOR_AMPHOUR || sens.type == SENSOR_WATTHOUR) && !strcmp(sens.desc, "last full capacity")) {
					bif_lastfullcap = (sens.type == SENSOR_AMPHOUR ? bst_volt : 1) * sens.value / 1000000.0f;
				}
				if ((sens.type == SENSOR_AMPHOUR || sens.type == SENSOR_WATTHOUR) && !strcmp(sens.desc, "low capacity")) {
					bif_lowcap = (sens.type == SENSOR_AMPHOUR ? bst_volt : 1) * sens.value / 1000000.0f;
				}
				if ((sens.type == SENSOR_AMPHOUR || sens.type == SENSOR_WATTHOUR) && !strcmp(sens.desc, "remaining capacity")) {
					bst_cap = (sens.type == SENSOR_AMPHOUR ? bst_volt : 1) * sens.value / 1000000.0f;
				}
				if ((sens.type == SENSOR_AMPS || sens.type == SENSOR_WATTS) && !strcmp(sens.desc, "rate")) {
					bst_rate = (sens.type == SENSOR_AMPS ? bst_volt : 1) * sens.value / 1000000.0f;
				}
				/*
				bif_dvolt = "voltage" = unused ?
				amphour1 = warning capacity ?
				raw0 = battery state
				*/
			}
		}
	}

	capacity = 0.0f;
	if(bif_lastfullcap > 0 && bif_cap > 0) {
		capacity = (bif_lastfullcap / bif_cap) * 100.0f;
		if (capacity < 0)
			capacity = 0.0f;
		if (capacity > 100.0)
			capacity = 100.0f;
	}

	g_object_set (device,
		"energy", bst_cap,
		"energy-full", bif_lastfullcap,
		"energy-full-design", bif_cap,
		"energy-rate", bst_rate,
		"energy-empty", bif_lowcap,
		"voltage", bst_volt,
		"capacity", capacity,
		(void*) NULL);
#endif
}

/* callback updating the device */
static gboolean
up_backend_apm_powerchange_event_cb(gpointer object)
{
	UpBackend *backend;

	g_return_val_if_fail (UP_IS_BACKEND (object), FALSE);
	backend = UP_BACKEND (object);
	up_device_refresh_internal (backend->priv->ac, UP_REFRESH_EVENT);
	up_device_refresh_internal (backend->priv->battery, UP_REFRESH_EVENT);
	/* return false to not endless loop */
	return FALSE;
}

static gboolean
up_apm_device_refresh(UpDevice* device, UpRefreshReason reason)
{
	UpDeviceKind type;
	gboolean ret;
	g_object_get (device, "type", &type, NULL);

	switch (type) {
		case UP_DEVICE_KIND_LINE_POWER:
			ret = up_backend_update_ac_state(device);
			break;
		case UP_DEVICE_KIND_BATTERY:
			ret = up_backend_update_battery_state(device);
			break;
		default:
			g_assert_not_reached ();
			break;
	}

	if (ret)
		g_object_set (device, "update-time", (guint64) g_get_real_time () / G_USEC_PER_SEC, NULL);

	return ret;
}

/*
 * Check the lid status, return TRUE if one was found, FALSE otherwise.
 */
static void
up_backend_update_lid_status(UpDaemon *daemon) {
#ifndef UPOWER_CI_DISABLE_PLATFORM_CODE
	/* Use hw.sensors.acpibtn0.indicator0=On (lid open) */
	struct sensordev sensordev;
	struct sensor sensor;
	size_t sdlen, slen;
	int dev, numt, mib[5] = {CTL_HW, HW_SENSORS, 0, 0, 0};
	gboolean lid_found = FALSE;
	gboolean lid_open = FALSE;

	sdlen = sizeof(struct sensordev);
	slen  = sizeof(struct sensor);

	/* go through all acpibtn devices, and check if one of the values match "lid"
	   if so, use that device.
	*/
	for (dev = 0; ; dev++) {
		mib[2] = dev;
		if (sysctl(mib, 3, &sensordev, &sdlen, NULL, 0) == -1) {
			if (errno == ENXIO)
				continue;
			if (errno == ENOENT)
				break;
		}

		if (strstr(sensordev.xname, "acpibtn") != NULL) {
			mib[3] = SENSOR_INDICATOR;
			for (numt = 0; numt < sensordev.maxnumt[SENSOR_INDICATOR]; numt++) {
				mib[4] = numt;
				if (sysctl(mib, 5, &sensor, &slen, NULL, 0) == -1) {
					if (errno != ENOENT) {
						g_warning("failed to get sensor data from %s",
							  sensordev.xname);
						continue;
					}
				}

				/*
				 * Found an acpibtn device, now check if the
				 * description has got anything with a lid in it.
				 */
				if (strstr(sensor.desc, "lid open") == NULL) {
					g_warning ("nothing here for %s with %s\n",
						   sensordev.xname, sensor.desc);
					continue;
				} else {
					lid_found = TRUE;
					if (sensor.value)
						lid_open = TRUE;
					else
						lid_open = FALSE;
				}
			}
		}
	}

	up_daemon_set_lid_is_present (daemon, lid_found);
	up_daemon_set_lid_is_closed (daemon, !lid_open);
#endif
}

/* thread doing kqueue() on apm device */
static gpointer
up_backend_apm_event_thread(gpointer object)
{
#ifndef UPOWER_CI_DISABLE_PLATFORM_CODE
	int kq, nevents;
	struct kevent ev;
	struct timespec ts = {600, 0}, sts = {0, 0};

	UpBackend *backend;

	g_return_val_if_fail (UP_IS_BACKEND (object), NULL);
	backend = UP_BACKEND (object);

	g_debug("setting up apm thread");

	kq = kqueue();
	if (kq <= 0)
		g_error("kqueue");
	EV_SET(&ev, up_apm_get_fd(), EVFILT_READ, EV_ADD | EV_ENABLE | EV_CLEAR,
	    0, 0, NULL);
	nevents = 1;
	if (kevent(kq, &ev, nevents, NULL, 0, &sts) < 0)
		g_error("kevent");

	/* blocking wait on kqueue */
	for (;;) {
		int rv;

		/* 10mn timeout */
		sts = ts;
		if ((rv = kevent(kq, NULL, 0, &ev, 1, &sts)) < 0)
			break;
		if (!rv)
			continue;
		if (ev.ident == (guint) up_apm_get_fd() && APM_EVENT_TYPE(ev.data) == APM_POWER_CHANGE ) {
			/* g_idle_add the callback */
			g_idle_add((GSourceFunc) up_backend_apm_powerchange_event_cb, backend);
		}
	}
#endif
	return NULL;
	/* shouldn't be reached ? */
}

/**
 * GObject class functions
 **/

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
	if (backend->priv->battery != NULL)
		g_object_unref (backend->priv->battery);
	if (backend->priv->ac != NULL)
		g_object_unref (backend->priv->ac);
	g_clear_object (&backend->priv->seat_manager_proxy);
	/* XXX stop apm_thread ? */

	G_OBJECT_CLASS (up_backend_parent_class)->finalize (object);
}
