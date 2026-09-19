/*
 * Copyright (c) 2014-2016, 2020-2021 Bastien Nocera <hadess@hadess.net>
 * Copyright (c) 2021 David Redondo <kde@david-redondo.de>
 *
 * This program is free software; you can redistribute it and/or modify it
 * under the terms of the GNU General Public License version 3 as published by
 * the Free Software Foundation.
 *
 */

#define G_LOG_DOMAIN "Core"

#include "config.h"

#include <glib-unix.h>
#include <locale.h>
#include <polkit/polkit.h>
#include <stdlib.h>
#include <stdio.h>

#include "power-profiles-daemon-resources.h"
#include "power-profiles-daemon.h"
#include "ppd-driver-cpu.h"
#include "ppd-driver-platform.h"
#include "ppd-action.h"
#include "ppd-enums.h"

#define POWER_PROFILES_DBUS_NAME          "org.freedesktop.UPower.PowerProfiles"
#define POWER_PROFILES_DBUS_PATH          "/org/freedesktop/UPower/PowerProfiles"
#define POWER_PROFILES_IFACE_NAME         POWER_PROFILES_DBUS_NAME

#define POWER_PROFILES_LEGACY_DBUS_NAME   "net.hadess.PowerProfiles"
#define POWER_PROFILES_LEGACY_DBUS_PATH   "/net/hadess/PowerProfiles"
#define POWER_PROFILES_LEGACY_IFACE_NAME  POWER_PROFILES_LEGACY_DBUS_NAME

#define POWER_PROFILES_POLICY_NAMESPACE "org.freedesktop.UPower.PowerProfiles"

#define POWER_PROFILES_RESOURCES_PATH "/org/freedesktop/UPower/PowerProfiles"

#define UPOWER_DBUS_NAME                  "org.freedesktop.UPower"
#define UPOWER_DBUS_PATH                  "/org/freedesktop/UPower"
#define UPOWER_DBUS_INTERFACE             "org.freedesktop.UPower"

#define UPOWER_DBUS_DISPLAY_DEVICE_PATH   "/org/freedesktop/UPower/devices/DisplayDevice"
#define UPOWER_DBUS_DEVICE_INTERFACE      "org.freedesktop.UPower.Device"

#define LOGIND_DBUS_NAME                  "org.freedesktop.login1"
#define LOGIND_DBUS_PATH                  "/org/freedesktop/login1"
#define LOGIND_DBUS_INTERFACE             "org.freedesktop.login1.Manager"

#ifndef POLKIT_HAS_AUTOPOINTERS
/* FIXME: Remove this once we're fine to depend on polkit 0.114 */
G_DEFINE_AUTOPTR_CLEANUP_FUNC (PolkitAuthorizationResult, g_object_unref)
G_DEFINE_AUTOPTR_CLEANUP_FUNC (PolkitSubject, g_object_unref)
#endif

typedef struct {
  GOptionGroup *group;
  GLogLevelFlags log_level;
  gboolean replace;
  gboolean disable_upower;
  gboolean disable_logind;
  GStrv blocked_drivers;
  GStrv blocked_actions;
} DebugOptions;

typedef struct {
  GMainLoop *main_loop;
  GDBusConnection *connection;
  GCancellable *cancellable;
  guint name_id;
  guint legacy_name_id;
  gboolean was_started;
  int ret;

  GKeyFile *config;
  char *config_path;

  PolkitAuthority *auth;

  PpdProfile active_profile;
  PpdProfile selected_profile;
  GPtrArray *probed_drivers;
  PpdDriverCpu *cpu_driver;
  PpdDriverPlatform *platform_driver;
  GPtrArray *actions;
  GHashTable *profile_holds;

  gboolean battery_support;
  GDBusProxy *upower_proxy;
  GDBusProxy *upower_display_proxy;
  gulong upower_watch_id;
  gulong upower_display_watch_id;
  gulong upower_properties_id;
  gulong upower_display_properties_id;
  PpdPowerChangedReason power_changed_reason;

  guint logind_sleep_signal_id;

  DebugOptions *debug_options;
} PpdApp;

typedef struct {
  PpdProfile profile;
  char *reason;
  char *application_id;
  char *requester;
  char *requester_iface;
} ProfileHold;

static void
debug_options_free(DebugOptions *options)
{
  g_strfreev (options->blocked_drivers);
  g_strfreev (options->blocked_actions);
  g_free (options);
}
G_DEFINE_AUTOPTR_CLEANUP_FUNC (DebugOptions, debug_options_free)

static void
profile_hold_free (ProfileHold *hold)
{
  if (hold == NULL)
    return;
  g_free (hold->reason);
  g_free (hold->application_id);
  g_free (hold->requester);
  g_free (hold->requester_iface);
  g_free (hold);
}

static PpdApp *ppd_app = NULL;

static void stop_profile_drivers (PpdApp *data);
static void start_profile_drivers (PpdApp *data);
static void upower_battery_set_power_changed_reason (PpdApp *, PpdPowerChangedReason);

/* profile drivers and actions */
#include "ppd-action-trickle-charge.h"
#include "ppd-action-amdgpu-panel-power.h"
#include "ppd-action-amdgpu-dpm.h"
#include "ppd-driver-placeholder.h"
#include "ppd-driver-platform-profile.h"
#include "ppd-driver-intel-pstate.h"
#include "ppd-driver-amd-pstate.h"
#include "ppd-driver-fake.h"

typedef GType (*GTypeGetFunc) (void);

static GTypeGetFunc objects[] = {
  /* Hardware specific profile drivers */
  ppd_driver_fake_get_type,
  ppd_driver_platform_profile_get_type,
  ppd_driver_intel_pstate_get_type,
  ppd_driver_amd_pstate_get_type,

  /* Generic profile driver */
  ppd_driver_placeholder_get_type,

  /* Actions */
  ppd_action_trickle_charge_get_type,
  ppd_action_amdgpu_panel_power_get_type,
  ppd_action_amdgpu_dpm_get_type,
};

typedef enum {
  PROP_ACTIVE_PROFILE             = 1 << 0,
  PROP_INHIBITED                  = 1 << 1,
  PROP_PROFILES                   = 1 << 2,
  PROP_ACTIONS                    = 1 << 3,
  PROP_DEGRADED                   = 1 << 4,
  PROP_ACTIVE_PROFILE_HOLDS       = 1 << 5,
  PROP_VERSION                    = 1 << 6,
  PROP_UPOWER                     = 1 << 7,
} PropertiesMask;

#define PROP_ALL (PROP_ACTIVE_PROFILE       | \
                  PROP_INHIBITED            | \
                  PROP_PROFILES             | \
                  PROP_ACTIONS              | \
                  PROP_DEGRADED             | \
                  PROP_ACTIVE_PROFILE_HOLDS | \
                  PROP_VERSION              | \
                  PROP_UPOWER)

static gboolean
driver_profile_support (PpdDriver *driver,
                       PpdProfile profile)
{
  if (!PPD_IS_DRIVER (driver))
    return FALSE;
  return (ppd_driver_get_profiles (driver) & profile) != 0;
}

static gboolean
get_profile_available (PpdApp     *data,
                       PpdProfile  profile)
{
  return driver_profile_support (PPD_DRIVER (data->cpu_driver), profile) ||
         driver_profile_support (PPD_DRIVER (data->platform_driver), profile);
}

static const char *
get_active_profile (PpdApp *data)
{
  return ppd_profile_to_str (data->active_profile);
}

static char *
get_performance_degraded (PpdApp *data)
{
  const gchar *cpu_degraded = NULL;
  const gchar *platform_degraded = NULL;

  if (driver_profile_support (PPD_DRIVER (data->platform_driver), PPD_PROFILE_PERFORMANCE))
    platform_degraded = ppd_driver_get_performance_degraded (PPD_DRIVER (data->platform_driver));

  if (driver_profile_support (PPD_DRIVER (data->cpu_driver), PPD_PROFILE_PERFORMANCE))
    cpu_degraded = ppd_driver_get_performance_degraded (PPD_DRIVER (data->cpu_driver));

  if (!cpu_degraded && !platform_degraded)
    return g_strdup ("");

  if (!cpu_degraded)
    return g_strdup (platform_degraded);

  if (!platform_degraded)
    return g_strdup (cpu_degraded);

  return g_strjoin (",", cpu_degraded, platform_degraded, NULL);
}

