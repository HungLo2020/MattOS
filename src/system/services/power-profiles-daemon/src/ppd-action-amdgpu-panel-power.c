/*
 * Copyright (c) 2024 Advanced Micro Devices
 *
 * This program is free software; you can redistribute it and/or modify it
 * under the terms of the GNU General Public License version 3 as published by
 * the Free Software Foundation.
 *
 */

#define G_LOG_DOMAIN "AmdgpuPanel"

#include "config.h"

#include <gudev/gudev.h>

#include "ppd-action-amdgpu-panel-power.h"
#include "ppd-profile.h"
#include "ppd-utils.h"

#define PANEL_POWER_SYSFS_NAME "amdgpu/panel_power_savings"
#define PANEL_STATUS_SYSFS_NAME "status"

/**
 * SECTION:ppd-action-amdgpu-panel-power
 * @Short_description: Power savings for eDP connected displays
 * @Title: AMDGPU Panel power action
 *
 * The AMDGPU panel power action utilizes the sysfs attribute present on some DRM
 * connectors for amdgpu called "panel_power_savings".  This will use an AMD specific
 * hardware feature for a power savings profile for the panel.
 *
 */

struct _PpdActionAmdgpuPanelPower
{
  PpdAction  parent_instance;
  PpdProfile last_profile;

  GUdevClient *client;

  gint panel_power_saving;
  gboolean valid_battery;
  gboolean on_battery;
  gdouble battery_level;
};

G_DEFINE_TYPE (PpdActionAmdgpuPanelPower, ppd_action_amdgpu_panel_power, PPD_TYPE_ACTION)

static GObject*
ppd_action_amdgpu_panel_power_constructor (GType                  type,
                                           guint                  n_construct_params,
                                           GObjectConstructParam *construct_params)
{
  GObject *object;

  object = G_OBJECT_CLASS (ppd_action_amdgpu_panel_power_parent_class)->constructor (type,
                                                                                     n_construct_params,
                                                                                     construct_params);
  g_object_set (object,
                "action-name", "amdgpu_panel_power",
                "action-description", "Panel Power Savings (may affect color quality)",
                "action-optin", TRUE,
                NULL);

  return object;
}

static gboolean
panel_connected (GUdevDevice *device)
{
  const char *value;
  g_autofree gchar *stripped = NULL;

  value = g_udev_device_get_sysfs_attr_uncached (device, PANEL_STATUS_SYSFS_NAME);
  if (!value)
    return FALSE;
  stripped = g_strchomp (g_strdup (value));

  return g_strcmp0 (stripped, "connected") == 0;
}

static gboolean
set_panel_power (PpdActionAmdgpuPanelPower *self, gint power, GError **error)
{
  GList *devices, *l;

  devices = g_udev_client_query_by_subsystem (self->client, "drm");
  if (devices == NULL) {
    g_set_error_literal (error,
                         G_IO_ERROR,
                         G_IO_ERROR_NOT_FOUND,
                         "no drm devices found");
    return FALSE;
  }

  for (l = devices; l != NULL; l = l->next) {
    GUdevDevice *dev = l->data;
    const char *value;
    guint64 parsed;

    value = g_udev_device_get_devtype (dev);
    if (g_strcmp0 (value, "drm_connector") != 0)
      continue;

    if (!panel_connected (dev))
      continue;

    value = g_udev_device_get_sysfs_attr_uncached (dev, PANEL_POWER_SYSFS_NAME);
    if (!value)
      continue;

    parsed = g_ascii_strtoull (value, NULL, 10);

    /* overflow check */
    if (parsed == G_MAXUINT64) {
      g_set_error (error,
                   G_IO_ERROR,
                   G_IO_ERROR_INVALID_DATA,
                   "cannot parse %s as caused overflow",
                   value);
      return FALSE;
    }

    if (parsed == power)
      continue;

    if (!ppd_utils_write_sysfs_int (dev, PANEL_POWER_SYSFS_NAME, power, error))
      return FALSE;

    break;
  }

  g_list_free_full (devices, g_object_unref);

  return TRUE;
}

static gboolean
ppd_action_amdgpu_panel_update_target (PpdActionAmdgpuPanelPower  *self,
                                       GError                    **error)
{
  gint target = 0;

  /* only activate if we know that we're on battery */
  if (self->on_battery) {
    switch (self->last_profile) {
    case PPD_PROFILE_POWER_SAVER:
      if (!self->battery_level || self->battery_level >= 50)
        target = 0;
      else if (self->battery_level > 30)
        target = 1;
      else if (self->battery_level > 20 && self->battery_level <= 30)
        target = 2;
      else /* < 20 */
        target = 3;
      break;
    case PPD_PROFILE_BALANCED:
      if (!self->battery_level || self->battery_level >= 30)
        target = 0;
      else
        target = 1;
      break;
    case PPD_PROFILE_PERFORMANCE:
      target = 0;
      break;
    }
  }

  g_info("Updating panel to %d due to 🔋 %d (%f)", target, self->on_battery, self->battery_level);
  if (!set_panel_power (self, target, error))
    return FALSE;
  self->panel_power_saving = target;

  return TRUE;
}

