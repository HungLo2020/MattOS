/*
 * Copyright (c) 2020 Bastien Nocera <hadess@hadess.net>
 * Copyright (c) 2022 Prajna Sariputra <putr4.s@gmail.com>
 *
 * This program is free software; you can redistribute it and/or modify it
 * under the terms of the GNU General Public License version 3 as published by
 * the Free Software Foundation.
 *
 */

#define G_LOG_DOMAIN "CpuDriver"

#include <upower.h>

#include "ppd-utils.h"
#include "ppd-driver-amd-pstate.h"

#define CPUFREQ_POLICY_DIR "/sys/devices/system/cpu/cpufreq/"
#define PSTATE_STATUS_PATH "/sys/devices/system/cpu/amd_pstate/status"

struct _PpdDriverAmdPstate
{
  PpdDriverCpu  parent_instance;

  PpdProfile activated_profile;
  GPtrArray *epp_devices; /* Array of paths */
  gboolean on_battery;
};

G_DEFINE_TYPE (PpdDriverAmdPstate, ppd_driver_amd_pstate, PPD_TYPE_DRIVER_CPU)

static gboolean ppd_driver_amd_pstate_activate_profile (PpdDriver                   *driver,
                                                        PpdProfile                   profile,
                                                        PpdProfileActivationReason   reason,
                                                        GError                     **error);

static GObject*
ppd_driver_amd_pstate_constructor (GType                  type,
                                   guint                  n_construct_params,
                                   GObjectConstructParam *construct_params)
{
  GObject *object;

  object = G_OBJECT_CLASS (ppd_driver_amd_pstate_parent_class)->constructor (type,
                                                                             n_construct_params,
                                                                             construct_params);
  g_object_set (object,
                "driver-name", "amd_pstate",
                "profiles", PPD_PROFILE_PERFORMANCE | PPD_PROFILE_BALANCED | PPD_PROFILE_POWER_SAVER,
                NULL);

  return object;
}

static PpdProbeResult
probe_epp (PpdDriverAmdPstate *pstate)
{
  g_autoptr(GDir) dir = NULL;
  g_autofree char *policy_dir = NULL;
  g_autofree char *pstate_status_path = NULL;
  const char *dirname;

  /* Verify that AMD P-State is running in active mode */
  pstate_status_path = ppd_utils_get_sysfs_path (PSTATE_STATUS_PATH);
  if (!g_file_test (pstate_status_path, G_FILE_TEST_EXISTS))
    return PPD_PROBE_RESULT_FAIL;

  policy_dir = ppd_utils_get_sysfs_path (CPUFREQ_POLICY_DIR);
  dir = g_dir_open (policy_dir, 0, NULL);
  if (!dir) {
    g_debug ("Could not open %s", policy_dir);
    return PPD_PROBE_RESULT_FAIL;
  }

  while ((dirname = g_dir_read_name (dir)) != NULL) {
    g_autofree char *base = NULL;
    g_autofree char *path = NULL;
    g_autofree char *contents = NULL;
    g_autoptr(GError) error = NULL;

    base = g_build_filename (policy_dir,
                             dirname,
                             NULL);

    path = g_build_filename (base,
                             "energy_performance_preference",
                             NULL);
    if (!g_file_test (path, G_FILE_TEST_EXISTS))
      continue;

    if (!g_file_get_contents (path, &contents, NULL, &error)) {
      g_debug ("Failed to read %s: %s", path, error->message);
      continue;
    }
    if (!ppd_utils_write (path, g_strchomp (contents), &error)) {
      g_debug ("Failed to write %s: %s", path, error->message);
      continue;
    }

    if (!pstate->epp_devices)
      pstate->epp_devices = g_ptr_array_new_with_free_func (g_free);

    g_ptr_array_add (pstate->epp_devices, g_steal_pointer (&base));
  }

  if (pstate->epp_devices && pstate->epp_devices->len)
    return PPD_PROBE_RESULT_SUCCESS;

  return PPD_PROBE_RESULT_FAIL;
}

