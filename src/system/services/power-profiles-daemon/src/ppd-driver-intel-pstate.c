/*
 * Copyright (c) 2020 Bastien Nocera <hadess@hadess.net>
 *
 * This program is free software; you can redistribute it and/or modify it
 * under the terms of the GNU General Public License version 3 as published by
 * the Free Software Foundation.
 *
 */

#define G_LOG_DOMAIN "CpuDriver"

#include <upower.h>

#include "ppd-utils.h"
#include "ppd-driver-intel-pstate.h"

#define CPU_DIR "/sys/devices/system/cpu/"
#define CPUFREQ_POLICY_DIR "/sys/devices/system/cpu/cpufreq/"
#define DEFAULT_CPU_FREQ_SCALING_GOV "powersave"
#define PSTATE_STATUS_PATH "/sys/devices/system/cpu/intel_pstate/status"
#define NO_TURBO_PATH "/sys/devices/system/cpu/intel_pstate/no_turbo"
#define TURBO_PCT_PATH "/sys/devices/system/cpu/intel_pstate/turbo_pct"

#define SYSTEMD_DBUS_NAME                       "org.freedesktop.login1"
#define SYSTEMD_DBUS_PATH                       "/org/freedesktop/login1"
#define SYSTEMD_DBUS_INTERFACE                  "org.freedesktop.login1.Manager"

struct _PpdDriverIntelPstate
{
  PpdDriverCpu  parent_instance;

  PpdProfile activated_profile;
  GPtrArray *epp_devices; /* Array of paths */
  GPtrArray *epb_devices; /* Array of paths */
  GFileMonitor *no_turbo_mon;
  char *no_turbo_path;
  gboolean on_battery;
};

G_DEFINE_TYPE (PpdDriverIntelPstate, ppd_driver_intel_pstate, PPD_TYPE_DRIVER_CPU)

static gboolean ppd_driver_intel_pstate_activate_profile (PpdDriver                   *driver,
                                                          PpdProfile                   profile,
                                                          PpdProfileActivationReason   reason,
                                                          GError                     **error);

static GObject*
ppd_driver_intel_pstate_constructor (GType                  type,
                                    guint                  n_construct_params,
                                    GObjectConstructParam *construct_params)
{
  GObject *object;

  object = G_OBJECT_CLASS (ppd_driver_intel_pstate_parent_class)->constructor (type,
                                                                              n_construct_params,
                                                                              construct_params);
  g_object_set (object,
                "driver-name", "intel_pstate",
                "profiles", PPD_PROFILE_PERFORMANCE | PPD_PROFILE_BALANCED | PPD_PROFILE_POWER_SAVER,
                NULL);

  return object;
}

static void
update_no_turbo (PpdDriverIntelPstate *pstate)
{
  g_autofree char *contents = NULL;
  gboolean turbo_disabled = FALSE;

  if (g_file_get_contents (pstate->no_turbo_path, &contents, NULL, NULL)) {
    contents = g_strchomp (contents);
    if (g_strcmp0 (contents, "1") == 0)
      turbo_disabled = TRUE;
  }

  g_object_set (G_OBJECT (pstate), "performance-degraded",
                turbo_disabled ? "high-operating-temperature" : NULL,
                NULL);
}

static void
no_turbo_changed (GFileMonitor     *monitor,
                  GFile            *file,
                  GFile            *other_file,
                  GFileMonitorEvent event_type,
                  gpointer          user_data)
{
  PpdDriverIntelPstate *pstate = user_data;
  g_autofree char *path = NULL;

  path = g_file_get_path (file);
  g_debug ("File monitor change happened for '%s' (event type %d)", path, event_type);

  g_return_if_fail (event_type != G_FILE_MONITOR_EVENT_DELETED);

  if (event_type == G_FILE_MONITOR_EVENT_CHANGES_DONE_HINT)
    update_no_turbo (pstate);
}

static GFileMonitor *
monitor_no_turbo_prop (const char *path)
{
  g_autoptr(GFile) no_turbo = NULL;

  if (!g_file_test (path, G_FILE_TEST_EXISTS)) {
    g_debug ("Not monitoring '%s' as it does not exist", path);
    return NULL;
  }

  g_debug ("About to start monitoring '%s'", path);
  no_turbo = g_file_new_for_path (path);
  return g_file_monitor (no_turbo, G_FILE_MONITOR_NONE, NULL, NULL);
}

static gboolean
sys_has_turbo (void)
{
  g_autofree char *turbo_pct_path = NULL;
  g_autofree char *contents = NULL;
  gboolean has_turbo = FALSE;

  turbo_pct_path = ppd_utils_get_sysfs_path (TURBO_PCT_PATH);
  if (g_file_get_contents (turbo_pct_path, &contents, NULL, NULL)) {
    contents = g_strchomp (contents);
    has_turbo = (g_strcmp0 (contents, "0") != 0);
  }

  return has_turbo;
}

