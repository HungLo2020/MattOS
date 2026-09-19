/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2022 Benjamin Berg <bberg@redhat.com>
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

#pragma once

#include "up-device.h"

G_BEGIN_DECLS

#define MAX_DISCHARGE_RATE 300

#define UP_TYPE_DEVICE_BATTERY	(up_device_battery_get_type ())

G_DECLARE_DERIVABLE_TYPE (UpDeviceBattery, up_device_battery, UP, DEVICE_BATTERY, UpDevice)

struct _UpDeviceBatteryClass
{
	UpDeviceClass parent_class;

	gboolean	(* set_battery_charge_thresholds) (UpDevice  *device,
							   guint      start,
							   guint      end,
							   GError   **error);
};

typedef enum {
	UP_BATTERY_UNIT_UNDEFINED = 0,
	UP_BATTERY_UNIT_ENERGY,
	UP_BATTERY_UNIT_CHARGE,
} UpBatteryUnits;

typedef struct {
	gint64 ts_us;
	UpDeviceState state;
	UpBatteryUnits units;

	union {
		struct {
			gdouble cur;
			gdouble rate;
		} energy;
		struct {
			gdouble cur;
			gdouble rate;
		} charge;
	};
	gdouble percentage;
	gdouble voltage;
	gdouble temperature;
	gchar *capacity_level;
} UpBatteryValues;

typedef struct {
	gboolean present;

	const char *vendor;
	const char *model;
	const char *serial;

	UpBatteryUnits units;

	union {
		struct {
			gdouble full;
			gdouble design;
		} energy;
		struct {
			gdouble full;
			gdouble design;
		} charge;
	};

	UpDeviceTechnology technology;
	gdouble voltage_design;
	gint charge_cycles;
	gdouble voltage_max_design;
	gdouble voltage_min_design;

	/* battery charging threshold */
	gboolean charge_control_enabled;
	gboolean charge_control_supported;
	guint charge_control_start_threshold;
	guint charge_control_end_threshold;
	guint charge_threshold_settings;
} UpBatteryInfo;


void up_device_battery_update_info (UpDeviceBattery *self, UpBatteryInfo *info);
void up_device_battery_report (UpDeviceBattery *self, UpBatteryValues *values, UpRefreshReason reason);

G_END_DECLS
