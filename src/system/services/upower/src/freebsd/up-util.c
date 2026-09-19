/***************************************************************************
 *
 * up-util.c : utilities
 *
 * Copyright (C) 2006, 2007 Jean-Yves Lefort <jylefort@FreeBSD.org>
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
 **************************************************************************/

#include "config.h"

#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <sys/types.h>
#ifndef UPOWER_CI_DISABLE_PLATFORM_CODE
#include <sys/sysctl.h>
#endif
#include <glib.h>

#include "up-util.h"

gboolean
up_has_sysctl (const gchar *format, ...)
{
#ifndef UPOWER_CI_DISABLE_PLATFORM_CODE
	va_list args;
	gchar *name;
	size_t value_len;
	gboolean status;

	g_return_val_if_fail (format != NULL, FALSE);

	va_start (args, format);
	name = g_strdup_vprintf (format, args);
	va_end (args);

	status = sysctlbyname (name, NULL, &value_len, NULL, 0) == 0;

	g_free (name);
	return status;
#else
	return FALSE;
#endif
}

gboolean
up_get_int_sysctl (int *value, GError **err, const gchar *format, ...)
{
#ifndef UPOWER_CI_DISABLE_PLATFORM_CODE
	va_list args;
	gchar *name;
	size_t value_len = sizeof(int);
	gboolean status;

	g_return_val_if_fail (value != NULL, FALSE);
	g_return_val_if_fail (format != NULL, FALSE);

	va_start (args, format);
	name = g_strdup_vprintf (format, args);
	va_end (args);

	status = sysctlbyname (name, value, &value_len, NULL, 0) == 0;
	if (!status)
		g_set_error(err, 0, 0, "%s", g_strerror (errno));

	g_free (name);
	return status;
#else
	return FALSE;
#endif
}

gchar *
up_get_string_sysctl (GError **err, const gchar *format, ...)
{
#ifndef UPOWER_CI_DISABLE_PLATFORM_CODE
	va_list args;
	gchar *name;
	size_t value_len;
	gchar *str = NULL;

	g_return_val_if_fail(format != NULL, FALSE);

	va_start (args, format);
	name = g_strdup_vprintf (format, args);
	va_end (args);

	if (sysctlbyname (name, NULL, &value_len, NULL, 0) == 0) {
		str = g_new (char, value_len + 1);
		if (sysctlbyname (name, str, &value_len, NULL, 0) == 0)
			str[value_len] = 0;
		else {
			g_free (str);
			str = NULL;
		}
	}

	if (!str)
		g_set_error (err, 0, 0, "%s", g_strerror(errno));

	g_free(name);
	return str;
#else
	return g_strdup ("asdf");
#endif
}