static PpdProbeResult
ppd_driver_amd_pstate_probe (PpdDriver  *driver)
{
  PpdDriverAmdPstate *pstate = PPD_DRIVER_AMD_PSTATE (driver);
  PpdProbeResult ret;

  ret = probe_epp (pstate);

  g_debug ("%s p-state settings",
           ret == PPD_PROBE_RESULT_SUCCESS ? "Found" : "Didn't find");
  return ret;
}

static const char *
profile_to_gov_pref (PpdProfile profile)
{
  switch (profile) {
  case PPD_PROFILE_POWER_SAVER:
    return "powersave";
  case PPD_PROFILE_BALANCED:
    return "powersave";
  case PPD_PROFILE_PERFORMANCE:
    return "performance";
  }

  g_return_val_if_reached (NULL);
}

static const char *
profile_to_epp_pref (PpdProfile profile, gboolean battery)
{
  /* Note that we don't check "energy_performance_available_preferences"
   * as all the values are always available */
  switch (profile) {
  case PPD_PROFILE_POWER_SAVER:
    return "power";
  case PPD_PROFILE_BALANCED:
    return battery ? "balance_power" : "balance_performance";
  case PPD_PROFILE_PERFORMANCE:
    return "performance";
  }

  g_return_val_if_reached (NULL);
}

static const char *
profile_to_cpb_pref (PpdProfile profile)
{
  switch (profile) {
  case PPD_PROFILE_POWER_SAVER:
    return "0";
  case PPD_PROFILE_BALANCED:
  case PPD_PROFILE_PERFORMANCE:
    return "1";
  }

  g_return_val_if_reached (NULL);

}

static const char *
profile_to_min_freq (PpdProfile profile)
{
  switch (profile) {
  case PPD_PROFILE_POWER_SAVER:
    return "cpuinfo_min_freq";
  case PPD_PROFILE_BALANCED:
  case PPD_PROFILE_PERFORMANCE:
    return "amd_pstate_lowest_nonlinear_freq";
  }

  g_return_val_if_reached (NULL);

}

static gboolean
apply_pref_to_devices (GPtrArray   *devices,
                       PpdProfile   profile,
                       gboolean     battery,
                       GError     **error)
{
  const char *epp_pref;
  const char *gov_pref;
  const char *cpb_pref;
  const char *min_freq;
  g_autofree char *status = NULL;
  g_autofree char *pstate_status_path = NULL;

  if (profile == PPD_PROFILE_UNSET)
    return TRUE;

  pstate_status_path = ppd_utils_get_sysfs_path (PSTATE_STATUS_PATH);
  if (!g_file_get_contents (pstate_status_path, &status, NULL, error))
    return FALSE;
  status = g_strchomp (status);
  if (g_strcmp0 (status, "active") != 0) {
    g_warning ("AMD P-State is not in active mode");
    return TRUE;
  }

  epp_pref = profile_to_epp_pref (profile, battery);
  gov_pref = profile_to_gov_pref (profile);
  cpb_pref = profile_to_cpb_pref (profile);
  min_freq = profile_to_min_freq (profile);

  for (guint i = 0; i < devices->len; ++i) {
    const char *base = g_ptr_array_index (devices, i);
    g_autofree char *epp = NULL;
    g_autofree char *gov = NULL;
    g_autofree char *cpb = NULL;
    g_autofree char *min_freq_path = NULL;

    gov = g_build_filename (base,
                            "scaling_governor",
                            NULL);

    if (!ppd_utils_write (gov, gov_pref, error))
      return FALSE;

    epp = g_build_filename (base,
                            "energy_performance_preference",
                            NULL);

    if (!ppd_utils_write (epp, epp_pref, error))
      return FALSE;

    cpb = g_build_filename (base, "boost", NULL);
    if (g_file_test (cpb, G_FILE_TEST_EXISTS)) {
      if (!ppd_utils_write (cpb, cpb_pref, error))
        return FALSE;
    }

    min_freq_path = g_build_filename (base, min_freq, NULL);
    if (g_file_test (min_freq_path, G_FILE_TEST_EXISTS)) {
      g_autofree char *scaling_freq_path = NULL;
      g_autofree char *min_freq_val = NULL;

      if (!g_file_get_contents (min_freq_path, &min_freq_val, NULL, error))
        return FALSE;
      min_freq_val = g_strchomp (min_freq_val);

      scaling_freq_path = g_build_filename (base, "scaling_min_freq", NULL);
      if (!ppd_utils_write (scaling_freq_path, min_freq_val, error))
        return FALSE;
    }
  }

  return TRUE;
}

