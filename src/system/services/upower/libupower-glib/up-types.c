/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2008 Richard Hughes <richard@hughsie.com>
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

/**
 * SECTION:up-types
 * @short_description: Types used by UPower and libupower-glib
 * @see_also: #UpClient, #UpDevice
 *
 * These helper functions provide a way to marshal enumerated values to
 * text and back again.
 */

#include "config.h"

#include <glib.h>

#include "up-types.h"

/**
 * up_device_kind_to_string:
 *
 * Converts a #UpDeviceKind to a string.
 *
 * Return value: identifier string
 *
 * Since: 0.9.0
 **/
const gchar *
up_device_kind_to_string (UpDeviceKind type_enum)
{
	switch (type_enum) {
	case UP_DEVICE_KIND_LINE_POWER:
		return "line-power";
	case UP_DEVICE_KIND_BATTERY:
		return "battery";
	case UP_DEVICE_KIND_UPS:
		return "ups";
	case UP_DEVICE_KIND_MONITOR:
		return "monitor";
	case UP_DEVICE_KIND_MOUSE:
		return "mouse";
	case UP_DEVICE_KIND_KEYBOARD:
		return "keyboard";
	case UP_DEVICE_KIND_PDA:
		return "pda";
	case UP_DEVICE_KIND_PHONE:
		return "phone";
	case UP_DEVICE_KIND_MEDIA_PLAYER:
		return "media-player";
	case UP_DEVICE_KIND_TABLET:
		return "tablet";
	case UP_DEVICE_KIND_COMPUTER:
		return "computer";
	case UP_DEVICE_KIND_GAMING_INPUT:
		return "gaming-input";
	case UP_DEVICE_KIND_PEN:
		return "pen";
	case UP_DEVICE_KIND_TOUCHPAD:
		return "touchpad";
	case UP_DEVICE_KIND_MODEM:
		return "modem";
	case UP_DEVICE_KIND_NETWORK:
		return "network";
	case UP_DEVICE_KIND_HEADSET:
		return "headset";
	case UP_DEVICE_KIND_SPEAKERS:
		return "speakers";
	case UP_DEVICE_KIND_HEADPHONES:
		return "headphones";
	case UP_DEVICE_KIND_VIDEO:
		return "video";
	case UP_DEVICE_KIND_OTHER_AUDIO:
		return "audio-device";
	case UP_DEVICE_KIND_REMOTE_CONTROL:
		return "remote-control";
	case UP_DEVICE_KIND_PRINTER:
		return "printer";
	case UP_DEVICE_KIND_SCANNER:
		return "scanner";
	case UP_DEVICE_KIND_CAMERA:
		return "camera";
	case UP_DEVICE_KIND_WEARABLE:
		return "wearable";
	case UP_DEVICE_KIND_TOY:
		return "toy";
	case UP_DEVICE_KIND_BLUETOOTH_GENERIC:
		return "bluetooth-generic";
	default:
		return "unknown";
	}
	g_assert_not_reached ();
}

/**
 * up_device_kind_from_string:
 *
 * Converts a string to a #UpDeviceKind.
 *
 * Return value: enumerated value
 *
 * Since: 0.9.0
 **/