static GVariant *
get_profiles_variant (PpdApp *data)
{
  GVariantBuilder builder;
  guint i;

  g_variant_builder_init (&builder, G_VARIANT_TYPE ("aa{sv}"));

  for (i = 0; i < NUM_PROFILES; i++) {
    PpdDriver *platform_driver = PPD_DRIVER (data->platform_driver);
    PpdDriver *cpu_driver = PPD_DRIVER (data->cpu_driver);
    PpdProfile profile = 1 << i;
    GVariantBuilder asv_builder;
    gboolean cpu, platform;
    const gchar *driver = NULL;

    /* check if any of the drivers support */
    if (!get_profile_available (data, profile))
      continue;

    g_variant_builder_init (&asv_builder, G_VARIANT_TYPE ("a{sv}"));
    g_variant_builder_add (&asv_builder, "{sv}", "Profile",
                           g_variant_new_string (ppd_profile_to_str (profile)));
    cpu = driver_profile_support (cpu_driver, profile);
    platform = driver_profile_support (platform_driver, profile);
    if (cpu)
      g_variant_builder_add (&asv_builder, "{sv}", "CpuDriver",
                             g_variant_new_string (ppd_driver_get_driver_name (cpu_driver)));
    if (platform)
      g_variant_builder_add (&asv_builder, "{sv}", "PlatformDriver",
                             g_variant_new_string (ppd_driver_get_driver_name (platform_driver)));

    /* compatibility with older API */
    if (cpu && platform)
      driver = "multiple";
    else if (cpu)
      driver = ppd_driver_get_driver_name (cpu_driver);
    else if (platform)
      driver = ppd_driver_get_driver_name (platform_driver);

    if (driver)
      g_variant_builder_add (&asv_builder, "{sv}", "Driver",
                             g_variant_new_string (driver));

    g_variant_builder_add (&builder, "a{sv}", &asv_builder);
  }

  return g_variant_builder_end (&builder);
}

static GVariant *
get_legacy_actions_variant (PpdApp *data)
{
  GVariantBuilder builder;
  guint i;

  g_variant_builder_init (&builder, G_VARIANT_TYPE ("as"));

  for (i = 0; i < data->actions->len; i++) {
    PpdAction *action = g_ptr_array_index (data->actions, i);

    if (!ppd_action_get_active (action))
      continue;

    g_variant_builder_add (&builder, "s", ppd_action_get_action_name (action));
  }

  return g_variant_builder_end (&builder);
}

static GVariant *
get_modern_actions_variant (PpdApp *data)
{
  GVariantBuilder builder;
  guint i;

  g_variant_builder_init (&builder, G_VARIANT_TYPE ("aa{sv}"));

  for (i = 0; i < data->actions->len; i++) {
    PpdAction *action = g_ptr_array_index (data->actions, i);
    GVariantBuilder asv_builder;

    g_variant_builder_init (&asv_builder, G_VARIANT_TYPE ("a{sv}"));
    g_variant_builder_add (&asv_builder, "{sv}", "Name",
                           g_variant_new_string (ppd_action_get_action_name (action)));
    g_variant_builder_add (&asv_builder, "{sv}", "Description",
                           g_variant_new_string (ppd_action_get_action_description (action)));
    g_variant_builder_add (&asv_builder, "{sv}", "Enabled",
                           g_variant_new_boolean (ppd_action_get_active (action)));
    g_variant_builder_add (&builder, "a{sv}", &asv_builder);
  }

  return g_variant_builder_end (&builder);
}


static GVariant *
get_profile_holds_variant (PpdApp *data)
{
  GVariantBuilder builder;
  GHashTableIter iter;
  gpointer value;

  g_variant_builder_init (&builder, G_VARIANT_TYPE ("aa{sv}"));
  g_hash_table_iter_init (&iter, data->profile_holds);

  while (g_hash_table_iter_next (&iter, NULL, &value)) {
    GVariantBuilder asv_builder;
    ProfileHold *hold = value;

    g_variant_builder_init (&asv_builder, G_VARIANT_TYPE ("a{sv}"));
    g_variant_builder_add (&asv_builder, "{sv}", "ApplicationId",
                           g_variant_new_string (hold->application_id));
    g_variant_builder_add (&asv_builder, "{sv}", "Profile",
                           g_variant_new_string (ppd_profile_to_str (hold->profile)));
    g_variant_builder_add (&asv_builder, "{sv}", "Reason", g_variant_new_string (hold->reason));

    g_variant_builder_add (&builder, "a{sv}", &asv_builder);
  }

  return g_variant_builder_end (&builder);
}

static void
send_dbus_event_iface (PpdApp         *data,
                       PropertiesMask  mask,
                       const gchar    *iface,
                       const gchar    *path)
{
  GVariantBuilder props_builder;
  GVariant *props_changed = NULL;

  g_return_if_fail (data->connection);

  if (mask == 0)
    return;

  g_return_if_fail ((mask & PROP_ALL) != 0);

  g_variant_builder_init (&props_builder, G_VARIANT_TYPE ("a{sv}"));

  if (mask & PROP_ACTIVE_PROFILE) {
    g_variant_builder_add (&props_builder, "{sv}", "ActiveProfile",
                           g_variant_new_string (get_active_profile (data)));
  }
  if (mask & PROP_INHIBITED) {
    g_variant_builder_add (&props_builder, "{sv}", "PerformanceInhibited",
                           g_variant_new_string (""));
  }
  if (mask & PROP_DEGRADED) {
    gchar *degraded = get_performance_degraded (data);
    g_variant_builder_add (&props_builder, "{sv}", "PerformanceDegraded",
                           g_variant_new_take_string (g_steal_pointer (&degraded)));
  }
  if (mask & PROP_PROFILES) {
    g_variant_builder_add (&props_builder, "{sv}", "Profiles",
                           get_profiles_variant (data));
  }
  if (mask & PROP_ACTIONS) {
    if (g_str_equal (iface, POWER_PROFILES_IFACE_NAME))
      g_variant_builder_add (&props_builder, "{sv}", "Actions",
                             get_modern_actions_variant (data));
    else
      g_variant_builder_add (&props_builder, "{sv}", "Actions",
                             get_legacy_actions_variant (data));
  }
  if (mask & PROP_ACTIVE_PROFILE_HOLDS) {
    g_variant_builder_add (&props_builder, "{sv}", "ActiveProfileHolds",
                           get_profile_holds_variant (data));
  }
  if (mask & PROP_VERSION) {
    g_variant_builder_add (&props_builder, "{sv}", "Version",
                           g_variant_new_string (VERSION));
  }
  if (mask & PROP_UPOWER) {
    g_variant_builder_add (&props_builder, "{sv}", "BatteryAware",
                           g_variant_new_boolean (data->battery_support));
  }

  props_changed = g_variant_new ("(s@a{sv}@as)", iface,
                                 g_variant_builder_end (&props_builder),
                                 g_variant_new_strv (NULL, 0));

  g_dbus_connection_emit_signal (data->connection,
                                 NULL,
                                 path,
                                 "org.freedesktop.DBus.Properties",
                                 "PropertiesChanged",
                                 props_changed, NULL);
}

static void
send_dbus_event (PpdApp         *data,
                 PropertiesMask  mask)
{
  send_dbus_event_iface (data, mask,
                         POWER_PROFILES_IFACE_NAME,
                         POWER_PROFILES_DBUS_PATH);
  send_dbus_event_iface (data, mask,
                         POWER_PROFILES_LEGACY_IFACE_NAME,
                         POWER_PROFILES_LEGACY_DBUS_PATH);
}

static void
save_configuration (PpdApp *data)
{
  g_autoptr(GError) error = NULL;

  if (PPD_IS_DRIVER_CPU (data->cpu_driver)) {
    g_key_file_set_string (data->config, "State", "CpuDriver",
                           ppd_driver_get_driver_name (PPD_DRIVER (data->cpu_driver)));
  }

  if (PPD_IS_DRIVER_PLATFORM (data->platform_driver)) {
    g_key_file_set_string (data->config, "State", "PlatformDriver",
                           ppd_driver_get_driver_name (PPD_DRIVER (data->platform_driver)));
  }

  g_key_file_set_string (data->config, "State", "Profile",
                         ppd_profile_to_str (data->active_profile));

  for (guint i = 0; i < data->actions->len; i++) {
    PpdAction *action = g_ptr_array_index (data->actions, i);
    g_key_file_set_boolean (data->config, "Actions",
                            ppd_action_get_action_name (action),
                            ppd_action_get_active (action));
  }

  g_key_file_set_boolean (data->config, "State", "battery_aware", data->battery_support);

  if (!g_key_file_save_to_file (data->config, data->config_path, &error))
    g_warning ("Could not save configuration file '%s': %s", data->config_path, error->message);
}

static gboolean
apply_configuration (PpdApp *data)
{
  g_autofree char *platform_driver = NULL;
  g_autofree char *profile_str = NULL;
  g_autofree char *cpu_driver = NULL;
  g_autoptr(GError) error = NULL;
  PpdProfile profile;

  data->battery_support = g_key_file_get_boolean (data->config, "State", "battery_aware", &error);
  if (error != NULL) {
    g_debug("battery_aware key not found: %s, defaulting to TRUE", error->message);
    data->battery_support = TRUE;
    g_key_file_set_boolean (data->config, "State", "battery_aware", TRUE);
    g_clear_error (&error);
  }

  cpu_driver = g_key_file_get_string (data->config, "State", "CpuDriver", NULL);
  if (PPD_IS_DRIVER_CPU (data->cpu_driver) &&
      g_strcmp0 (ppd_driver_get_driver_name (PPD_DRIVER (data->cpu_driver)), cpu_driver) != 0)
    return FALSE;

  platform_driver = g_key_file_get_string (data->config, "State", "PlatformDriver", NULL);

  if (PPD_IS_DRIVER_PLATFORM (data->platform_driver) &&
      g_strcmp0 (ppd_driver_get_driver_name (PPD_DRIVER (data->platform_driver)), platform_driver) != 0)
    return FALSE;

  profile_str = g_key_file_get_string (data->config, "State", "Profile", NULL);
  if (profile_str == NULL)
    return FALSE;

  profile = ppd_profile_from_str (profile_str);
  if (profile == PPD_PROFILE_UNSET) {
    g_debug ("Resetting invalid configuration profile '%s'", profile_str);
    g_key_file_remove_key (data->config, "State", "Profile", NULL);
    return FALSE;
  }

  g_debug ("Applying profile '%s' from configuration file", profile_str);
  data->active_profile = profile;
  return TRUE;
}

