/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2007 David Zeuthen <davidz@redhat.com>
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

#include <stdlib.h>
#include <stdio.h>
#include <unistd.h>
#include <signal.h>
#include <sys/time.h>
#include <glib.h>
#include <glib/gi18n-lib.h>
#include <glib-unix.h>
#include <locale.h>

#include "upower.h"

static gboolean opt_monitor_detail = FALSE;

/**
 * up_tool_get_timestamp:
 **/
static gchar *
up_tool_get_timestamp (void)
{
	g_autofree gchar *str_time = NULL;
	g_autofree gchar *timestamp = NULL;
	time_t the_time;
	struct timeval time_val;

	time (&the_time);
	gettimeofday (&time_val, NULL);
	str_time = g_new0 (gchar, 255);
	strftime (str_time, 254, "%H:%M:%S", localtime (&the_time));

	/* generate header text */
	timestamp = g_strdup_printf ("%s.%03i", str_time, (gint) time_val.tv_usec / 1000);
	return g_steal_pointer (&timestamp);
}

/**
 * up_tool_device_added_cb:
 **/
static void
up_tool_device_added_cb (UpClient *client, UpDevice *device, gpointer user_data)
{
	g_autofree gchar *timestamp = NULL;
	g_autofree gchar *text = NULL;
	const gchar *object_path;

	timestamp = up_tool_get_timestamp ();
	object_path = up_device_get_object_path (device);
	g_print ("[%s]\tdevice added:     %s\n", timestamp, object_path ? object_path : "unknown");
	if (opt_monitor_detail) {
		text = up_device_to_text (device);
		g_print ("%s\n", text ? text : "unknown");
	}
}

/**
 * up_tool_device_changed_cb:
 **/
static void
up_tool_device_changed_cb (UpDevice *device, GParamSpec *pspec, gpointer user_data)
{
	g_autofree gchar *timestamp = NULL;
	g_autofree gchar *text = NULL;
	const gchar *object_path;

	timestamp = up_tool_get_timestamp ();
	object_path = up_device_get_object_path (device);
	g_print ("[%s]\tdevice changed:     %s\n", timestamp, object_path ? object_path : "unknown");
	if (opt_monitor_detail) {
		/* TODO: would be nice to just show the diff */
		text = up_device_to_text (device);
		g_print ("%s\n", text ? text : "unknown");
	}
}

/**
 * up_tool_device_removed_cb:
 **/
static void
up_tool_device_removed_cb (UpClient *client, const char *object_path, gpointer user_data)
{
	g_autofree gchar *timestamp = NULL;

	if (object_path == NULL) {
		g_print ("Object path is NULL\n");
		return;
	}

	timestamp = up_tool_get_timestamp ();
	g_print ("[%s]\tdevice removed:   %s\n", timestamp, object_path);
	if (opt_monitor_detail)
		g_print ("\n");
}

/**
 * up_client_print:
 **/
static void
up_client_print (UpClient *client)
{
	g_autofree gchar *daemon_version = NULL;
	gboolean on_battery;
	gboolean lid_is_closed;
	gboolean lid_is_present;
	g_autofree gchar *action = NULL;

	g_object_get (client,
		      "daemon-version", &daemon_version,
		      "on-battery", &on_battery,
		      "lid-is-closed", &lid_is_closed,
		      "lid-is-present", &lid_is_present,
		      NULL);

	g_print ("  daemon-version:  %s\n", daemon_version ? daemon_version : "unknown");
	g_print ("  on-battery:      %s\n", on_battery ? "yes" : "no");
	g_print ("  lid-is-closed:   %s\n", lid_is_closed ? "yes" : "no");
	g_print ("  lid-is-present:  %s\n", lid_is_present ? "yes" : "no");
	action = up_client_get_critical_action (client);
	g_print ("  critical-action: %s\n", action ? action : "unknown");
}

/**
 * up_tool_changed_cb:
 **/
static void
up_tool_changed_cb (UpClient *client, GParamSpec *pspec, gpointer user_data)
{
	g_autofree gchar *timestamp = NULL;

	timestamp = up_tool_get_timestamp ();
	g_print ("[%s]\tdaemon changed:\n", timestamp);
	if (opt_monitor_detail) {
		up_client_print (client);
		g_print ("\n");
	}
}