static gboolean
ppd_action_amdgpu_panel_power_activate_profile (PpdAction   *action,
                                                PpdProfile   profile,
                                                GError     **error)
{
  PpdActionAmdgpuPanelPower *self = PPD_ACTION_AMDGPU_PANEL_POWER (action);
  self->last_profile = profile;

  if (!self->valid_battery) {
    g_debug ("upower not available; battery data might be stale");
    return TRUE;
  }

  return ppd_action_amdgpu_panel_update_target (self, error);
}

static gboolean
ppd_action_amdgpu_panel_power_power_changed (PpdAction             *action,
                                             PpdPowerChangedReason  reason,
                                             GError               **error)
{
  PpdActionAmdgpuPanelPower *self = PPD_ACTION_AMDGPU_PANEL_POWER (action);

  switch (reason) {
  case PPD_POWER_CHANGED_REASON_UNKNOWN:
    self->valid_battery = FALSE;
    return TRUE;
  case PPD_POWER_CHANGED_REASON_AC:
    self->on_battery = FALSE;
    break;
  case PPD_POWER_CHANGED_REASON_BATTERY:
    self->on_battery = TRUE;
    break;
  default:
    g_assert_not_reached ();
  }

  self->valid_battery = TRUE;

  return ppd_action_amdgpu_panel_update_target (self, error);
}

static gboolean
ppd_action_amdgpu_panel_power_battery_changed (PpdAction           *action,
                                               gdouble              val,
                                               GError             **error)
{
  PpdActionAmdgpuPanelPower *self = PPD_ACTION_AMDGPU_PANEL_POWER (action);

  self->battery_level = val;

  return ppd_action_amdgpu_panel_update_target (self, error);
}

static void
udev_uevent_cb (GUdevClient *client,
                gchar       *action,
                GUdevDevice *device,
                gpointer     user_data)
{
  PpdActionAmdgpuPanelPower *self = user_data;

  if (!g_str_equal (action, "add"))
    return;

  if (!g_udev_device_has_sysfs_attr (device, PANEL_POWER_SYSFS_NAME))
    return;

  if (!panel_connected (device))
      return;

  g_debug ("Updating panel power saving for '%s' to '%d'",
           g_udev_device_get_sysfs_path (device),
           self->panel_power_saving);
  ppd_utils_write_sysfs_int (device, PANEL_POWER_SYSFS_NAME,
                             self->panel_power_saving, NULL);
}

static PpdProbeResult
ppd_action_amdgpu_panel_power_probe (PpdAction *action)
{
  return ppd_utils_match_cpu_vendor ("AuthenticAMD") ? PPD_PROBE_RESULT_SUCCESS : PPD_PROBE_RESULT_FAIL;
}

static void
ppd_action_amdgpu_panel_power_finalize (GObject *object)
{
  PpdActionAmdgpuPanelPower *action;

  action = PPD_ACTION_AMDGPU_PANEL_POWER (object);
  g_clear_object (&action->client);
  G_OBJECT_CLASS (ppd_action_amdgpu_panel_power_parent_class)->finalize (object);
}

static void
ppd_action_amdgpu_panel_power_class_init (PpdActionAmdgpuPanelPowerClass *klass)
{
  GObjectClass *object_class;
  PpdActionClass *driver_class;

  object_class = G_OBJECT_CLASS(klass);
  object_class->constructor = ppd_action_amdgpu_panel_power_constructor;
  object_class->finalize = ppd_action_amdgpu_panel_power_finalize;

  driver_class = PPD_ACTION_CLASS(klass);
  driver_class->probe = ppd_action_amdgpu_panel_power_probe;
  driver_class->activate_profile = ppd_action_amdgpu_panel_power_activate_profile;
  driver_class->power_changed = ppd_action_amdgpu_panel_power_power_changed;
  driver_class->battery_changed = ppd_action_amdgpu_panel_power_battery_changed;
}

static void
ppd_action_amdgpu_panel_power_init (PpdActionAmdgpuPanelPower *self)
{
  const gchar * const subsystem[] = { "drm", NULL };

  self->client = g_udev_client_new (subsystem);
  g_signal_connect_object (G_OBJECT (self->client), "uevent",
                           G_CALLBACK (udev_uevent_cb), self, 0);
}