static void
load_configuration (PpdApp *data)
{
  g_autoptr(GError) error = NULL;

  if (g_getenv ("UMOCKDEV_DIR") != NULL)
    data->config_path = g_build_filename (g_getenv ("UMOCKDEV_DIR"), "ppd_test_conf.ini", NULL);
  else
    data->config_path = g_strdup ("/var/lib/power-profiles-daemon/state.ini");
  data->config = g_key_file_new ();
  if (!g_key_file_load_from_file (data->config, data->config_path, G_KEY_FILE_KEEP_COMMENTS, &error))
    g_debug ("Could not load configuration file '%s': %s", data->config_path, error->message);
  else
    g_debug ("Loaded configuration file '%s'", data->config_path);
}

static void
actions_activate_profile (GPtrArray *actions,
                          PpdProfile profile)
{
  guint i;

  g_return_if_fail (actions != NULL);

  for (i = 0; i < actions->len; i++) {
    g_autoptr(GError) error = NULL;
    PpdAction *action;

    action = g_ptr_array_index (actions, i);

    if (!ppd_action_get_active (action))
      continue;

    if (!ppd_action_activate_profile (action, profile, &error))
      g_warning ("Failed to activate action '%s' to profile %s: %s",
                 ppd_profile_to_str (profile),
                 ppd_action_get_action_name (action),
                 error->message);
  }
}

static gboolean
activate_target_profile (PpdApp                      *data,
                         PpdProfile                   target_profile,
                         PpdProfileActivationReason   reason,
                         GError                     **error)
{
  PpdProfile current_profile = data->active_profile;

  g_info ("Setting active profile '%s' for reason '%s' (current: '%s')",
           ppd_profile_to_str (target_profile),
           ppd_profile_activation_reason_to_str (reason),
           ppd_profile_to_str (current_profile));

  /* Try CPU first */
  if (driver_profile_support (PPD_DRIVER (data->cpu_driver), target_profile) &&
      !ppd_driver_activate_profile (PPD_DRIVER (data->cpu_driver),
                                    target_profile, reason, error)) {
    g_prefix_error (error, "Failed to activate CPU driver '%s': ",
                    ppd_driver_get_driver_name (PPD_DRIVER (data->cpu_driver)));
    return FALSE;
  }

  /* Then try platform */
  if (driver_profile_support (PPD_DRIVER (data->platform_driver), target_profile) &&
      !ppd_driver_activate_profile (PPD_DRIVER (data->platform_driver),
                                                target_profile, reason, error)) {
    g_autoptr(GError) recovery_error = NULL;

    g_prefix_error (error, "Failed to activate platform driver '%s': ",
                    ppd_driver_get_driver_name (PPD_DRIVER (data->platform_driver)));

    if (!PPD_IS_DRIVER (data->cpu_driver))
      return FALSE;

    g_debug ("Reverting CPU driver '%s' to profile '%s'",
              ppd_driver_get_driver_name (PPD_DRIVER (data->cpu_driver)),
              ppd_profile_to_str (current_profile));

    if (!ppd_driver_activate_profile (PPD_DRIVER (data->cpu_driver),
                                      current_profile, PPD_PROFILE_ACTIVATION_REASON_INTERNAL,
                                      &recovery_error)) {
      g_warning ("Failed to revert CPU driver '%s': %s",
                  ppd_driver_get_driver_name (PPD_DRIVER (data->cpu_driver)),
                  recovery_error->message);
    }

    return FALSE;
  }

  actions_activate_profile (data->actions, target_profile);

  data->active_profile = target_profile;

  if (reason == PPD_PROFILE_ACTIVATION_REASON_USER ||
      reason == PPD_PROFILE_ACTIVATION_REASON_INTERNAL)
    save_configuration (data);

  return TRUE;
}

static void
release_hold_notify (PpdApp      *data,
                     ProfileHold *hold,
                     guint        cookie)
{
  const char *req_path = POWER_PROFILES_DBUS_PATH;

  if (g_strcmp0 (hold->requester_iface, POWER_PROFILES_LEGACY_IFACE_NAME) == 0)
    req_path = POWER_PROFILES_LEGACY_DBUS_PATH;

  g_dbus_connection_emit_signal (data->connection, hold->requester, req_path,
                                 hold->requester_iface, "ProfileReleased",
                                 g_variant_new ("(u)", cookie), NULL);
}

static void
release_all_profile_holds (PpdApp *data)
{
  GHashTableIter iter;
  gpointer key, value;

  g_hash_table_iter_init (&iter, data->profile_holds);
  while (g_hash_table_iter_next (&iter, &key, &value)) {
    ProfileHold *hold = value;
    guint cookie = GPOINTER_TO_UINT (key);

    release_hold_notify (data, hold, cookie);
    g_bus_unwatch_name (cookie);
  }
  g_hash_table_remove_all (data->profile_holds);
}

static gboolean
set_battery_support (PpdApp *data, gboolean battery_support, GError **error)
{
  if (data->battery_support == battery_support) {
    g_set_error (error, G_DBUS_ERROR, G_DBUS_ERROR_FAILED,
                 "battery_aware is already set to %s", battery_support ? "TRUE" : "FALSE");
    return FALSE;
  }

  data->battery_support = battery_support;
  g_key_file_set_boolean (data->config, "State", "battery_aware", data->battery_support);
  g_debug("battery_aware set to %s", data->battery_support ? "TRUE" : "FALSE");

  restart_profile_drivers_for_default_app ();

  return TRUE;
}

static gboolean
set_active_profile (PpdApp      *data,
                    const char  *profile,
                    GError     **error)
{
  PpdProfile target_profile;
  guint mask = PROP_ACTIVE_PROFILE;

  target_profile = ppd_profile_from_str (profile);
  if (target_profile == PPD_PROFILE_UNSET) {
    g_set_error (error, G_DBUS_ERROR, G_DBUS_ERROR_FAILED,
                 "Invalid profile name '%s'", profile);
    return FALSE;
  }
  if (!get_profile_available (data, target_profile)) {
    g_set_error (error, G_DBUS_ERROR, G_DBUS_ERROR_FAILED,
                 "Cannot switch to unavailable profile '%s'", profile);
    return FALSE;
  }

  if (target_profile == data->active_profile)
    return TRUE;

  g_debug ("Transitioning active profile from '%s' to '%s' by user request",
           ppd_profile_to_str (data->active_profile), profile);

  if (g_hash_table_size (data->profile_holds) != 0 ) {
    g_debug ("Releasing active profile holds");
    release_all_profile_holds (data);
    mask |= PROP_ACTIVE_PROFILE_HOLDS;
  }

  if (!activate_target_profile (data, target_profile, PPD_PROFILE_ACTIVATION_REASON_USER, error))
    return FALSE;
  data->selected_profile = target_profile;
  send_dbus_event (data, mask);

  return TRUE;
}

static PpdProfile
effective_hold_profile (PpdApp *data)
{
  GHashTableIter iter;
  gpointer value;
  PpdProfile profile = PPD_PROFILE_UNSET;

  g_hash_table_iter_init (&iter, data->profile_holds);
  while (g_hash_table_iter_next (&iter, NULL, &value)) {
    ProfileHold *hold = value;

    if (hold->profile == PPD_PROFILE_POWER_SAVER) {
      profile = PPD_PROFILE_POWER_SAVER;
      break;
    }
    profile = hold->profile;
  }
  return profile;
}

static void
driver_performance_degraded_changed_cb (GObject    *gobject,
                                        GParamSpec *pspec,
                                        gpointer    user_data)
{
  PpdApp *data = user_data;
  PpdDriver *driver = PPD_DRIVER (gobject);
  const char *prop_str = pspec->name;

  if (g_strcmp0 (prop_str, "performance-degraded") != 0) {
    g_warning ("Ignoring '%s' property change on profile driver '%s'",
               prop_str, ppd_driver_get_driver_name (driver));
    return;
  }

  if (!(ppd_driver_get_profiles (driver) & PPD_PROFILE_PERFORMANCE)) {
    g_warning ("Ignored 'performance-degraded' change on non-performance driver '%s'",
               ppd_driver_get_driver_name (driver));
    return;
  }

  send_dbus_event (data, PROP_DEGRADED);
}

static void
driver_profile_changed_cb (PpdDriver *driver,
                           PpdProfile new_profile,
                           gpointer   user_data)
{
  PpdApp *data = user_data;

  g_debug ("Driver '%s' switched internally to profile '%s' (current: '%s')",
           ppd_driver_get_driver_name (driver),
           ppd_profile_to_str (new_profile),
           ppd_profile_to_str (data->active_profile));
  if (new_profile == data->active_profile)
    return;

  activate_target_profile (data, new_profile, PPD_PROFILE_ACTIVATION_REASON_INTERNAL, NULL);
  send_dbus_event (data, PROP_ACTIVE_PROFILE);
}