/**
 * up_tool_do_monitor:
 **/
static gboolean
up_tool_do_monitor (UpClient *client, GMainLoop *loop)
{
	g_autoptr (GPtrArray) devices = NULL;
	guint i;

	g_print ("Monitoring activity from the power daemon. Press Ctrl+C to cancel.\n");

	g_signal_connect (client, "device-added", G_CALLBACK (up_tool_device_added_cb), NULL);
	g_signal_connect (client, "device-removed", G_CALLBACK (up_tool_device_removed_cb), NULL);
	g_signal_connect (client, "notify", G_CALLBACK (up_tool_changed_cb), NULL);

	devices = up_client_get_devices2 (client);
	if (devices == NULL)
		return FALSE;
	for (i=0; i < devices->len; i++) {
		UpDevice *device;
		device = g_ptr_array_index (devices, i);
		g_signal_connect (device, "notify", G_CALLBACK (up_tool_device_changed_cb), NULL);
	}

	g_main_loop_run (loop);

	return TRUE;
}

static void
up_tool_output_daemon (UpClient *client)
{
	g_print ("Daemon:\n");
	up_client_print (client);
}

static gint
up_tool_output_display_device (UpClient *client)
{
	g_autoptr (UpDevice) device = NULL;
	g_autofree gchar *text = NULL;
	const gchar *object_path;

	device = up_client_get_display_device (client);
	if (!device) {
		g_print ("Failed to get display device\n");
		return 1;
	}

	object_path = up_device_get_object_path (device);
	g_print ("Device: %s\n", object_path ? object_path : "unknown");
	text = up_device_to_text (device);
	g_print ("%s\n", text ? text : "unknown");

	return 0;
}

static gint
up_tool_output_device_dump (UpClient *client, GList *device_filter)
{
	g_autoptr (GPtrArray) devices = NULL;
	UpDevice *device;
	const gchar *object_path;
	guint i;
	guint kind = 0;
	gint ret = 0;
	gchar *text = NULL;

	devices = up_client_get_devices2 (client);
	if (devices == NULL) {
		g_print ("Failed to get devices\n");
		return 1;
	}

	for (i=0; i < devices->len; i++) {
		device = (UpDevice*) g_ptr_array_index (devices, i);
		g_object_get (device, "kind", &kind, NULL);
		if (g_list_find (device_filter, GINT_TO_POINTER (kind)) || device_filter == NULL) {
			object_path = up_device_get_object_path (device);
			g_print ("Device: %s\n", object_path ? object_path : "unknown");
			text = up_device_to_text (device);
			g_print ("%s\n", text ? text : "unknown");
			g_free (text);
		}
	}

	if (device_filter == NULL) {
		ret = up_tool_output_display_device (client);
		up_tool_output_daemon (client);
	}

	return ret;
}

static gint
up_tool_output_enumerate (UpClient *client)
{
	g_autoptr (GPtrArray) devices = NULL;
	g_autoptr (UpDevice) display_device = NULL;
	UpDevice *device;
	const gchar *object_path;
	guint i;

	devices = up_client_get_devices2 (client);
	if (devices == NULL) {
		g_print ("Failed to get devices\n");
		return 1;
	}
	for (i = 0; i < devices->len; i++) {
		device = (UpDevice*) g_ptr_array_index (devices, i);
		object_path = up_device_get_object_path (device);
		g_print ("%s\n", object_path ? object_path : "unknown");
	}

	display_device = up_client_get_display_device (client);
	if (display_device == NULL) {
		g_print ("Failed to get display device\n");
		return 1;
	}
	object_path = up_device_get_object_path (display_device);
	g_print ("%s\n", object_path ? object_path : "unknown");
	return 0;
}

static gboolean
up_tool_signal_cb (gpointer user_data)
{
	GMainLoop *loop = user_data;

	g_debug ("Caught signal, exiting");
	g_main_loop_quit (loop);
	return G_SOURCE_REMOVE;
}

/**
 * main:
 **/
