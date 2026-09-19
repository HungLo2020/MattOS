/*
 * Copyright (c) 2024 Advanced Micro Devices
 *
 * This program is free software; you can redistribute it and/or modify it
 * under the terms of the GNU General Public License version 3 as published by
 * the Free Software Foundation.
 *
 */

#define G_LOG_DOMAIN "AmdgpuDpm"

#include "config.h"

#include <gudev/gudev.h>

#include "ppd-action-amdgpu-dpm.h"
#include "ppd-profile.h"
#include "ppd-utils.h"

#define DPM_SYSFS_NAME "device/power_dpm_force_performance_level"

/**
 * SECTION:ppd-action-amdgpu-dpm
 * @Short_description: Power savings for GPU clocks
 * @Title: AMDGPU DPM clock control
 *
 * The AMDGPU DPM clock control action utilizes the sysfs attribute present on some DRM
 * connectors for amdgpu called "power_dpm_force_performance_level".
 */

struct _PpdActionAmdgpuDpm
{
  PpdAction  parent_instance;
  PpdProfile last_profile;

  GUdevClient *client;
};

G_DEFINE_TYPE (PpdActionAmdgpuDpm, ppd_action_amdgpu_dpm, PPD_TYPE_ACTION)

static GObject*
ppd_action_amdgpu_dpm_constructor (GType                  type,
                                   guint                  n_construct_params,
                                   GObjectConstructParam *construct_params)
{
  GObject *object;

  object = G_OBJECT_CLASS (ppd_action_amdgpu_dpm_parent_class)->constructor (type,
                                                                             n_construct_params,
                                                                             construct_params);
  g_object_set (object,
                "action-name", "amdgpu_dpm",
                "action-description", "Adjust GPU dynamic power management",
                "action-optin", TRUE,
                NULL);

  return object;
}

static gboolean
ppd_action_amdgpu_dpm_update_target (PpdActionAmdgpuDpm *self, GError **error)
{
  g_autolist (GUdevDevice) devices = NULL;
  const gchar *target;

  switch (self->last_profile) {
  case PPD_PROFILE_POWER_SAVER:
    target = "low";
    break;
  case PPD_PROFILE_BALANCED:
  case PPD_PROFILE_PERFORMANCE:
    target = "auto";
    break;
  default:
    g_assert_not_reached ();
  }

  devices = g_udev_client_query_by_subsystem (self->client, "drm");
  if (devices == NULL) {
    g_set_error_literal (error,
                         G_IO_ERROR,
                         G_IO_ERROR_NOT_FOUND,
                         "no drm devices found");
    return FALSE;
  }

  for (GList *l = devices; l != NULL; l = l->next) {
    GUdevDevice *dev = l->data;
    const char *value;

    value = g_udev_device_get_devtype (dev);
    if (g_strcmp0 (value, "drm_minor") != 0)
      continue;

    value = g_udev_device_get_sysfs_attr_uncached (dev, DPM_SYSFS_NAME);
    if (!value)
      continue;

    if (g_strcmp0 (value, target) == 0) {
      g_info ("Device %s already set to %s", g_udev_device_get_sysfs_path (dev), target);
      continue;
    }

    if (g_strcmp0 (value, "manual") == 0) {
      g_info ("Device %s is in manual mode, not changing", g_udev_device_get_sysfs_path (dev));
      continue;
    }

    g_info ("Setting device %s to %s", g_udev_device_get_sysfs_path (dev), target);
    if (!ppd_utils_write_sysfs (dev, DPM_SYSFS_NAME, target, error))
      return FALSE;
  }

  return TRUE;
}

static gboolean
ppd_action_amdgpu_dpm_activate_profile (PpdAction   *action,
                                        PpdProfile   profile,
                                        GError     **error)
{
  PpdActionAmdgpuDpm *self = PPD_ACTION_AMDGPU_DPM (action);
  self->last_profile = profile;

  return ppd_action_amdgpu_dpm_update_target (self, error);
}

static void
udev_uevent_cb (GUdevClient *client,
                gchar       *action,
                GUdevDevice *device,
                gpointer     user_data)
{
  PpdActionAmdgpuDpm *self = user_data;

  g_debug ("Device %s %s", g_udev_device_get_sysfs_path (device), action);

  if (!g_str_equal (action, "add"))
    return;

  if (!g_udev_device_has_sysfs_attr (device, DPM_SYSFS_NAME))
    return;

  ppd_action_amdgpu_dpm_update_target (self, NULL);
}

static PpdProbeResult
ppd_action_amdgpu_dpm_probe (PpdAction *action)
{
  return ppd_utils_match_cpu_vendor ("AuthenticAMD") ?
    PPD_PROBE_RESULT_SUCCESS : PPD_PROBE_RESULT_FAIL;
}

static void
ppd_action_amdgpu_dpm_finalize (GObject *object)
{
  PpdActionAmdgpuDpm *action;

  action = PPD_ACTION_AMDGPU_DPM (object);
  g_clear_object (&action->client);
  G_OBJECT_CLASS (ppd_action_amdgpu_dpm_parent_class)->finalize (object);
}

static void
ppd_action_amdgpu_dpm_class_init (PpdActionAmdgpuDpmClass *klass)
{
  GObjectClass *object_class;
  PpdActionClass *driver_class;

  object_class = G_OBJECT_CLASS(klass);
  object_class->constructor = ppd_action_amdgpu_dpm_constructor;
  object_class->finalize = ppd_action_amdgpu_dpm_finalize;

  driver_class = PPD_ACTION_CLASS(klass);
  driver_class->probe = ppd_action_amdgpu_dpm_probe;
  driver_class->activate_profile = ppd_action_amdgpu_dpm_activate_profile;
}

static void
ppd_action_amdgpu_dpm_init (PpdActionAmdgpuDpm *self)
{
  const gchar * const subsystem[] = { "drm", NULL };

  self->client = g_udev_client_new (subsystem);
  g_signal_connect_object (G_OBJECT (self->client), "uevent",
                           G_CALLBACK (udev_uevent_cb), self, 0);
}