static void
release_profile_hold (PpdApp *data,
                      guint   cookie)
{
  guint mask = PROP_ACTIVE_PROFILE_HOLDS;
  ProfileHold *hold;
  PpdProfile hold_profile, next_profile;

  hold = g_hash_table_lookup (data->profile_holds, GUINT_TO_POINTER (cookie));
  if (!hold) {
    g_debug ("No hold with cookie %d", cookie);
    return;
  }

  g_bus_unwatch_name (cookie);
  hold_profile = hold->profile;
  release_hold_notify (data, hold, cookie);
  g_hash_table_remove (data->profile_holds, GUINT_TO_POINTER (cookie));

  if (g_hash_table_size (data->profile_holds) == 0 &&
      hold_profile != data->selected_profile) {
    g_debug ("No profile holds anymore going back to last manually activated profile");
    activate_target_profile (data, data->selected_profile, PPD_PROFILE_ACTIVATION_REASON_PROGRAM_HOLD, NULL);
    mask |= PROP_ACTIVE_PROFILE;
  } else if (hold_profile == data->active_profile) {
    next_profile = effective_hold_profile (data);
    if (next_profile != PPD_PROFILE_UNSET &&
        next_profile != data->active_profile) {
      g_debug ("Next profile is %s", ppd_profile_to_str (next_profile));
      activate_target_profile (data, next_profile, PPD_PROFILE_ACTIVATION_REASON_PROGRAM_HOLD, NULL);
      mask |= PROP_ACTIVE_PROFILE;
    }
  }

  send_dbus_event (data, mask);
}

static void
holder_disappeared (GDBusConnection *connection,
                    const gchar     *name,
                    gpointer         user_data)
{
  PpdApp *data = user_data;
  GHashTableIter iter;
  gpointer key, value;
  GPtrArray *cookies;
  guint i;

  cookies = g_ptr_array_new ();
  g_hash_table_iter_init (&iter, data->profile_holds);
  while (g_hash_table_iter_next (&iter, &key, (gpointer *) &value)) {
    guint cookie = GPOINTER_TO_UINT (key);
    ProfileHold *hold = value;

    if (g_strcmp0 (hold->requester, name) != 0)
      continue;

    g_debug ("Holder %s with cookie %u disappeared, adding to list", name, cookie);
    g_ptr_array_add (cookies, GUINT_TO_POINTER (cookie));
  }

  for (i = 0; i < cookies->len; i++) {
    guint cookie = GPOINTER_TO_UINT (cookies->pdata[i]);
    g_debug ("Removing profile hold for cookie %u", cookie);
    release_profile_hold (data, cookie);
  }
  g_ptr_array_free (cookies, TRUE);
}

static void
hold_profile (PpdApp                *data,
              GVariant              *parameters,
              GDBusMethodInvocation *invocation)
{
  const char *profile_name;
  const char *reason;
  const char *application_id;
  PpdProfile profile;
  ProfileHold *hold;
  guint watch_id;
  guint mask;

  g_variant_get (parameters, "(&s&s&s)", &profile_name, &reason, &application_id);
  profile = ppd_profile_from_str (profile_name);
  if (profile != PPD_PROFILE_PERFORMANCE &&
      profile != PPD_PROFILE_POWER_SAVER) {
    g_dbus_method_invocation_return_error_literal (invocation, G_DBUS_ERROR, G_DBUS_ERROR_INVALID_ARGS,
                                                   "Only profiles 'performance' and 'power-saver' can be a hold profile");
    return;
  }
  if (!get_profile_available (data, profile)) {
    g_dbus_method_invocation_return_error (invocation, G_DBUS_ERROR, G_DBUS_ERROR_INVALID_ARGS,
                                           "Cannot hold profile '%s' as it is not available",
                                           profile_name);
    return;
  }

  hold = g_new0 (ProfileHold, 1);
  hold->profile = profile;
  hold->reason = g_strdup (reason);
  hold->application_id = g_strdup (application_id);
  hold->requester = g_strdup (g_dbus_method_invocation_get_sender (invocation));
  hold->requester_iface = g_strdup (g_dbus_method_invocation_get_interface_name (invocation));

  g_debug ("%s (%s) requesting to hold profile '%s', reason: '%s'", application_id,
           hold->requester, profile_name, reason);
  watch_id = g_bus_watch_name_on_connection (data->connection, hold->requester,
                                             G_BUS_NAME_WATCHER_FLAGS_NONE, NULL,
                                             holder_disappeared, data, NULL);
  g_hash_table_insert (data->profile_holds, GUINT_TO_POINTER (watch_id), hold);
  g_dbus_method_invocation_return_value (invocation, g_variant_new ("(u)", watch_id));
  mask = PROP_ACTIVE_PROFILE_HOLDS;

  if (profile != data->active_profile) {
    PpdProfile target_profile = effective_hold_profile (data);
    if (target_profile != PPD_PROFILE_UNSET &&
        target_profile != data->active_profile) {
      activate_target_profile (data, target_profile, PPD_PROFILE_ACTIVATION_REASON_PROGRAM_HOLD, NULL);
      mask |= PROP_ACTIVE_PROFILE;
    }
  }

  send_dbus_event (data, mask);
}

static void
release_profile (PpdApp                *data,
                 GVariant              *parameters,
                 GDBusMethodInvocation *invocation)
{
  guint cookie;
  g_variant_get (parameters, "(u)", &cookie);
  if (!g_hash_table_contains (data->profile_holds, GUINT_TO_POINTER (cookie))) {
    g_dbus_method_invocation_return_error (invocation, G_DBUS_ERROR, G_DBUS_ERROR_INVALID_ARGS,
                                           "No hold with cookie  %d", cookie);
    return;
  }
  release_profile_hold (data, cookie);
  g_dbus_method_invocation_return_value (invocation, NULL);
}

static gboolean
check_action_permission (PpdApp                *data,
                         const char            *sender,
                         const char            *action,
                         GError               **error)
{
  g_autoptr(GError) local_error = NULL;
  g_autoptr(PolkitAuthorizationResult) result = NULL;
  g_autoptr(PolkitSubject) subject = NULL;

  subject = polkit_system_bus_name_new (sender);
  result = polkit_authority_check_authorization_sync (data->auth,
                                                      subject,
                                                      action,
                                                      NULL,
                                                      POLKIT_CHECK_AUTHORIZATION_FLAGS_NONE,
                                                      NULL, &local_error);
  if (result == NULL ||
      !polkit_authorization_result_get_is_authorized (result))
    {
      g_set_error (error, G_DBUS_ERROR,
                   G_DBUS_ERROR_ACCESS_DENIED,
                   "Not Authorized: %s", local_error ? local_error->message : action);
      return FALSE;
    }

  return TRUE;

}

static GVariant *
handle_get_property (GDBusConnection *connection,
                     const gchar     *sender,
                     const gchar     *object_path,
                     const gchar     *interface_name,
                     const gchar     *property_name,
                     GError         **error,
                     gpointer         user_data)
{
  PpdApp *data = user_data;

  g_return_val_if_fail (data->connection, NULL);

  if (g_strcmp0 (property_name, "ActiveProfile") == 0)
    return g_variant_new_string (get_active_profile (data));
  if (g_strcmp0 (property_name, "PerformanceInhibited") == 0)
    return g_variant_new_string ("");
  if (g_strcmp0 (property_name, "Profiles") == 0)
    return get_profiles_variant (data);
  if (g_str_equal (property_name, "ActionsInfo") &&
      g_str_equal (interface_name, POWER_PROFILES_IFACE_NAME))
      return get_modern_actions_variant (data);
  if (g_str_equal (property_name, "Actions"))
      return get_legacy_actions_variant (data);
  if (g_str_equal (property_name, "BatteryAware"))
      return g_variant_new_boolean (data->battery_support);
  if (g_strcmp0 (property_name, "PerformanceDegraded") == 0) {
    gchar *degraded = get_performance_degraded (data);
    return g_variant_new_take_string (g_steal_pointer (&degraded));
  }
  if (g_strcmp0 (property_name, "ActiveProfileHolds") == 0)
    return get_profile_holds_variant (data);
  if (g_strcmp0 (property_name, "Version") == 0)
    return g_variant_new_string (VERSION);
  g_set_error (error, G_DBUS_ERROR, G_DBUS_ERROR_FAILED,
               "Invalid property'%s'", property_name);
  return NULL;
}

static gboolean
handle_set_property (GDBusConnection  *connection,
                     const gchar      *sender,
                     const gchar      *object_path,
                     const gchar      *interface_name,
                     const gchar      *property_name,
                     GVariant         *value,
                     GError          **error,
                     gpointer          user_data)
{
  PpdApp *data = user_data;

  g_return_val_if_fail (data->connection, FALSE);

  if (g_str_equal (property_name, "ActiveProfile")) {
    const char *profile;

    if (!check_action_permission (data, sender,
                                  POWER_PROFILES_POLICY_NAMESPACE ".switch-profile",
                                  error))
      return FALSE;

    g_variant_get (value, "&s", &profile);
    return set_active_profile (data, profile, error);
  } else if (g_str_equal (property_name, "BatteryAware")) {
    if (!check_action_permission (data,
                                  sender,
                                  POWER_PROFILES_POLICY_NAMESPACE ".configure-battery-aware",
                                  error)) {
      return FALSE;
    }

    return set_battery_support (data, g_variant_get_boolean (value), error);
  }

  g_set_error (error, G_DBUS_ERROR, G_DBUS_ERROR_FAILED,
               "No such property: %s", property_name);
  return FALSE;
}

