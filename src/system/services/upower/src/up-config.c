/* -*- Mode: C; tab-width: 8; indent-tabs-mode: t; c-basic-offset: 8 -*-
 *
 * Copyright (C) 2011 Richard Hughes <richard@hughsie.com>
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

#include <glib-object.h>
#include <gio/gio.h>

#include "up-config.h"

static void     up_config_finalize	(GObject     *object);

/**
 * UpConfigPrivate:
 *
 * Private #UpConfig data
 **/
struct _UpConfigPrivate
{
	GKeyFile			*keyfile;
};

G_DEFINE_TYPE_WITH_PRIVATE (UpConfig, up_config, G_TYPE_OBJECT)

static gpointer up_config_object = NULL;

/**
 * up_config_get_boolean:
 **/
gboolean
up_config_get_boolean (UpConfig *config, const gchar *key)
{
	return g_key_file_get_boolean (config->priv->keyfile,
				       "UPower", key, NULL);
}

/**
 * up_config_get_uint:
 **/
guint
up_config_get_uint (UpConfig *config, const gchar *key)
{
	int val;

	val = g_key_file_get_integer (config->priv->keyfile,
				      "UPower", key, NULL);
	if (val < 0)
		return 0;

	return val;
}

/**
 * up_config_get_double:
 **/
gdouble
up_config_get_double (UpConfig *config, const gchar *key)
{
	int val;

	val = g_key_file_get_double (config->priv->keyfile,
				     "UPower", key, NULL);
	if (val < 0.0)
		return 0.0;

	return val;
}

/**
 * up_config_get_string:
 **/
gchar *
up_config_get_string (UpConfig *config, const gchar *key)
{
	return g_key_file_get_string (config->priv->keyfile,
				      "UPower", key, NULL);
}

/**
 * up_config_class_init:
 **/
static void
up_config_class_init (UpConfigClass *klass)
{
	GObjectClass *object_class = G_OBJECT_CLASS (klass);
	object_class->finalize = up_config_finalize;
}

/**
 * up_config_list_compare_files:
 **/
static gint
up_config_list_compare_files (gconstpointer a, gconstpointer b)
{
	return g_strcmp0 ((const gchar*)a, (const gchar*)b);
}

/**
 * up_config_list_confd_files:
 *
 * The format of the filename should be '^([0-9][0-9])-([a-zA-Z0-9-_])*\.conf$',
 * that is, starting with "00-" to "99-", ending in ".conf", and with a mix of
 * alphanumeric characters with dashes and underscores in between. For example:
 * '01-upower-override.conf'.
 *
 * Files named differently, or containing invalid groups (currently only
 * 'UPower' is valid), will not be considered.
 *
 * The candidate files within the given directory are sorted (with g_strcmp0(),
 * so the ordering will be as with strcmp()). The configuration in the files
 * being processed later will override previous config, in particular the main
 * config, but also the one from previous files processed, if the Group and Key
 * coincide.
 *
 * For example, consider 'UPower.conf' that contains the defaults:
 *   PercentageLow=20.0
 *   PercentageCritical=5.0
 *   PercentageAction=2.0
 *
 * and there is a file 'UPower.conf.d/70-change-percentages.conf'
 * containing settings for all 'Percentage*' keys:
 *   [UPower]
 *   PercentageLow=15.0
 *   PercentageCritical=10.0
 *   PercentageAction=5.0
 *
 * and another 'UPower.conf.d/99-change-percentages-local.conf'
 * containing settings only for 'PercentageAction':
 *   [UPower]
 *   PercentageAction=7.5
 *
 * First the main 'UPower.conf' will be processed, then
 * 'UPower.conf.d/70-change-percentages.conf' overriding the defaults
 * of all percentages from the main config file with the given values,
 * and finally 'UPower.conf.d/99-change-percentages-local.conf'
 * overriding once again only 'PercentageAction'. The final, effective
 * values are:
 *   PercentageLow=15.0
 *   PercentageCritical=10.0
 *   PercentageAction=7.5
 **/
static GPtrArray*
up_config_list_confd_files (const gchar* conf_d_path, GError** error)
{
	g_autoptr (GPtrArray) ret_conf_d_files = NULL;
	GDir *dir = NULL;
	const gchar *filename = NULL;
	const char *regex_pattern = "^([0-9][0-9])-([a-zA-Z0-9-_])*\\.conf$";
	g_autoptr (GRegex) regex = NULL;

	dir = g_dir_open (conf_d_path, 0, error);
	if (dir == NULL)
		return NULL;

	regex = g_regex_new (regex_pattern, G_REGEX_DEFAULT, G_REGEX_MATCH_DEFAULT, NULL);
	g_assert (regex != NULL);

	ret_conf_d_files = g_ptr_array_new_full (0, g_free);

	while ((filename = g_dir_read_name (dir)) != NULL) {
		g_autofree gchar *file_path = NULL;
		g_autoptr (GFile) file = NULL;
		g_autoptr (GFileInfo) file_info = NULL;

		if (!g_regex_match (regex, filename, G_REGEX_MATCH_DEFAULT, NULL))
			continue;

		file_path = g_build_filename (conf_d_path, filename, NULL);
		file = g_file_new_for_path (file_path);
		file_info = g_file_query_info (file,
					       G_FILE_ATTRIBUTE_STANDARD_TYPE,
					       G_FILE_QUERY_INFO_NOFOLLOW_SYMLINKS,
					       NULL,
					       NULL);
		if (file_info != NULL) {
			g_debug ("Will consider additional config file '%s'", file_path);
			g_ptr_array_add (ret_conf_d_files, g_strdup (file_path));
		}
	}

	g_dir_close (dir);

	g_ptr_array_sort_values (ret_conf_d_files, up_config_list_compare_files);

	return g_ptr_array_ref (ret_conf_d_files);
}

