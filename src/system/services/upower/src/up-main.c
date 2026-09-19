/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2007 David Zeuthen <davidz@redhat.com>
 * Copyright (C) 2008-2009 Richard Hughes <richard@hughsie.com>
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

#include <string.h>
#include <signal.h>
#include <sys/types.h>
#include <unistd.h>

#include <gio/gio.h>
#include <glib.h>
#include <glib-unix.h>
#include <glib/gi18n-lib.h>
#include <glib-object.h>
#include <locale.h>

#include "up-daemon.h"

#define DEVKIT_POWER_SERVICE_NAME "org.freedesktop.UPower"

typedef struct UpState {
	UpDaemon *daemon;
	GMainLoop *loop;
} UpState;

static void
up_state_free (UpState *state)
{
	up_daemon_shutdown (state->daemon);

	g_clear_object (&state->daemon);
	g_clear_pointer (&state->loop, g_main_loop_unref);

	g_free (state);
}

static UpState *
up_state_new (void)
{
	UpState *state = g_new0 (UpState, 1);

	state->daemon = up_daemon_new ();
	state->loop = g_main_loop_new (NULL, FALSE);

	return state;
}

/**
 * up_main_bus_acquired:
 **/
static void
up_main_bus_acquired (GDBusConnection *connection,
		      const gchar *name,
		      gpointer user_data)
{
	UpState *state = user_data;

	if (!up_daemon_startup (state->daemon, connection)) {
		g_warning ("Could not startup; bailing out");
		g_main_loop_quit (state->loop);
	}
}

/**
 * up_main_name_lost:
 **/
static void
up_main_name_lost (GDBusConnection *connection,
		   const gchar *name,
		   gpointer user_data)
{
	UpState *state = user_data;
	g_debug ("name lost, exiting");
	g_main_loop_quit (state->loop);
}

/**
 * up_main_sigint_cb:
 **/
static gboolean
up_main_sigint_cb (gpointer user_data)
{
	UpState *state = user_data;
	g_debug ("Handling SIGINT");
	g_main_loop_quit (state->loop);
	return FALSE;
}

static gboolean
up_main_sigterm_cb (gpointer user_data)
{
	UpState *state = user_data;
	g_debug ("Handling SIGTERM");
	g_main_loop_quit (state->loop);
	return FALSE;
}

/**
 * up_main_timed_exit_cb:
 *
 * Exits the main loop, which is helpful for valgrinding.
 **/
static gboolean
up_main_timed_exit_cb (UpState *state)
{
	g_main_loop_quit (state->loop);
	return FALSE;
}

/**
 * up_main_log_ignore_cb:
 **/
static void
up_main_log_ignore_cb (const gchar *log_domain, GLogLevelFlags log_level,
		       const gchar *message, gpointer user_data)
{
}

/**
 * up_main_log_handler_cb:
 **/
static void
up_main_log_handler_cb (const gchar *log_domain, GLogLevelFlags log_level,
			const gchar *message, gpointer user_data)
{
	gchar str_time[255];
	time_t the_time;

	/* header always in green */
	time (&the_time);
	strftime (str_time, 254, "%H:%M:%S", localtime (&the_time));
	g_print ("%c[%dmTI:%s\t", 0x1B, 32, str_time);

	/* critical is also in red */
	if (log_level == G_LOG_LEVEL_CRITICAL ||
	    log_level == G_LOG_LEVEL_WARNING ||
	    log_level == G_LOG_LEVEL_ERROR) {
		g_print ("%c[%dm%s\n%c[%dm", 0x1B, 31, message, 0x1B, 0);
	} else {
		/* debug in blue */
		g_print ("%c[%dm%s\n%c[%dm", 0x1B, 34, message, 0x1B, 0);
	}
}

/**
 * main:
 **/