static gboolean set_action_enabled (PpdApp                *data,
                                    GVariant              *parameters,
                                    GError                **error)
{
  const char *action_name;
  gboolean active;

  g_variant_get (parameters, "(&sb)", &action_name, &active);

  for (guint i = 0; i < data->actions->len; i++) {
    PpdAction *action = g_ptr_array_index (data->actions, i);

    if (g_strcmp0 (ppd_action_get_action_name (action), action_name) == 0) {
      g_key_file_set_boolean (data->config, "Actions", action_name, active);
      restart_profile_drivers_for_default_app ();
      return TRUE;
    }
  }
  g_set_error (error, G_DBUS_ERROR, G_DBUS_ERROR_INVALID_ARGS,
               "No such action '%s'", action_name);
  return FALSE;
}

static void
handle_method_call (GDBusConnection       *connection,
                    const gchar           *sender,
                    const gchar           *object_path,
                    const gchar           *interface_name,
                    const gchar           *method_name,
                    GVariant              *parameters,
                    GDBusMethodInvocation *invocation,
                    gpointer               user_data)
{
  PpdApp *data = user_data;
  g_return_if_fail (data->connection);

  if (!g_str_equal (interface_name, POWER_PROFILES_IFACE_NAME) &&
      !g_str_equal (interface_name, POWER_PROFILES_LEGACY_IFACE_NAME)) {
    g_dbus_method_invocation_return_error (invocation,G_DBUS_ERROR, G_DBUS_ERROR_UNKNOWN_INTERFACE,
                                           "Unknown interface %s", interface_name);
    return;
  }

  if (g_strcmp0 (method_name, "HoldProfile") == 0) {
    g_autoptr(GError) local_error = NULL;
    if (!check_action_permission (data,
                                  g_dbus_method_invocation_get_sender (invocation),
                                  POWER_PROFILES_POLICY_NAMESPACE ".hold-profile",
                                  &local_error)) {
      g_dbus_method_invocation_return_gerror (invocation, local_error);
      return;
    }
    hold_profile (data, parameters, invocation);
  } else if (g_strcmp0 (method_name, "ReleaseProfile") == 0) {
    release_profile (data, parameters, invocation);
  } else if (g_strcmp0 (method_name, "SetActionEnabled") == 0) {
    g_autoptr(GError) local_error = NULL;

    if (g_str_equal (interface_name, POWER_PROFILES_LEGACY_IFACE_NAME)) {
      g_dbus_method_invocation_return_error (invocation, G_DBUS_ERROR, G_DBUS_ERROR_UNKNOWN_METHOD,
                                             "Method %s is not available in interface %s", method_name,
                                             interface_name);
      return;
    }

    if (!check_action_permission (data,
                                  g_dbus_method_invocation_get_sender (invocation),
                                  POWER_PROFILES_POLICY_NAMESPACE ".configure-action",
                                  &local_error)) {
      g_dbus_method_invocation_return_gerror (invocation, local_error);
      return;
    }

    if (!set_action_enabled (data, parameters, &local_error)) {
      g_dbus_method_invocation_return_gerror (invocation, local_error);
      return;
    }
    g_dbus_method_invocation_return_value (invocation, NULL);
  } else {
      g_dbus_method_invocation_return_error (invocation, G_DBUS_ERROR, G_DBUS_ERROR_UNKNOWN_METHOD,
                                             "No such method %s in interface %s", interface_name,
                                              method_name);
  }
}


static const GDBusInterfaceVTable interface_vtable =
{
  handle_method_call,
  handle_get_property,
  handle_set_property
};

typedef struct {
  PpdApp *app;
  GBusNameOwnerFlags flags;
  GDBusInterfaceInfo *interface;
  GDBusInterfaceInfo *legacy_interface;
} PpdBusOwnData;

static void
ppd_bus_own_data_free (PpdBusOwnData *data)
{
  g_clear_pointer (&data->interface, g_dbus_interface_info_unref);
  g_clear_pointer (&data->legacy_interface, g_dbus_interface_info_unref);
  g_free (data);
}

static void
name_lost_handler (GDBusConnection *connection,
                   const gchar     *name,
                   gpointer         user_data)
{
  PpdBusOwnData *data = user_data;
  PpdApp *app = data->app;

  g_warning ("power-profiles-daemon is already running, or it cannot own its D-Bus name. Verify installation.");
  if (!app->was_started)
    app->ret = EXIT_FAILURE;

  g_main_loop_quit (app->main_loop);
}

static void
legacy_name_acquired_handler (GDBusConnection *connection,
                              const gchar     *name,
                              gpointer         user_data)
{
  g_debug ("Name '%s' acquired", name);
}

static void
bus_acquired_handler (GDBusConnection *connection,
                      const gchar     *name,
                      gpointer         user_data)
{
  PpdBusOwnData *data = user_data;

  g_dbus_connection_register_object (connection,
                                     POWER_PROFILES_DBUS_PATH,
                                     data->interface,
                                     &interface_vtable,
                                     data->app,
                                     NULL,
                                     NULL);

  g_dbus_connection_register_object (connection,
                                     POWER_PROFILES_LEGACY_DBUS_PATH,
                                     data->legacy_interface,
                                     &interface_vtable,
                                     data->app,
                                     NULL,
                                     NULL);

  data->app->legacy_name_id = g_bus_own_name_on_connection (connection,
                                                            POWER_PROFILES_LEGACY_DBUS_NAME,
                                                            data->flags,
                                                            legacy_name_acquired_handler,
                                                            name_lost_handler,
                                                            data,
                                                            NULL);

  data->app->connection = g_object_ref (connection);
}

static void
upower_source_update_from_value (PpdApp   *data,
                                 GVariant *battery_val)
{
  PpdPowerChangedReason reason;

  if (!battery_val)
    reason = PPD_POWER_CHANGED_REASON_UNKNOWN;
  else if (g_variant_get_boolean (battery_val))
    reason = PPD_POWER_CHANGED_REASON_BATTERY;
  else
    reason = PPD_POWER_CHANGED_REASON_AC;

  upower_battery_set_power_changed_reason (data, reason);
}

static void
upower_source_update (PpdApp *data)
{
  g_autoptr(GVariant) battery_val = NULL;

  battery_val = g_dbus_proxy_get_cached_property (data->upower_proxy, "OnBattery");
  upower_source_update_from_value (data, battery_val);
}

static void
upower_battery_changed (PpdApp *data, gdouble level)
{
  g_info ("Battery level changed to %f", level);

  for (guint i = 0; i < data->actions->len; i++) {
    g_autoptr(GError) error = NULL;
    PpdAction *action;

    action = g_ptr_array_index (data->actions, i);

    if (!ppd_action_get_active (action))
      continue;

    if (!ppd_action_battery_changed (action, level, &error)) {
      g_warning ("failed to update action %s: %s",
                 ppd_action_get_action_name (action),
                 error->message);
      g_clear_error (&error);
      continue;
    }
  }

  if (PPD_IS_DRIVER_CPU (data->cpu_driver)) {
    g_autoptr(GError) error = NULL;

    if (!ppd_driver_battery_changed (PPD_DRIVER (data->cpu_driver), level, &error)) {
      g_warning ("failed to update driver %s: %s",
                 ppd_driver_get_driver_name (PPD_DRIVER (data->cpu_driver)),
                 error->message);
    }
  }

  if (PPD_IS_DRIVER_PLATFORM (data->platform_driver)) {
    g_autoptr(GError) error = NULL;

    if (!ppd_driver_battery_changed (PPD_DRIVER (data->platform_driver), level, &error)) {
      g_warning ("failed to update driver %s: %s",
                 ppd_driver_get_driver_name (PPD_DRIVER (data->platform_driver)),
                 error->message);
    }
  }
}

static void
upower_battery_update (PpdApp *data)
{
  g_autoptr(GVariant) val = NULL;

  val = g_dbus_proxy_get_cached_property (data->upower_display_proxy, "Percentage");
  if (val)
    upower_battery_changed(data, g_variant_get_double (val));
}

static void
upower_properties_changed (GDBusProxy *proxy,
                           GVariant *changed_properties,
                           GStrv invalidated_properties,
                           PpdApp *data)
{
  g_auto(GVariantDict) props_dict = G_VARIANT_DICT_INIT (changed_properties);
  g_autoptr(GVariant) battery_val = NULL;
  g_autoptr(GVariant) percent_val = NULL;

  battery_val = g_variant_dict_lookup_value (&props_dict, "OnBattery",
                                             G_VARIANT_TYPE_BOOLEAN);

  if (battery_val)
    upower_source_update_from_value (data, battery_val);

  percent_val = g_variant_dict_lookup_value (&props_dict, "Percentage",
                                             G_VARIANT_TYPE_DOUBLE);
  if (percent_val)
    upower_battery_changed (data, g_variant_get_double (percent_val));
}