static gboolean
ppd_driver_amd_pstate_activate_profile (PpdDriver                    *driver,
                                        PpdProfile                   profile,
                                        PpdProfileActivationReason   reason,
                                        GError                     **error)
{
  PpdDriverAmdPstate *pstate = PPD_DRIVER_AMD_PSTATE (driver);
  gboolean ret = FALSE;

  g_return_val_if_fail (pstate->epp_devices != NULL, FALSE);
  g_return_val_if_fail (pstate->epp_devices->len != 0, FALSE);

  ret = apply_pref_to_devices (pstate->epp_devices, profile, pstate->on_battery, error);
  if (!ret && pstate->activated_profile != PPD_PROFILE_UNSET) {
    g_autoptr(GError) error_local = NULL;
    /* reset back to previous */
    if (!apply_pref_to_devices (pstate->epp_devices,
                                pstate->activated_profile,
                                pstate->on_battery,
                                &error_local))
      g_warning ("failed to restore previous profile: %s", error_local->message);
    return ret;
  }

  if (ret)
    pstate->activated_profile = profile;

  return ret;
}

static gboolean
ppd_driver_amd_pstate_power_changed (PpdDriver              *driver,
                                     PpdPowerChangedReason   reason,
                                     GError                **error)
{
  PpdDriverAmdPstate *pstate = PPD_DRIVER_AMD_PSTATE (driver);

  switch (reason) {
  case PPD_POWER_CHANGED_REASON_UNKNOWN:
  case PPD_POWER_CHANGED_REASON_AC:
    pstate->on_battery = FALSE;
    break;
  case PPD_POWER_CHANGED_REASON_BATTERY:
    pstate->on_battery = TRUE;
    break;
  default:
    g_assert_not_reached ();
  }

  return apply_pref_to_devices (pstate->epp_devices,
                                pstate->activated_profile,
                                pstate->on_battery,
                                error);
}

static void
ppd_driver_amd_pstate_finalize (GObject *object)
{
  PpdDriverAmdPstate *driver;

  driver = PPD_DRIVER_AMD_PSTATE (object);
  g_clear_pointer (&driver->epp_devices, g_ptr_array_unref);
  G_OBJECT_CLASS (ppd_driver_amd_pstate_parent_class)->finalize (object);
}

static void
ppd_driver_amd_pstate_class_init (PpdDriverAmdPstateClass *klass)
{
  GObjectClass *object_class;
  PpdDriverClass *driver_class;

  object_class = G_OBJECT_CLASS (klass);
  object_class->constructor = ppd_driver_amd_pstate_constructor;
  object_class->finalize = ppd_driver_amd_pstate_finalize;

  driver_class = PPD_DRIVER_CLASS (klass);
  driver_class->probe = ppd_driver_amd_pstate_probe;
  driver_class->activate_profile = ppd_driver_amd_pstate_activate_profile;
  driver_class->power_changed = ppd_driver_amd_pstate_power_changed;
}

static void
ppd_driver_amd_pstate_init (PpdDriverAmdPstate *self)
{
}
