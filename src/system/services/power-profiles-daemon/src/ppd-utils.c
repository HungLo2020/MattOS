/*
 * Copyright (c) 2020 Bastien Nocera <hadess@hadess.net>
 *
 * This program is free software; you can redistribute it and/or modify it
 * under the terms of the GNU General Public License version 3 as published by
 * the Free Software Foundation.
 *
 */

#define G_LOG_DOMAIN "Utils"

#include "ppd-utils.h"
#include <glib/gstdio.h>
#include <gio/gio.h>
#include <fcntl.h>
#include <stdio.h>
#include <errno.h>

#define PROC_CPUINFO_PATH      "/proc/cpuinfo"

char *
ppd_utils_get_sysfs_path (const char *filename)
{
  const char *root;

  root = g_getenv ("UMOCKDEV_DIR");
  if (!root || *root == '\0')
    root = "/";

  return g_build_filename (root, filename, NULL);
}

gboolean
ppd_utils_write (const char  *filename,
                 const char  *value,
                 GError     **error)
{
#if GLIB_CHECK_VERSION (2, 76, 0)
  g_autofd
#endif
  int fd = -1;
  size_t size;

  g_return_val_if_fail (filename, FALSE);
  g_return_val_if_fail (value, FALSE);

  g_debug ("Writing '%s' to '%s'", value, filename);

  fd = g_open (filename, O_WRONLY | O_TRUNC | O_SYNC);
  if (fd == -1) {
    g_set_error (error, G_IO_ERROR, g_io_error_from_errno (errno),
                 "Could not open '%s' for writing", filename);
    g_debug ("Could not open for writing '%s'", filename);
    return FALSE;
  }

  size = strlen (value);
  while (size) {
    ssize_t written = write (fd, value, size);

    if (written == -1) {
      g_set_error (error, G_IO_ERROR, g_io_error_from_errno (errno),
                   "Error writing '%s': %s", filename, g_strerror (errno));
      g_debug ("Error writing '%s': %s", filename, g_strerror (errno));
#if !GLIB_CHECK_VERSION (2, 76, 0)
      g_close (fd, NULL);
#endif
      return FALSE;
    }

    g_return_val_if_fail (written <= size, FALSE);
    size -= written;
  }

#if !GLIB_CHECK_VERSION (2, 76, 0)
  g_close (fd, NULL);
#endif

  return TRUE;
}

gboolean
ppd_utils_write_files (GPtrArray   *filenames,
                       const char  *value,
                       GError     **error)
{
  g_return_val_if_fail (filenames != NULL, FALSE);

  for (guint i = 0; i < filenames->len; i++) {
    const char *file = g_ptr_array_index (filenames, i);

    if (!ppd_utils_write (file, value, error))
      return FALSE;
  }

  return TRUE;
}

gboolean ppd_utils_write_sysfs (GUdevDevice  *device,
                                const char   *attribute,
                                const char   *value,
                                GError      **error)
{
  g_autofree char *filename = NULL;

  g_return_val_if_fail (G_UDEV_IS_DEVICE (device), FALSE);
  g_return_val_if_fail (attribute, FALSE);
  g_return_val_if_fail (value, FALSE);

  filename = g_build_filename (g_udev_device_get_sysfs_path (device), attribute, NULL);
  return ppd_utils_write (filename, value, error);
}

gboolean ppd_utils_write_sysfs_int (GUdevDevice  *device,
                                    const char   *attribute,
                                    gint64        value,
                                    GError      **error)
{
  g_autofree char *str_value = NULL;

  str_value = g_strdup_printf ("%" G_GINT64_FORMAT, value);
  return ppd_utils_write_sysfs (device, attribute, str_value, error);
}

GFileMonitor *
ppd_utils_monitor_sysfs_attr (GUdevDevice  *device,
                              const char   *attribute,
                              GError      **error)
{
  g_autofree char *path = NULL;
  g_autoptr(GFile) file = NULL;

  path = g_build_filename (g_udev_device_get_sysfs_path (device), attribute, NULL);
  file = g_file_new_for_path (path);
  g_debug ("Monitoring file %s for changes", path);
  return g_file_monitor_file (file,
                              G_FILE_MONITOR_NONE,
                              NULL,
                              error);
}

GUdevDevice *
ppd_utils_find_device (const char   *subsystem,
                       GCompareFunc  func,
                       gpointer      user_data)
{
  const gchar * subsystems[] = { NULL, NULL };
  g_autoptr(GUdevClient) client = NULL;
  GUdevDevice *ret = NULL;
  GList *devices, *l;

  g_return_val_if_fail (subsystem != NULL, NULL);
  g_return_val_if_fail (func != NULL, NULL);

  subsystems[0] = subsystem;
  client = g_udev_client_new (subsystems);
  devices = g_udev_client_query_by_subsystem (client, subsystem);
  if (devices == NULL)
    return NULL;

  for (l = devices; l != NULL; l = l->next) {
    GUdevDevice *dev = l->data;

    if ((func) (dev, user_data) != 0)
      continue;

    ret = g_object_ref (dev);
    break;
  }
  g_list_free_full (devices, g_object_unref);

  return ret;
}

gboolean
ppd_utils_match_cpu_vendor (const char *vendor)
{
  g_autofree gchar *cpuinfo_path = NULL;
  g_autofree gchar *cpuinfo = NULL;
  g_auto(GStrv) lines = NULL;

  cpuinfo_path = ppd_utils_get_sysfs_path (PROC_CPUINFO_PATH);
  if (!g_file_get_contents (cpuinfo_path, &cpuinfo, NULL, NULL))
    return FALSE;

  lines = g_strsplit (cpuinfo, "\n", -1);

  for (gchar **line = lines; *line != NULL; line++) {
      if (g_str_has_prefix (*line, "vendor_id") &&
          strchr (*line, ':')) {
          g_auto(GStrv) sections = g_strsplit (*line, ":", 2);

          if (g_strv_length (sections) < 2)
            continue;
          if (g_strcmp0 (g_strstrip (sections[1]), vendor) == 0)
            return TRUE;
      }
  }

  return FALSE;
}