static void
upower_battery_set_power_changed_reason (PpdApp                *data,
                                         PpdPowerChangedReason  reason)
{
  if (data->power_changed_reason == reason)
    return;

  data->power_changed_reason = reason;
  g_info ("Power Changed because of reason %s",
          ppd_power_changed_reason_to_str (reason));

  for (guint i = 0; i < data->actions->len; i++) {
    g_autoptr(GError) error = NULL;
    PpdAction *action;

    action = g_ptr_array_index (data->actions, i);

    if (!ppd_action_get_active (action))
      continue;

    if (!ppd_action_power_changed (action, reason, &error)) {
      g_warning ("failed to update action %s: %s",
                 ppd_action_get_action_name (action),
                 error->message);
      g_clear_error (&error);
      continue;
    }
  }

  if (PPD_IS_DRIVER_CPU (data->cpu_driver)) {
    g_autoptr(GError) error = NULL;

    if (!ppd_driver_power_changed (PPD_DRIVER (data->cpu_driver), reason, &error)) {
      g_warning ("failed to update driver %s: %s",
                 ppd_driver_get_driver_name (PPD_DRIVER (data->cpu_driver)),
                 error->message);
    }
  }

  if (PPD_IS_DRIVER_PLATFORM (data->platform_driver)) {
    g_autoptr(GError) error = NULL;

    if (!ppd_driver_power_changed (PPD_DRIVER (data->platform_driver), reason, &error)) {
      g_warning ("failed to update driver %s: %s",
                 ppd_driver_get_driver_name (PPD_DRIVER (data->platform_driver)),
                 error->message);
    }
  }
}

static void
upower_name_owner_changed (GObject    *object,
                           GParamSpec *pspec,
                           gpointer    user_data)
{
  PpdApp *data = user_data;
  GDBusProxy *upower_proxy = G_DBUS_PROXY (object);
  g_autofree char *name_owner = NULL;

  name_owner = g_dbus_proxy_get_name_owner (upower_proxy);

  if (name_owner != NULL) {
    g_debug ("%s appeared", UPOWER_DBUS_NAME);
    upower_source_update (data);
    return;
  }

  g_debug ("%s vanished", UPOWER_DBUS_NAME);
  upower_battery_set_power_changed_reason (data, PPD_POWER_CHANGED_REASON_UNKNOWN);
}

static void
on_upower_proxy_cb (GObject *source_object,
                    GAsyncResult *res,
                    gpointer user_data)
{
  PpdApp *data = user_data;
  g_autoptr(GDBusProxy) upower_proxy = NULL;
  g_autoptr(GError) error = NULL;

  upower_proxy = g_dbus_proxy_new_finish (res, &error);

  if (upower_proxy == NULL) {
    if (g_error_matches (error, G_IO_ERROR, G_IO_ERROR_CANCELLED))
      return;

    g_warning ("failed to connect to upower: %s", error->message);
    return;
  }

  g_return_if_fail (data->upower_proxy == NULL);
  data->upower_proxy = g_steal_pointer (&upower_proxy);

  data->upower_properties_id = g_signal_connect (data->upower_proxy,
                                                 "g-properties-changed",
                                                 G_CALLBACK (upower_properties_changed),
                                                 data);

  if (!data->upower_display_watch_id) {
    data->upower_watch_id = g_signal_connect (data->upower_proxy,
                                              "notify::g-name-owner",
                                              G_CALLBACK (upower_name_owner_changed),
                                              data);
  }

  upower_source_update (data);
}

static void
on_upower_display_proxy_cb (GObject *source_object,
                            GAsyncResult *res,
                            gpointer user_data)
{
  PpdApp *data = user_data;
  g_autoptr(GDBusProxy) proxy = NULL;
  g_autoptr(GError) error = NULL;

  proxy = g_dbus_proxy_new_finish (res, &error);

  if (proxy == NULL) {
    if (g_error_matches (error, G_IO_ERROR, G_IO_ERROR_CANCELLED))
      return;

    g_warning ("failed to connect to upower: %s", error->message);
    return;
  }

  g_return_if_fail (data->upower_display_proxy == NULL);
  data->upower_display_proxy = g_steal_pointer (&proxy);

  data->upower_display_properties_id = g_signal_connect (data->upower_display_proxy,
                                                         "g-properties-changed",
                                                         G_CALLBACK (upower_properties_changed),
                                                         data);

  if (!data->upower_watch_id) {
    data->upower_display_watch_id = g_signal_connect (data->upower_display_proxy,
                                                      "notify::g-name-owner",
                                                      G_CALLBACK (upower_name_owner_changed),
                                                      data);
  }

  upower_battery_update (data);
}

static void
on_logind_prepare_for_sleep_cb (GDBusConnection *connection,
                                const gchar     *sender_name,
                                const gchar     *object_path,
                                const gchar     *interface_name,
                                const gchar     *signal_name,
                                GVariant        *parameters,
                                gpointer         user_data)
{
  PpdApp *data = user_data;
  gboolean start;

  g_variant_get (parameters, "(b)", &start);

  if (start)
    g_debug ("System preparing for suspend");
  else
    g_debug ("System woke up from suspend");

  if (PPD_IS_DRIVER_CPU (data->cpu_driver)) {
    g_autoptr(GError) error = NULL;

    if (!ppd_driver_prepare_to_sleep (PPD_DRIVER (data->cpu_driver), start, &error)) {
      g_warning ("failed to notify driver %s: %s",
                 ppd_driver_get_driver_name (PPD_DRIVER (data->cpu_driver)),
                 error->message);
    }
  }

  if (PPD_IS_DRIVER_PLATFORM (data->platform_driver)) {
    g_autoptr(GError) error = NULL;

    if (!ppd_driver_prepare_to_sleep (PPD_DRIVER (data->platform_driver), start, &error)) {
      g_warning ("failed to notify driver %s: %s",
                 ppd_driver_get_driver_name (PPD_DRIVER (data->platform_driver)),
                 error->message);
    }
  }
}

static gboolean
has_required_drivers (PpdApp *data)
{
  if (!PPD_IS_DRIVER_CPU (data->cpu_driver) &&
      !PPD_IS_DRIVER_PLATFORM (data->platform_driver))
    return FALSE;

  if (!get_profile_available (data, PPD_PROFILE_BALANCED | PPD_PROFILE_POWER_SAVER))
    return FALSE;

  return TRUE;
}

static void
restart_profile_drivers (PpdApp *data)
{
  stop_profile_drivers (data);
  start_profile_drivers (data);
}

static void
driver_probe_request_cb (PpdDriver *driver,
                         gpointer   user_data)
{
  PpdApp *data = user_data;

  restart_profile_drivers (data);
}

static void
disconnect_array_objects_signals_by_data (GPtrArray *array,
                                          void      *data)
{
  for (guint i = 0; i < array->len; ++i) {
    GObject *object = g_ptr_array_index (array, i);

    g_signal_handlers_disconnect_by_data (object, data);
  }
}

static void
maybe_disconnect_object_by_data (void *object,
                                 void *data)
{
  if (!G_IS_OBJECT (object))
    return;

  g_signal_handlers_disconnect_by_data (object, data);
}

static void
stop_profile_drivers (PpdApp *data)
{
  if (data->logind_sleep_signal_id) {
    g_dbus_connection_signal_unsubscribe (data->connection, data->logind_sleep_signal_id);
    data->logind_sleep_signal_id = 0;
  }

  upower_battery_set_power_changed_reason (data, PPD_POWER_CHANGED_REASON_UNKNOWN);
  release_all_profile_holds (data);
  g_cancellable_cancel (data->cancellable);
  disconnect_array_objects_signals_by_data (data->probed_drivers, data);
  g_ptr_array_set_size (data->probed_drivers, 0);
  disconnect_array_objects_signals_by_data (data->actions, data);
  g_ptr_array_set_size (data->actions, 0);
  g_clear_signal_handler (&data->upower_watch_id, data->upower_proxy);
  g_clear_signal_handler (&data->upower_properties_id, data->upower_proxy);
  g_clear_signal_handler (&data->upower_display_watch_id, data->upower_display_proxy);
  g_clear_signal_handler (&data->upower_display_properties_id, data->upower_display_proxy);
  g_clear_object (&data->cancellable);
  maybe_disconnect_object_by_data (data->upower_proxy, data);
  g_clear_object (&data->upower_proxy);
  maybe_disconnect_object_by_data (data->upower_display_proxy, data);
  g_clear_object (&data->upower_display_proxy);
  maybe_disconnect_object_by_data (data->cpu_driver, data);
  g_clear_object (&data->cpu_driver);
  maybe_disconnect_object_by_data (data->platform_driver, data);
  g_clear_object (&data->platform_driver);
}

static gboolean
action_blocked (PpdApp *app, PpdAction *action)
{
  const gchar *action_name = ppd_action_get_action_name (action);
  g_autoptr(GError) error = NULL;
  gboolean blocked;

  if (app->debug_options->blocked_actions != NULL &&
      g_strv_length (app->debug_options->blocked_actions) != 0) {

    blocked = g_strv_contains ((const gchar *const *) app->debug_options->blocked_actions, action_name);

    if (blocked) {
      g_debug ("Action '%s' is blocked by command line", action_name);
      return TRUE;
    }
  }

  blocked = !g_key_file_get_boolean (app->config, "Actions", action_name, &error);

  /* if not in conffile, fallback to action opt-in */
  if (error != NULL) {
    blocked = ppd_action_get_optin (action);
    g_debug ("Action '%s' is set %s by default", action_name, blocked ? "disabled" : "enabled");
    return blocked;
  }

  g_debug ("Action '%s' is %s by configuration", action_name, blocked ? "disabled" : "enabled");
  return blocked;
}