UpDeviceKind
up_device_kind_from_string (const gchar *type)
{
	if (type == NULL)
		return UP_DEVICE_KIND_UNKNOWN;
	if (g_str_equal (type, "line-power"))
		return UP_DEVICE_KIND_LINE_POWER;
	if (g_str_equal (type, "battery"))
		return UP_DEVICE_KIND_BATTERY;
	if (g_str_equal (type, "ups"))
		return UP_DEVICE_KIND_UPS;
	if (g_str_equal (type, "monitor"))
		return UP_DEVICE_KIND_MONITOR;
	if (g_str_equal (type, "mouse"))
		return UP_DEVICE_KIND_MOUSE;
	if (g_str_equal (type, "keyboard"))
		return UP_DEVICE_KIND_KEYBOARD;
	if (g_str_equal (type, "pda"))
		return UP_DEVICE_KIND_PDA;
	if (g_str_equal (type, "phone"))
		return UP_DEVICE_KIND_PHONE;
	if (g_str_equal (type, "media-player"))
		return UP_DEVICE_KIND_MEDIA_PLAYER;
	if (g_str_equal (type, "tablet"))
		return UP_DEVICE_KIND_TABLET;
	if (g_str_equal (type, "gaming-input"))
		return UP_DEVICE_KIND_GAMING_INPUT;
	if (g_str_equal (type, "pen"))
		return UP_DEVICE_KIND_PEN;
	if (g_str_equal (type, "touchpad"))
		return UP_DEVICE_KIND_TOUCHPAD;
	if (g_str_equal (type, "modem"))
		return UP_DEVICE_KIND_MODEM;
	if (g_str_equal (type, "network"))
		return UP_DEVICE_KIND_NETWORK;
	if (g_str_equal (type, "headset"))
		return UP_DEVICE_KIND_HEADSET;
	if (g_str_equal (type, "speakers"))
		return UP_DEVICE_KIND_SPEAKERS;
	if (g_str_equal (type, "headphones"))
		return UP_DEVICE_KIND_HEADPHONES;
	if (g_str_equal (type, "video"))
		return UP_DEVICE_KIND_VIDEO;
	if (g_str_equal (type, "audio-device"))
		return UP_DEVICE_KIND_OTHER_AUDIO;
	if (g_str_equal (type, "remote-control"))
		return UP_DEVICE_KIND_REMOTE_CONTROL;
	if (g_str_equal (type, "printer"))
		return UP_DEVICE_KIND_PRINTER;
	if (g_str_equal (type, "scanner"))
		return UP_DEVICE_KIND_SCANNER;
	if (g_str_equal (type, "camera"))
		return UP_DEVICE_KIND_CAMERA;
	if (g_str_equal (type, "wearable"))
		return UP_DEVICE_KIND_WEARABLE;
	if (g_str_equal (type, "toy"))
		return UP_DEVICE_KIND_TOY;
	if (g_str_equal (type, "bluetooth-generic"))
		return UP_DEVICE_KIND_BLUETOOTH_GENERIC;
	return UP_DEVICE_KIND_UNKNOWN;
}

/**
 * up_device_state_to_string:
 *
 * Converts a #UpDeviceState to a string.
 *
 * Return value: identifier string
 *
 * Since: 0.9.0
 **/
const gchar *
up_device_state_to_string (UpDeviceState state_enum)
{
	switch (state_enum) {
	case UP_DEVICE_STATE_CHARGING:
		return "charging";
	case UP_DEVICE_STATE_DISCHARGING:
		return "discharging";
	case UP_DEVICE_STATE_EMPTY:
		return "empty";
	case UP_DEVICE_STATE_FULLY_CHARGED:
		return "fully-charged";
	case UP_DEVICE_STATE_PENDING_CHARGE:
		return "pending-charge";
	case UP_DEVICE_STATE_PENDING_DISCHARGE:
		return "pending-discharge";
	default:
		return "unknown";
	}
	g_assert_not_reached ();
}

/**
 * up_device_state_from_string:
 *
 * Converts a string to a #UpDeviceState.
 *
 * Return value: enumerated value
 *
 * Since: 0.9.0
 **/
UpDeviceState
up_device_state_from_string (const gchar *state)
{
	if (state == NULL)
		return UP_DEVICE_STATE_UNKNOWN;
	if (g_str_equal (state, "charging"))
		return UP_DEVICE_STATE_CHARGING;
	if (g_str_equal (state, "discharging"))
		return UP_DEVICE_STATE_DISCHARGING;
	if (g_str_equal (state, "empty"))
		return UP_DEVICE_STATE_EMPTY;
	if (g_str_equal (state, "fully-charged"))
		return UP_DEVICE_STATE_FULLY_CHARGED;
	if (g_str_equal (state, "pending-charge"))
		return UP_DEVICE_STATE_PENDING_CHARGE;
	if (g_str_equal (state, "pending-discharge"))
		return UP_DEVICE_STATE_PENDING_DISCHARGE;
	return UP_DEVICE_STATE_UNKNOWN;
}

/**
 * up_device_technology_to_string:
 *
 * Converts a #UpDeviceTechnology to a string.
 *
 * Return value: identifier string
 *
 * Since: 0.9.0
 **/