static gboolean
ppd_driver_intel_pstate_prepare_for_sleep (PpdDriver  *driver,
                                           gboolean    start,
                                           GError    **error)
{
  PpdDriverIntelPstate *pstate = PPD_DRIVER_INTEL_PSTATE (driver);
  g_autoptr(GError) local_error = NULL;

  if (start)
    return TRUE;

  g_debug ("Re-applying energy_perf_bias");
  if (!ppd_driver_intel_pstate_activate_profile (PPD_DRIVER (pstate),
                                                 pstate->activated_profile,
                                                 PPD_PROFILE_ACTIVATION_REASON_RESUME,
                                                 &local_error)) {
    g_propagate_prefixed_error (error, g_steal_pointer (&local_error),
                                "Could not reapply energy_perf_bias preference on resume: ");
    return FALSE;
  }

  return TRUE;
}

static PpdProbeResult
probe_epb (PpdDriverIntelPstate *pstate)
{
  g_autoptr(GDir) dir = NULL;
  g_autofree char *policy_dir = NULL;
  const char *dirname;

  policy_dir = ppd_utils_get_sysfs_path (CPU_DIR);
  dir = g_dir_open (policy_dir, 0, NULL);
  if (!dir) {
    g_debug ("Could not open %s", CPU_DIR);
    return PPD_PROBE_RESULT_FAIL;
  }

  while ((dirname = g_dir_read_name (dir)) != NULL) {
    g_autofree char *path = NULL;

    path = g_build_filename (policy_dir,
                             dirname,
                             "power",
                             "energy_perf_bias",
                             NULL);
    if (!g_file_test (path, G_FILE_TEST_EXISTS))
      continue;

    if (!pstate->epb_devices)
      pstate->epb_devices = g_ptr_array_new_with_free_func (g_free);

    g_ptr_array_add (pstate->epb_devices, g_steal_pointer (&path));
  }

  if (pstate->epb_devices && pstate->epb_devices->len)
    return PPD_PROBE_RESULT_SUCCESS;

  return PPD_PROBE_RESULT_FAIL;
}

static PpdProbeResult
probe_epp (PpdDriverIntelPstate *pstate)
{
  g_autoptr(GDir) dir = NULL;
  g_autofree char *policy_dir = NULL;
  g_autofree char *pstate_status_path = NULL;
  g_autofree char *status = NULL;
  const char *dirname;

  /* Verify that Intel P-State is running in active mode */
  pstate_status_path = ppd_utils_get_sysfs_path (PSTATE_STATUS_PATH);
  if (!g_file_get_contents (pstate_status_path, &status, NULL, NULL))
    return PPD_PROBE_RESULT_FAIL;
  status = g_strchomp (status);
  if (g_strcmp0 (status, "active") != 0) {
    g_debug ("Intel P-State is running in passive mode");
    return PPD_PROBE_RESULT_FAIL;
  }

  policy_dir = ppd_utils_get_sysfs_path (CPUFREQ_POLICY_DIR);
  dir = g_dir_open (policy_dir, 0, NULL);
  if (!dir) {
    g_debug ("Could not open %s", policy_dir);
    return PPD_PROBE_RESULT_FAIL;
  }

  while ((dirname = g_dir_read_name (dir)) != NULL) {
    g_autofree char *path = NULL;
    g_autofree char *gov_path = NULL;
    g_autoptr(GError) error = NULL;

    path = g_build_filename (policy_dir,
                             dirname,
                             "energy_performance_preference",
                             NULL);
    if (!g_file_test (path, G_FILE_TEST_EXISTS))
      continue;

    /* Force a scaling_governor where the preference can be written */
    gov_path = g_build_filename (policy_dir,
                                 dirname,
                                 "scaling_governor",
                                 NULL);
    if (!ppd_utils_write (gov_path, DEFAULT_CPU_FREQ_SCALING_GOV, &error)) {
      g_warning ("Could not change scaling governor %s to '%s'", dirname, DEFAULT_CPU_FREQ_SCALING_GOV);
      continue;
    }

    if (!pstate->epp_devices)
      pstate->epp_devices = g_ptr_array_new_with_free_func (g_free);

    g_ptr_array_add (pstate->epp_devices, g_steal_pointer (&path));
  }

  if (pstate->epp_devices && pstate->epp_devices->len)
    return PPD_PROBE_RESULT_SUCCESS;

  return PPD_PROBE_RESULT_FAIL;
}