static gboolean
driver_blocked (PpdApp *app, PpdDriver *driver)
{
  const gchar *driver_name = ppd_driver_get_driver_name (driver);
  gboolean blocked;

  if (app->debug_options->blocked_drivers == NULL || g_strv_length (app->debug_options->blocked_drivers) == 0)
    return FALSE;

  blocked = g_strv_contains ((const gchar *const *) app->debug_options->blocked_drivers, driver_name);
  if (blocked)
    g_debug ("Driver '%s' is blocked", driver_name);
  return blocked;
}

static void
start_profile_drivers (PpdApp *data)
{
  guint i;
  g_autoptr(GError) initial_error = NULL;
  gboolean needs_battery_state_monitor = FALSE;
  gboolean needs_battery_change_monitor = FALSE;
  gboolean needs_suspend_monitor = FALSE;

  data->cancellable = g_cancellable_new ();

  for (i = 0; i < G_N_ELEMENTS (objects); i++) {
    g_autoptr(GObject) object = NULL;

    object = g_object_new (objects[i] (), NULL);

    if (PPD_IS_DRIVER (object)) {
      g_autoptr(PpdDriver) driver = PPD_DRIVER (g_steal_pointer (&object));
      PpdProfile profiles;
      PpdProbeResult result;

      g_debug ("Handling driver '%s'", ppd_driver_get_driver_name (driver));
      if (driver_blocked (data, driver)) {
        g_debug ("Driver '%s' is blocked, skipping", ppd_driver_get_driver_name (driver));
        continue;
      }

      if (PPD_IS_DRIVER_CPU (data->cpu_driver) && PPD_IS_DRIVER_CPU (driver)) {
        g_debug ("CPU driver '%s' already probed, skipping driver '%s'",
                 ppd_driver_get_driver_name (PPD_DRIVER (data->cpu_driver)),
                 ppd_driver_get_driver_name (driver));
        continue;
      }

      if (PPD_IS_DRIVER_PLATFORM (data->platform_driver) && PPD_IS_DRIVER_PLATFORM (driver)) {
        g_debug ("Platform driver '%s' already probed, skipping driver '%s'",
                 ppd_driver_get_driver_name (PPD_DRIVER (data->platform_driver)),
                 ppd_driver_get_driver_name (driver));
        continue;
      }

      profiles = ppd_driver_get_profiles (driver);
      if (!(profiles & PPD_PROFILE_ALL)) {
        g_warning ("Profile Driver '%s' implements invalid profiles '0x%X'",
                   ppd_driver_get_driver_name (driver),
                   profiles);
        continue;
      }

      result = ppd_driver_probe (driver);
      if (result == PPD_PROBE_RESULT_FAIL) {
        g_debug ("probe () failed for driver %s, skipping",
                 ppd_driver_get_driver_name (driver));
        continue;
      }

      if (result == PPD_PROBE_RESULT_DEFER) {
        g_signal_connect (G_OBJECT (driver), "probe-request",
                          G_CALLBACK (driver_probe_request_cb), data);
        g_ptr_array_add (data->probed_drivers, g_steal_pointer (&driver));
        continue;
      }

      if (PPD_IS_DRIVER_CPU (driver))
        g_set_object (&data->cpu_driver, PPD_DRIVER_CPU (driver));
      else if (PPD_IS_DRIVER_PLATFORM (driver))
        g_set_object (&data->platform_driver, PPD_DRIVER_PLATFORM (driver));
      else
        g_return_if_reached ();

      if (PPD_DRIVER_GET_CLASS (driver)->power_changed != NULL)
        needs_battery_state_monitor = TRUE;

      if (PPD_DRIVER_GET_CLASS (driver)->battery_changed != NULL)
        needs_battery_change_monitor = TRUE;

      if (PPD_DRIVER_GET_CLASS (driver)->prepare_to_sleep != NULL)
        needs_suspend_monitor = TRUE;

      g_info ("Driver '%s' loaded", ppd_driver_get_driver_name (driver));

      g_signal_connect (G_OBJECT (driver), "notify::performance-degraded",
                        G_CALLBACK (driver_performance_degraded_changed_cb), data);
      g_signal_connect (G_OBJECT (driver), "profile-changed",
                        G_CALLBACK (driver_profile_changed_cb), data);
      continue;
    }

    if (PPD_IS_ACTION (object)) {
      g_autoptr(PpdAction) action = PPD_ACTION (g_steal_pointer (&object));

      g_debug ("Handling action '%s'", ppd_action_get_action_name (action));

      if (action_blocked (data, action)) {
          ppd_action_set_active (action, FALSE);
      } else {
        switch (ppd_action_probe(action)) {
        case PPD_PROBE_RESULT_SUCCESS:
          ppd_action_set_active (action, TRUE);
          break;
        default:
          ppd_action_set_active (action, FALSE);
          break;
        }
      }

      if (PPD_ACTION_GET_CLASS (action)->power_changed != NULL)
        needs_battery_state_monitor = TRUE;

      if (PPD_ACTION_GET_CLASS (action)->battery_changed != NULL)
        needs_battery_change_monitor = TRUE;

      g_info ("Action '%s' active %d",
              ppd_action_get_action_name (action),
              ppd_action_get_active (action));
      g_ptr_array_add (data->actions, g_steal_pointer (&action));
      continue;
    }

    g_return_if_reached ();
  }

  if (!has_required_drivers (data)) {
    data->ret = EXIT_FAILURE;
    g_warning ("Some non-optional profile drivers are missing, programmer error");
    g_main_loop_quit (data->main_loop);
  }

  /* Set initial state either from configuration, or using the currently selected profile */
  apply_configuration (data);
  if (!activate_target_profile (data, data->active_profile, PPD_PROFILE_ACTIVATION_REASON_RESET, &initial_error))
    g_warning ("Failed to activate initial profile: %s", initial_error->message);

  send_dbus_event (data, PROP_ALL);
  data->was_started = TRUE;

  if (data->debug_options->disable_upower)
    data->battery_support = FALSE;

  if (!data->battery_support) {
    g_debug ("upower is disabled, let's skip it");
  } else if (needs_battery_state_monitor || needs_battery_change_monitor) {
    /* start watching for power changes */
    if (needs_battery_state_monitor) {
      g_debug ("Battery state monitor required, connecting to upower...");
      g_dbus_proxy_new (data->connection,
                        G_DBUS_PROXY_FLAGS_DO_NOT_AUTO_START |
                        G_DBUS_PROXY_FLAGS_DO_NOT_CONNECT_SIGNALS,
                        NULL,
                        UPOWER_DBUS_NAME,
                        UPOWER_DBUS_PATH,
                        UPOWER_DBUS_INTERFACE,
                        data->cancellable,
                        on_upower_proxy_cb,
                        data);
    }

    if (needs_battery_change_monitor) {
      g_debug ("Battery change monitor required, connecting to upower...");
      g_dbus_proxy_new (data->connection,
                        G_DBUS_PROXY_FLAGS_DO_NOT_AUTO_START |
                        G_DBUS_PROXY_FLAGS_DO_NOT_CONNECT_SIGNALS,
                        NULL,
                        UPOWER_DBUS_NAME,
                        UPOWER_DBUS_DISPLAY_DEVICE_PATH,
                        UPOWER_DBUS_DEVICE_INTERFACE,
                        data->cancellable,
                        on_upower_display_proxy_cb,
                        data);
    }
  } else {
    g_debug ("No battery state monitor required by any driver, let's skip it");
  }

  if (data->debug_options->disable_logind) {
    g_debug ("logind is disabled, let's skip it");
  } else if (needs_suspend_monitor) {
    g_debug ("Suspension state monitor required, monitoring logind...");
    data->logind_sleep_signal_id =
      g_dbus_connection_signal_subscribe (data->connection,
                                          LOGIND_DBUS_NAME,
                                          LOGIND_DBUS_INTERFACE,
                                          "PrepareForSleep",
                                          LOGIND_DBUS_PATH,
                                          NULL,
                                          G_DBUS_SIGNAL_FLAGS_NONE,
                                          on_logind_prepare_for_sleep_cb,
                                          data,
                                          NULL);
  } else {
    g_debug ("No suspension monitor required by any driver, let's skip it");
  }
}

void
restart_profile_drivers_for_default_app (void)
{
  restart_profile_drivers (ppd_app);
}


static void
name_acquired_handler (GDBusConnection *connection,
                       const gchar     *name,
                       gpointer         user_data)
{
  PpdBusOwnData *data = user_data;

  g_debug ("Name '%s' acquired", name);

  start_profile_drivers (data->app);
}