gint
main (gint argc, gchar **argv)
{
	GError *error = NULL;
	GOptionContext *context;
	gboolean timed_exit = FALSE;
	gboolean immediate_exit = FALSE;
	guint timer_id = 0;
	gboolean debug = FALSE;
	gboolean verbose = FALSE;
	UpState *state;
	GBusNameOwnerFlags bus_flags;
	gboolean replace = FALSE;

	const GOptionEntry options[] = {
		{ "timed-exit", '\0', 0, G_OPTION_ARG_NONE, &timed_exit,
		  /* TRANSLATORS: exit after we've started up, used for user profiling */
		  _("Exit after a small delay"), NULL },
		{ "immediate-exit", '\0', 0, G_OPTION_ARG_NONE, &immediate_exit,
		  /* TRANSLATORS: exit straight away, used for automatic profiling */
		  _("Exit after the engine has loaded"), NULL },
		{ "replace", 'r', 0, G_OPTION_ARG_NONE, &replace,
		  _("Replace the old daemon"), NULL},
		{ "verbose", 'v', 0, G_OPTION_ARG_NONE, &verbose,
		  _("Show extra debugging information"), NULL },
		{ "debug", 'd', 0, G_OPTION_ARG_NONE, &debug,
		  _("Enable debugging (implies --verbose)"), NULL },
		{ NULL}
	};

#if !defined(GLIB_VERSION_2_36)
	g_type_init ();
#endif
	setlocale(LC_ALL, "");

	context = g_option_context_new ("upower daemon");
	g_option_context_add_main_entries (context, options, NULL);
	if (!g_option_context_parse (context, &argc, &argv, &error)) {
		g_warning ("Failed to parse command-line options: %s", error->message);
		g_error_free (error);
		g_option_context_free (context);
		return 1;
	}
	g_option_context_free (context);

	if (debug)
		verbose = TRUE;

	/* verbose? */
	if (verbose) {
		g_log_set_fatal_mask (NULL, G_LOG_LEVEL_ERROR |
					    G_LOG_LEVEL_CRITICAL);
		g_log_set_handler (G_LOG_DOMAIN,
				   G_LOG_LEVEL_ERROR |
				   G_LOG_LEVEL_CRITICAL |
				   G_LOG_LEVEL_DEBUG |
				   G_LOG_LEVEL_WARNING,
				   up_main_log_handler_cb, NULL);
		g_log_set_handler ("UPower-Linux",
				   G_LOG_LEVEL_ERROR |
				   G_LOG_LEVEL_CRITICAL |
				   G_LOG_LEVEL_DEBUG |
				   G_LOG_LEVEL_WARNING,
				   up_main_log_handler_cb, NULL);
	} else {
		/* hide all debugging */
		g_log_set_fatal_mask (NULL, G_LOG_LEVEL_ERROR);
		g_log_set_handler (G_LOG_DOMAIN,
				   G_LOG_LEVEL_DEBUG,
				   up_main_log_ignore_cb,
				   NULL);
		g_log_set_handler ("UPower-Linux",
				   G_LOG_LEVEL_DEBUG,
				   up_main_log_ignore_cb,
				   NULL);
	}

	/* initialize state */
	state = up_state_new ();
	up_daemon_set_debug (state->daemon, debug);

	/* do stuff on ctrl-c */
	g_unix_signal_add_full (G_PRIORITY_DEFAULT,
				SIGINT,
				up_main_sigint_cb,
				state,
				NULL);

	/* Clean shutdown on SIGTERM */
	g_unix_signal_add_full (G_PRIORITY_DEFAULT,
				SIGTERM,
				up_main_sigterm_cb,
				state,
				NULL);

	/* acquire name */
	bus_flags = G_BUS_NAME_OWNER_FLAGS_ALLOW_REPLACEMENT;
	if (replace)
		bus_flags |= G_BUS_NAME_OWNER_FLAGS_REPLACE;
	g_bus_own_name (G_BUS_TYPE_SYSTEM,
			DEVKIT_POWER_SERVICE_NAME,
			bus_flags,
			up_main_bus_acquired,
			NULL,
			up_main_name_lost,
			state, NULL);

	g_debug ("Starting upowerd version %s", PACKAGE_VERSION);

	/* only timeout and close the mainloop if we have specified it on the command line */
	if (timed_exit) {
		timer_id = g_timeout_add_seconds (30, (GSourceFunc) up_main_timed_exit_cb, state);
		g_source_set_name_by_id (timer_id, "[upower] up_main_timed_exit_cb");
	}

	/* immediately exit */
	if (immediate_exit) {
		timer_id = g_timeout_add (50, (GSourceFunc) up_main_timed_exit_cb, state);
		g_source_set_name_by_id (timer_id, "[upower] up_main_timed_exit_cb");
	}

	/* wait for input or timeout */
	g_main_loop_run (state->loop);
	up_state_free (state);

	return 0;
}