static PpdProbeResult
ppd_driver_intel_pstate_probe (PpdDriver  *driver)
{
  PpdDriverIntelPstate *pstate = PPD_DRIVER_INTEL_PSTATE (driver);
  PpdProbeResult ret = PPD_PROBE_RESULT_FAIL;
  PpdProbeResult epp_ret, epb_ret;
  gboolean has_turbo;

  epp_ret = probe_epp (pstate);
  epb_ret = probe_epb (pstate);
  ret = (epp_ret == PPD_PROBE_RESULT_SUCCESS) ? epp_ret : epb_ret;

  if (ret != PPD_PROBE_RESULT_SUCCESS)
    goto out;

  has_turbo = sys_has_turbo ();
  if (has_turbo) {
    /* Monitor the first "no_turbo" */
    pstate->no_turbo_path = ppd_utils_get_sysfs_path (NO_TURBO_PATH);
    pstate->no_turbo_mon = monitor_no_turbo_prop (pstate->no_turbo_path);
    if (pstate->no_turbo_mon) {
      g_signal_connect_object (G_OBJECT (pstate->no_turbo_mon), "changed",
                               G_CALLBACK (no_turbo_changed), pstate, 0);
    }
    update_no_turbo (pstate);
  }

out:
  g_debug ("%s Intel p-state settings",
           ret == PPD_PROBE_RESULT_SUCCESS ? "Found" : "Didn't find");
  if (ret == PPD_PROBE_RESULT_SUCCESS) {
    g_debug ("\tEnergy Performance Preference: %s",
             epp_ret == PPD_PROBE_RESULT_SUCCESS ? "yes" : "no");
    g_debug ("\tEnergy Performance Bias: %s",
             epp_ret == PPD_PROBE_RESULT_SUCCESS ? "yes" : "no");
    g_debug ("\tHas Turbo: %s", has_turbo ? "yes" : "no");
  }
  return ret;
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
profile_to_epb_pref (PpdProfile profile, gboolean battery)
{
  /* From arch/x86/include/asm/msr-index.h
   * See ENERGY_PERF_BIAS_* */
  switch (profile) {
  case PPD_PROFILE_POWER_SAVER:
    return "15";
  case PPD_PROFILE_BALANCED:
    return battery ? "8" : "6";
  case PPD_PROFILE_PERFORMANCE:
    return "0";
  }

  g_return_val_if_reached (NULL);
}

static gboolean
apply_pref_to_devices (PpdDriver   *driver,
                       PpdProfile   profile,
                       GError     **error)
{
  PpdDriverIntelPstate *pstate = PPD_DRIVER_INTEL_PSTATE (driver);

  if (profile == PPD_PROFILE_UNSET)
    return TRUE;

  g_return_val_if_fail (pstate->epp_devices != NULL ||
                        pstate->epb_devices != NULL, FALSE);
  g_return_val_if_fail ((pstate->epp_devices && pstate->epp_devices->len != 0) ||
                        (pstate->epb_devices && pstate->epb_devices->len != 0), FALSE);

  if (pstate->epp_devices) {
    const char *epp_pref = profile_to_epp_pref (profile, pstate->on_battery);

    if (!ppd_utils_write_files (pstate->epp_devices, epp_pref, error))
      return FALSE;
  }

  if (pstate->epb_devices) {
    const char *epb_pref = profile_to_epb_pref (profile, pstate->on_battery);

    if (!ppd_utils_write_files (pstate->epb_devices, epb_pref, error))
      return FALSE;
  }

  pstate->activated_profile = profile;

  return TRUE;
}

static gboolean
ppd_driver_intel_pstate_power_changed (PpdDriver              *driver,
                                       PpdPowerChangedReason   reason,
                                       GError                **error)
{
  PpdDriverIntelPstate *pstate = PPD_DRIVER_INTEL_PSTATE (driver);

  switch (reason) {
  case PPD_POWER_CHANGED_REASON_UNKNOWN:
  case PPD_POWER_CHANGED_REASON_AC:
    pstate->on_battery = FALSE;
    break;
  case PPD_POWER_CHANGED_REASON_BATTERY:
    pstate->on_battery = TRUE;
    break;
  default:
    g_return_val_if_reached (FALSE);
  }

  return apply_pref_to_devices (driver,
                                pstate->activated_profile,
                                error);
}

static gboolean
ppd_driver_intel_pstate_activate_profile (PpdDriver                    *driver,
                                          PpdProfile                   profile,
                                          PpdProfileActivationReason   reason,
                                          GError                     **error)
{
  return apply_pref_to_devices (driver, profile, error);
}

static void
ppd_driver_intel_pstate_finalize (GObject *object)
{
  PpdDriverIntelPstate *driver;

  driver = PPD_DRIVER_INTEL_PSTATE (object);

  g_clear_pointer (&driver->epp_devices, g_ptr_array_unref);
  g_clear_pointer (&driver->epb_devices, g_ptr_array_unref);
  g_clear_pointer (&driver->no_turbo_path, g_free);
  g_clear_object (&driver->no_turbo_mon);
  G_OBJECT_CLASS (ppd_driver_intel_pstate_parent_class)->finalize (object);
}

static void
ppd_driver_intel_pstate_class_init (PpdDriverIntelPstateClass *klass)
{
  GObjectClass *object_class;
  PpdDriverClass *driver_class;

  object_class = G_OBJECT_CLASS (klass);
  object_class->constructor = ppd_driver_intel_pstate_constructor;
  object_class->finalize = ppd_driver_intel_pstate_finalize;

  driver_class = PPD_DRIVER_CLASS (klass);
  driver_class->probe = ppd_driver_intel_pstate_probe;
  driver_class->activate_profile = ppd_driver_intel_pstate_activate_profile;
  driver_class->prepare_to_sleep = ppd_driver_intel_pstate_prepare_for_sleep;
  driver_class->power_changed = ppd_driver_intel_pstate_power_changed;
}

static void
ppd_driver_intel_pstate_init (PpdDriverIntelPstate *self)
{
}