/**
 * up_config_override_from_confd:
 **/
static void
up_config_override_from_confd (GKeyFile *key_file, const gchar* new_config_path)
{
	g_autoptr (GKeyFile) new_keyfile = NULL;
	gchar **keys = NULL;
	gsize keys_size = 0;

	new_keyfile = g_key_file_new();
	if (!g_key_file_load_from_file (new_keyfile, new_config_path, G_KEY_FILE_NONE, NULL))
		return;

	if (!g_key_file_has_group (new_keyfile, "UPower"))
		return;

	keys = g_key_file_get_keys (new_keyfile, "UPower", &keys_size, NULL);
	if (keys == NULL)
		return;

	for (gsize i = 0; i < keys_size; i++) {
		g_autofree gchar *value = NULL;
		g_autofree gchar *old_value = NULL;

		value = g_key_file_get_value (new_keyfile, "UPower", keys[i], NULL);
		if (value == NULL)
			continue;

		old_value = g_key_file_get_value (key_file, "UPower", keys[i], NULL);

		if (old_value != NULL)
			g_key_file_set_value (key_file, "UPower", keys[i], value);
	}
	g_strfreev (keys);
}

/**
 * up_config_init:
 **/
static void
up_config_init (UpConfig *config)
{
	gboolean allow_risky_critical_action = FALSE;
	gboolean expect_battery_recalibration = FALSE;
	g_autofree gchar *critical_action = NULL;
	g_autoptr (GError) error = NULL;
	g_autofree gchar *filename = NULL;
	gboolean ret;
	g_autofree gchar *conf_dir = NULL;
	g_autofree gchar *conf_d_path = NULL;
	g_autoptr (GPtrArray) conf_d_files = NULL;

	config->priv = up_config_get_instance_private (config);
	config->priv->keyfile = g_key_file_new ();

	filename = g_strdup (g_getenv ("UPOWER_CONF_FILE_NAME"));
	if (filename == NULL) {
		filename = g_build_filename (PACKAGE_SYSCONF_DIR,"UPower", "UPower.conf", NULL);
		conf_d_path = g_build_filename (PACKAGE_SYSCONF_DIR, "UPower", "UPower.conf.d", NULL);
	} else {
		conf_dir = g_path_get_dirname (filename);
		conf_d_path = g_build_filename (conf_dir, "UPower.conf.d", NULL);
	}

	/* load */
	ret = g_key_file_load_from_file (config->priv->keyfile,
					 filename,
					 G_KEY_FILE_NONE,
					 &error);

	if (!ret) {
		g_warning ("failed to load config file '%s': %s",
			   filename, error->message);
		g_clear_error (&error);
	}

	conf_d_files = up_config_list_confd_files (conf_d_path, &error);
	if (conf_d_files != NULL) {
		for (guint i = 0; i < conf_d_files->len; i++) {
			const gchar* conf_d_file = (const gchar*)(g_ptr_array_index (conf_d_files, i));
			up_config_override_from_confd (config->priv->keyfile,
						       conf_d_file);
		}
	} else {
		g_debug ("failed to find files in 'UPower.conf.d': %s", error->message);
	}

	/* Warn for any dangerous configurations */
	critical_action = up_config_get_string (config, "CriticalPowerAction");
	allow_risky_critical_action = up_config_get_boolean (config, "AllowRiskyCriticalPowerAction");

	if (!g_strcmp0 (critical_action, "Suspend") || !g_strcmp0 (critical_action, "Ignore")) {
		if (allow_risky_critical_action) {
			g_warning ("The \"%s\" CriticalPowerAction setting is considered risky:"
				   " abrupt power loss due to battery exhaustion may lead to data"
				   " corruption. Use AllowRiskyCriticalPowerAction=false to disable"
				   " support for risky settings.", critical_action);
		} else {
			g_warning ("The \"%s\" CriticalPowerAction setting is considered risky:"
				   " abrupt power loss due to battery exhaustion may lead to data"
				   " corruption. The system will perform \"HybridSleep\" instead."
				   " Use AllowRiskyCriticalPowerAction=true to enable support for"
				   " risky settings.", critical_action);
		}
	}

	expect_battery_recalibration = up_config_get_boolean (config, "ExpectBatteryRecalibration");
	if (expect_battery_recalibration) {
		if (allow_risky_critical_action) {
			g_warning ("The \"ExpectBatteryRecalibration\" setting is considered risky:"
				   " abrupt power loss due to battery exhaustion may lead to data"
				   " corruption. The system will unexpected down when the AC is disconnected."
				   " Use AllowRiskyCriticalPowerAction=false to disable support for"
				   " risky settings.");
		}
	}
}

/**
 * up_config_finalize:
 **/
static void
up_config_finalize (GObject *object)
{
	UpConfig *config = UP_CONFIG (object);
	UpConfigPrivate *priv = config->priv;

	g_key_file_free (priv->keyfile);

	G_OBJECT_CLASS (up_config_parent_class)->finalize (object);
}

/**
 * up_config_new:
 **/
UpConfig *
up_config_new (void)
{
	if (up_config_object != NULL) {
		g_object_ref (up_config_object);
	} else {
		up_config_object = g_object_new (UP_TYPE_CONFIG, NULL);
		g_object_add_weak_pointer (up_config_object, &up_config_object);
	}
	return UP_CONFIG (up_config_object);
}