const gchar *
up_device_technology_to_string (UpDeviceTechnology technology_enum)
{
	switch (technology_enum) {
	case UP_DEVICE_TECHNOLOGY_LITHIUM_ION:
		return "lithium-ion";
	case UP_DEVICE_TECHNOLOGY_LITHIUM_POLYMER:
		return "lithium-polymer";
	case UP_DEVICE_TECHNOLOGY_LITHIUM_IRON_PHOSPHATE:
		return "lithium-iron-phosphate";
	case UP_DEVICE_TECHNOLOGY_LEAD_ACID:
		return "lead-acid";
	case UP_DEVICE_TECHNOLOGY_NICKEL_CADMIUM:
		return "nickel-cadmium";
	case UP_DEVICE_TECHNOLOGY_NICKEL_METAL_HYDRIDE:
		return "nickel-metal-hydride";
	default:
		return "unknown";
	}
	g_assert_not_reached ();
}

/**
 * up_device_technology_from_string:
 *
 * Converts a string to a #UpDeviceTechnology.
 *
 * Return value: enumerated value
 *
 * Since: 0.9.0
 **/
UpDeviceTechnology
up_device_technology_from_string (const gchar *technology)
{
	if (technology == NULL)
		return UP_DEVICE_TECHNOLOGY_UNKNOWN;
	if (g_str_equal (technology, "lithium-ion"))
		return UP_DEVICE_TECHNOLOGY_LITHIUM_ION;
	if (g_str_equal (technology, "lithium-polymer"))
		return UP_DEVICE_TECHNOLOGY_LITHIUM_POLYMER;
	if (g_str_equal (technology, "lithium-iron-phosphate"))
		return UP_DEVICE_TECHNOLOGY_LITHIUM_IRON_PHOSPHATE;
	if (g_str_equal (technology, "lead-acid"))
		return UP_DEVICE_TECHNOLOGY_LEAD_ACID;
	if (g_str_equal (technology, "nickel-cadmium"))
		return UP_DEVICE_TECHNOLOGY_NICKEL_CADMIUM;
	if (g_str_equal (technology, "nickel-metal-hydride"))
		return UP_DEVICE_TECHNOLOGY_NICKEL_METAL_HYDRIDE;
	return UP_DEVICE_TECHNOLOGY_UNKNOWN;
}

/**
 * up_device_level_to_string:
 *
 * Converts a #UpDeviceLevel to a string.
 *
 * Return value: identifier string
 *
 * Since: 1.0
 **/
const gchar *
up_device_level_to_string (UpDeviceLevel level_enum)
{
	switch (level_enum) {
	case UP_DEVICE_LEVEL_UNKNOWN:
		return "unknown";
	case UP_DEVICE_LEVEL_NONE:
		return "none";
	case UP_DEVICE_LEVEL_DISCHARGING:
		return "discharging";
	case UP_DEVICE_LEVEL_LOW:
		return "low";
	case UP_DEVICE_LEVEL_CRITICAL:
		return "critical";
	case UP_DEVICE_LEVEL_ACTION:
		return "action";
	case UP_DEVICE_LEVEL_NORMAL:
		return "normal";
	case UP_DEVICE_LEVEL_HIGH:
		return "high";
	case UP_DEVICE_LEVEL_FULL:
		return "full";
	default:
		return "unknown";
	}
	g_assert_not_reached ();
}

/**
 * up_device_level_from_string:
 *
 * Converts a string to a #UpDeviceLevel.
 *
 * Return value: enumerated value
 *
 * Since: 1.0
 **/
UpDeviceLevel
up_device_level_from_string (const gchar *level)
{
	if (level == NULL)
		return UP_DEVICE_LEVEL_UNKNOWN;
	if (g_str_equal (level, "unknown"))
		return UP_DEVICE_LEVEL_UNKNOWN;
	if (g_str_equal (level, "none"))
		return UP_DEVICE_LEVEL_NONE;
	if (g_str_equal (level, "discharging"))
		return UP_DEVICE_LEVEL_DISCHARGING;
	if (g_str_equal (level, "low"))
		return UP_DEVICE_LEVEL_LOW;
	if (g_str_equal (level, "critical"))
		return UP_DEVICE_LEVEL_CRITICAL;
	if (g_str_equal (level, "action"))
		return UP_DEVICE_LEVEL_ACTION;
	if (g_str_equal (level, "normal"))
		return UP_DEVICE_LEVEL_NORMAL;
	if (g_str_equal (level, "high"))
		return UP_DEVICE_LEVEL_HIGH;
	if (g_str_equal (level, "full"))
		return UP_DEVICE_LEVEL_FULL;
	return UP_DEVICE_LEVEL_UNKNOWN;
}