int
main (int argc, char **argv)
{
	g_autoptr (GMainLoop) loop = NULL;
	GOptionContext *context;
	gboolean opt_battery = FALSE;
	gboolean opt_dump = FALSE;
	gboolean opt_enumerate = FALSE;
	gboolean opt_monitor = FALSE;
	g_autofree gchar *opt_show_info = NULL;
	gboolean opt_version = FALSE;
	GList *device_filter = NULL;
	gboolean ret;
	gint retval;
	GError *error = NULL;
	g_autofree gchar *text = NULL;

	g_autoptr (UpClient) client = NULL;
	g_autoptr (UpDevice) device = NULL;

	const GOptionEntry entries[] = {
		{ "battery", 'b', 0, G_OPTION_ARG_NONE, &opt_battery, _("Dump all parameters for battery objects"), NULL },
		{ "dump", 'd', 0, G_OPTION_ARG_NONE, &opt_dump, _("Dump all parameters for all objects"), NULL },
		{ "enumerate", 'e', 0, G_OPTION_ARG_NONE, &opt_enumerate, _("Enumerate objects paths for devices"), NULL },
		{ "monitor", 'm', 0, G_OPTION_ARG_NONE, &opt_monitor, _("Monitor activity from the power daemon"), NULL },
		{ "monitor-detail", 0, 0, G_OPTION_ARG_NONE, &opt_monitor_detail, _("Monitor with detail"), NULL },
		{ "show-info", 'i', 0, G_OPTION_ARG_STRING, &opt_show_info, _("Show information about object path"), NULL },
		{ "version", 'v', 0, G_OPTION_ARG_NONE, &opt_version, "Print version of client and daemon", NULL },
		{ NULL }
	};

#if !defined(GLIB_VERSION_2_36)
	g_type_init ();
#endif
	setlocale(LC_ALL, "");

	context = g_option_context_new ("UPower tool");
	g_option_context_add_main_entries (context, entries, NULL);
	ret = g_option_context_parse (context, &argc, &argv, &error);
	g_option_context_free (context);
	if (!ret) {
		g_print ("Failed to parse command-line options: %s\n", error->message);
		g_error_free (error);
		return EXIT_FAILURE;
	}

	client = up_client_new_full (NULL, &error);
	if (client == NULL) {
		g_warning ("Cannot connect to upowerd: %s", error->message);
		g_error_free (error);
		return EXIT_FAILURE;
	}

	if (opt_version) {
		g_autofree gchar *daemon_version = NULL;
		g_object_get (client,
			      "daemon-version", &daemon_version,
			      NULL);
		g_print ("UPower client version %s\n"
			 "UPower daemon version %s\n",
			 PACKAGE_VERSION, daemon_version ? daemon_version : "unknown");
		return EXIT_SUCCESS;
	}

	if (opt_enumerate)
		return up_tool_output_enumerate (client);

	if (opt_battery) {
		device_filter = g_list_append (device_filter, GINT_TO_POINTER (UP_DEVICE_KIND_BATTERY));
		opt_dump = TRUE;
	}

	if (opt_dump) {
		retval = up_tool_output_device_dump (client, device_filter);
		g_list_free (device_filter);
		return retval;
	}

	if (opt_monitor || opt_monitor_detail) {
		loop = g_main_loop_new (NULL, FALSE);

		g_unix_signal_add_full (G_PRIORITY_DEFAULT,
					SIGINT,
					up_tool_signal_cb,
					loop,
					NULL);

		g_unix_signal_add_full (G_PRIORITY_DEFAULT,
					SIGTERM,
					up_tool_signal_cb,
					loop,
					NULL);

		if (!up_tool_do_monitor (client, loop))
			return EXIT_FAILURE;
		return EXIT_SUCCESS;
	}

	if (opt_show_info != NULL) {
		device = up_device_new ();
		ret = up_device_set_object_path_sync (device, opt_show_info, NULL, &error);
		if (!ret) {
			g_print ("failed to set path: %s\n", error->message);
			g_error_free (error);
		} else {
			text = up_device_to_text (device);
			g_print ("%s\n", text ? text : "unknown");
		}
		return EXIT_SUCCESS;
	}

	return EXIT_SUCCESS;
}
