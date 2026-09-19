/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2008 David Zeuthen <davidz@redhat.com>
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

#ifdef HAVE_CONFIG_H
#  include <config.h>
#endif

#include <glib.h>

#ifdef HAVE_POLKIT
#include <polkit/polkit.h>
#endif

#include "up-polkit.h"
#include "up-daemon.h"

#define UP_POLKIT_GET_PRIVATE(o) (G_TYPE_INSTANCE_GET_PRIVATE ((o), UP_TYPE_POLKIT, UpPolkitPrivate))

struct UpPolkitPrivate
{
	GDBusConnection		*connection;
#ifdef HAVE_POLKIT
	PolkitAuthority		*authority;
#endif
};

G_DEFINE_TYPE_WITH_PRIVATE (UpPolkit, up_polkit, G_TYPE_OBJECT)

#ifdef HAVE_POLKIT
/**
 * up_polkit_get_subject:
 **/
PolkitSubject *
up_polkit_get_subject (UpPolkit *polkit, GDBusMethodInvocation *invocation)
{
	g_autoptr (GError) error = NULL;
	const gchar *sender;
	g_autoptr (PolkitSubject) subject = NULL;

	sender = g_dbus_method_invocation_get_sender (invocation);
	subject = polkit_system_bus_name_new (sender);

	if (subject == NULL) {
		error = g_error_new (UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL, "failed to get PolicyKit subject");
		g_dbus_method_invocation_return_error (invocation,
						       UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL,
						       "failed to get Polkit subject: %s", error->message);
	}

	return g_steal_pointer (&subject);
}

/**
 * up_polkit_check_auth:
 **/
gboolean
up_polkit_check_auth (UpPolkit *polkit, PolkitSubject *subject, const gchar *action_id, GDBusMethodInvocation *invocation)
{
	g_autoptr (GError) error = NULL;
	g_autoptr (GError) error_local = NULL;
	g_autoptr (PolkitAuthorizationResult) result = NULL;

	/* check auth */
	result = polkit_authority_check_authorization_sync (polkit->priv->authority,
							    subject, action_id, NULL,
							    POLKIT_CHECK_AUTHORIZATION_FLAGS_ALLOW_USER_INTERACTION,
							    NULL, &error_local);
	if (result == NULL) {
		error = g_error_new (UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL, "failed to check authorization: %s", error_local->message);
		g_dbus_method_invocation_return_gerror (invocation, error);
		return FALSE;
	}

	/* okay? */
	if (polkit_authorization_result_get_is_authorized (result)) {
		return TRUE;
	} else {
		error = g_error_new (UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL, "not authorized");
		g_dbus_method_invocation_return_gerror (invocation, error);
	}
	return FALSE;
}

/**
 * up_polkit_is_allowed:
 **/
gboolean
up_polkit_is_allowed (UpPolkit *polkit, PolkitSubject *subject, const gchar *action_id, GError **error)
{
	gboolean ret = FALSE;
	g_autoptr (GError) error_local = NULL;
	g_autoptr (PolkitAuthorizationResult) result = NULL;

	/* check auth */
	result = polkit_authority_check_authorization_sync (polkit->priv->authority,
							    subject, action_id, NULL,
							    POLKIT_CHECK_AUTHORIZATION_FLAGS_NONE,
							    NULL, &error_local);
	if (result == NULL) {
		if (error_local != NULL)
			g_set_error (error, UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL, "%s", error_local->message);
		else
			g_set_error (error, UP_DAEMON_ERROR, UP_DAEMON_ERROR_GENERAL, "failed to check authorization");
		return FALSE;
	}

	ret = polkit_authorization_result_get_is_authorized (result) ||
	      polkit_authorization_result_get_is_challenge (result);

	return ret;
}
#endif

/**
 * up_polkit_finalize:
 **/
static void
up_polkit_finalize (GObject *object)
{
#ifdef HAVE_POLKIT
	UpPolkit *polkit;
	g_return_if_fail (UP_IS_POLKIT (object));
	polkit = UP_POLKIT (object);

	if (polkit->priv->connection != NULL)
		g_object_unref (polkit->priv->connection);

	g_object_unref (polkit->priv->authority);
#endif

	G_OBJECT_CLASS (up_polkit_parent_class)->finalize (object);
}

/**
 * up_polkit_class_init:
 **/
static void
up_polkit_class_init (UpPolkitClass *klass)
{
	GObjectClass *object_class = G_OBJECT_CLASS (klass);
	object_class->finalize = up_polkit_finalize;
}

/**
 * up_polkit_init:
 *
 * initializes the polkit class. NOTE: We expect polkit objects
 * to *NOT* be removed or added during the session.
 * We only control the first polkit object if there are more than one.
 **/
static void
up_polkit_init (UpPolkit *polkit)
{
	g_autoptr (GError) error = NULL;

	polkit->priv = up_polkit_get_instance_private (polkit);
#ifdef HAVE_POLKIT
	polkit->priv->authority = polkit_authority_get_sync (NULL, &error);
	if (polkit->priv->authority == NULL)
		g_error ("failed to get polkit authority: %s", error->message);
#endif
}

/**
 * up_polkit_new:
 * Return value: A new polkit class instance.
 **/
UpPolkit *
up_polkit_new (void)
{
	return UP_POLKIT (g_object_new (UP_TYPE_POLKIT, NULL));
}
