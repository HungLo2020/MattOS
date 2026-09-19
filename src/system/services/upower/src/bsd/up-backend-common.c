/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2015 Eric Koegel <eric.koegel@gmail.com>
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

#include "up-backend-bsd-private.h"

static gboolean
check_action_result (GVariant *result)
{
	if (result) {
		const char *s;

		g_variant_get (result, "(s)", &s);
		if (g_strcmp0 (s, "yes") == 0)
			return TRUE;
	}
	return FALSE;
}

/**
 * functions called by upower daemon
 **/

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
		{ "HybridSleep", "CanHybridSleep" },
		{ "Hibernate", "CanHibernate" },
		{ "PowerOff", NULL },
	};
	guint i = 0;
	char *action;
	GDBusProxy *seat_manager_proxy = up_backend_get_seat_manager_proxy (backend);
	UpConfig *config = up_backend_get_config (backend);

	g_return_val_if_fail (seat_manager_proxy != NULL, NULL);
	g_return_val_if_fail (config != NULL, NULL);

	/* Find the configured action first */
	action = up_config_get_string (config, "CriticalPowerAction");
	if (action != NULL) {
		for (i = 0; i < G_N_ELEMENTS (actions); i++)
			if (g_str_equal (actions[i].method, action))
				break;
		if (i >= G_N_ELEMENTS (actions))
			i = 0;
		g_free (action);
	}

	for (; i < G_N_ELEMENTS (actions); i++) {
		GVariant *result;

		if (actions[i].can_method) {
			gboolean action_available;

			/* Check whether we can use the method */
			result = g_dbus_proxy_call_sync (seat_manager_proxy,
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
	const char *method;
	GDBusProxy *seat_manager_proxy = up_backend_get_seat_manager_proxy (backend);

	method = up_backend_get_critical_action (backend);
	g_assert (method != NULL);

	/* Take action */
	g_debug ("About to call ConsoleKit2 method %s", method);
	g_dbus_proxy_call (seat_manager_proxy,
			   method,
			   g_variant_new ("(b)", FALSE),
			   G_DBUS_CALL_FLAGS_NONE,
			   G_MAXINT,
			   NULL,
			   NULL,
			   NULL);
}

/**
 * up_backend_inhibitor_lock_take:
 * @backend: The %UpBackend class instance
 * @reason: Why the inhibitor lock is taken
 * @mode: The mode of the lock ('delay' or 'block')
 *
 * BSD does not support inhibitor locks so they are simply ignored.
 */
int
up_backend_inhibitor_lock_take (UpBackend  *backend,
                                const char *reason,
                                const char *mode)
{
    g_debug ("Taking inhibitor locks are not supported in BSD");
    return -1;
}