static gboolean
setup_dbus (PpdApp    *data,
            GError   **error)
{
  g_autoptr(GBytes) iface_data = NULL;
  g_autoptr(GBytes) legacy_iface_data = NULL;
  g_autoptr(GDBusNodeInfo) introspection_data = NULL;
  g_autoptr(GDBusNodeInfo) legacy_introspection_data = NULL;
  PpdBusOwnData *own_data;

  iface_data = g_resources_lookup_data (POWER_PROFILES_RESOURCES_PATH "/"
                                        POWER_PROFILES_DBUS_NAME ".xml",
                                        G_RESOURCE_LOOKUP_FLAGS_NONE,
                                        error);
  if (!iface_data)
    return FALSE;

  legacy_iface_data = g_resources_lookup_data (POWER_PROFILES_RESOURCES_PATH "/"
                                               POWER_PROFILES_LEGACY_DBUS_NAME ".xml",
                                               G_RESOURCE_LOOKUP_FLAGS_NONE,
                                               error);
  if (!legacy_iface_data)
    return FALSE;

  introspection_data =  g_dbus_node_info_new_for_xml (g_bytes_get_data (iface_data, NULL),
                                                      error);
  if (!introspection_data)
    return FALSE;

  legacy_introspection_data =  g_dbus_node_info_new_for_xml (g_bytes_get_data (legacy_iface_data, NULL),
                                                             error);
  if (!legacy_introspection_data)
    return FALSE;

  own_data = g_new0 (PpdBusOwnData, 1);
  own_data->app = data;
  own_data->interface = g_dbus_interface_info_ref (introspection_data->interfaces[0]);
  own_data->legacy_interface = g_dbus_interface_info_ref (legacy_introspection_data->interfaces[0]);

  own_data->flags = G_BUS_NAME_OWNER_FLAGS_ALLOW_REPLACEMENT;
  if (data->debug_options->replace)
    own_data->flags |= G_BUS_NAME_OWNER_FLAGS_REPLACE;

  data->name_id = g_bus_own_name (G_BUS_TYPE_SYSTEM,
                                  POWER_PROFILES_DBUS_NAME,
                                  own_data->flags,
                                  bus_acquired_handler,
                                  name_acquired_handler,
                                  name_lost_handler,
                                  own_data,
                                  (GDestroyNotify) ppd_bus_own_data_free);

  return TRUE;
}

static void
free_app_data (PpdApp *data)
{
  if (data == NULL)
    return;

  stop_profile_drivers (data);

  g_clear_handle_id (&data->name_id, g_bus_unown_name);
  g_clear_handle_id (&data->legacy_name_id, g_bus_unown_name);

  g_clear_pointer (&data->debug_options, debug_options_free);
  g_clear_pointer (&data->config_path, g_free);
  g_clear_pointer (&data->config, g_key_file_unref);
  g_clear_pointer (&data->probed_drivers, g_ptr_array_unref);
  g_clear_pointer (&data->actions, g_ptr_array_unref);
  g_clear_object (&data->cpu_driver);
  g_clear_object (&data->platform_driver);
  g_hash_table_destroy (data->profile_holds);

  g_clear_object (&data->auth);

  g_clear_pointer (&data->main_loop, g_main_loop_unref);
  g_clear_object (&data->connection);
  g_free (data);
  ppd_app = NULL;
}

G_DEFINE_AUTOPTR_CLEANUP_FUNC (PpdApp, free_app_data)

void
main_loop_quit (void)
{
  g_main_loop_quit (ppd_app->main_loop);
}

static inline gboolean
use_colored_ouput (void)
{
  if (g_getenv ("NO_COLOR"))
    return FALSE;
  return isatty (fileno (stdout));
}

static void
debug_handler_cb (const gchar *log_domain,
                  GLogLevelFlags log_level,
                  const gchar *message,
                  gpointer user_data)
{
  DebugOptions *data = user_data;
  g_autoptr(GString) domain = NULL;
  gboolean use_color;
  gint color;

  /* not in verbose mode */
  if (log_level > data->log_level)
   return;

  domain = g_string_new (log_domain);
  use_color = use_colored_ouput ();

  for (gsize i = domain->len; i < 15; i++)
    g_string_append_c (domain, ' ');
  g_print ("%s", domain->str);

  switch (log_level) {
  case G_LOG_LEVEL_ERROR:
  case G_LOG_LEVEL_CRITICAL:
  case G_LOG_LEVEL_WARNING:
    color = 31; /* red */
    break;
  default:
    color = 34; /* blue */
    break;
  }

  if (use_color)
    g_print ("%c[%dm%s\n%c[%dm", 0x1B, color, message, 0x1B, 0);
  else
    g_print ("%s\n", message);
}

static gboolean
quit_signal_callback (gpointer user_data)
{
  PpdApp *data = user_data;

  g_main_loop_quit (data->main_loop);
  return FALSE;
}

static gboolean
verbose_arg_cb (const gchar *option_name,
                const gchar *value,
                gpointer user_data,
                GError **error)
{
  DebugOptions *data = user_data;

  if (data->log_level == G_LOG_LEVEL_MESSAGE) {
          data->log_level = G_LOG_LEVEL_INFO;
          return TRUE;
  }
  if (data->log_level == G_LOG_LEVEL_INFO) {
          data->log_level = G_LOG_LEVEL_DEBUG;
          return TRUE;
  }
  g_set_error_literal (error,
                       G_OPTION_ERROR,
                       G_OPTION_ERROR_FAILED,
                       "No further debug level supported");
  return FALSE;
}

static gboolean
debug_pre_parse_hook (GOptionContext *context,
                      GOptionGroup *group,
                      gpointer user_data,
                      GError **error)
{
  DebugOptions *data = user_data;

  const GOptionEntry options[] = {
    {
      "verbose",
      'v',
      G_OPTION_FLAG_NO_ARG,
      G_OPTION_ARG_CALLBACK,
      (GOptionArgFunc)verbose_arg_cb,
      "Show extra debugging information",
      NULL,
    },
    {
      "replace",
      'r',
      G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE,
      &data->replace,
      "Replace the running instance of power-profiles-daemon",
      NULL,
    },
    {
      "block-driver",
      0,
      G_OPTION_FLAG_NONE,
      G_OPTION_ARG_STRING_ARRAY,
      &data->blocked_drivers,
      "Block driver(s) from loading",
      NULL,
    },
    {
      "block-action",
      0,
      G_OPTION_FLAG_NONE,
      G_OPTION_ARG_STRING_ARRAY,
      &data->blocked_actions,
      "Block action(s) from loading",
      NULL,
    },
    {
      "disable-upower",
      0,
      G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE,
      &data->disable_upower,
      "Disable upower integration",
      NULL,
    },
    {
      "disable-logind",
      0,
      G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE,
      &data->disable_logind,
      "Disable logind integration",
      NULL,
    },
    { NULL }
  };
  g_option_group_add_entries (group, options);

  return TRUE;
}

static gboolean
debug_post_parse_hook (GOptionContext *context,
                       GOptionGroup *group,
                       gpointer user_data,
                       GError **error)
{
  DebugOptions *debug = (DebugOptions *)user_data;

  g_log_set_default_handler (debug_handler_cb, debug);

  return TRUE;
}

int main (int argc, char **argv)
{
  g_autoptr(DebugOptions) debug_options = g_new0 (DebugOptions, 1);
  g_autoptr(PpdApp) data = NULL;
  g_autoptr(GOptionContext) option_context = NULL;
  g_autoptr(GError) error = NULL;

  debug_options->log_level = G_LOG_LEVEL_MESSAGE;
  debug_options->group = g_option_group_new ("debug",
                                             "Debugging Options",
                                             "Show debugging options",
                                             debug_options,
                                             NULL);
  g_option_group_set_parse_hooks (debug_options->group,
                                  debug_pre_parse_hook,
                                  debug_post_parse_hook);

  setlocale (LC_ALL, "");
  option_context = g_option_context_new ("");
  g_option_context_add_group (option_context, debug_options->group);

  if (!g_option_context_parse (option_context, &argc, &argv, &error)) {
    g_print ("Failed to parse arguments: %s\n", error->message);
    return EXIT_FAILURE;
  }

  data = g_new0 (PpdApp, 1);
  data->main_loop = g_main_loop_new (NULL, TRUE);
  data->auth = polkit_authority_get_sync (NULL, NULL);
  data->probed_drivers = g_ptr_array_new_with_free_func ((GDestroyNotify) g_object_unref);
  data->actions = g_ptr_array_new_with_free_func ((GDestroyNotify) g_object_unref);
  data->profile_holds = g_hash_table_new_full (g_direct_hash, g_direct_equal, NULL, (GDestroyNotify) profile_hold_free);
  data->active_profile = PPD_PROFILE_BALANCED;
  data->selected_profile = PPD_PROFILE_BALANCED;
  data->debug_options = g_steal_pointer(&debug_options);

  g_unix_signal_add (SIGTERM, quit_signal_callback, data);
  g_unix_signal_add (SIGINT, quit_signal_callback, data);

  g_info ("Starting power-profiles-daemon version "VERSION);

  load_configuration (data);
  ppd_app = data;

  /* Set up D-Bus */
  if (!setup_dbus (data, &error)) {
    g_error ("Failed to start dbus: %s", error->message);
    return EXIT_FAILURE;
  }

  g_main_loop_run (data->main_loop);

  return data->ret;
}
