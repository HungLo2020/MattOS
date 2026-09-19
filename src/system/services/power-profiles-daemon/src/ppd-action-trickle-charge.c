/*
 * Copyright (c) 2020 Bastien Nocera <hadess@hadess.net>
 *
 * This program is free software; you can redistribute it and/or modify it
 * under the terms of the GNU General Public License version 3 as published by
 * the Free Software Foundation.
 *
 */

#define G_LOG_DOMAIN "TrickleChargeAction"

#include <gudev/gudev.h>

#include "ppd-action-trickle-charge.h"
#include "ppd-profile.h"
#include "ppd-utils.h"

#define CHARGE_TYPE_SYSFS_NAME "charge_type"

typedef enum {
  PPD_CHARGE_TYPE_UNKNOWN,
  PPD_CHARGE_TYPE_STANDARD,
  PPD_CHARGE_TYPE_FAST,
  PPD_CHARGE_TYPE_TRICKLE,
  PPD_CHARGE_TYPE_CUSTOM,
  PPD_CHARGE_TYPE_ADAPTIVE,
} PpdChargeType;

struct _PpdActionTrickleCharge
{
  PpdAction  parent_instance;

  GUdevClient *client;
  PpdChargeType charge_type;
};

G_DEFINE_TYPE (PpdActionTrickleCharge, ppd_action_trickle_charge, PPD_TYPE_ACTION)

static GObject*
ppd_action_trickle_charge_constructor (GType                  type,
                                            guint                  n_construct_params,
                                            GObjectConstructParam *construct_params)
{
  GObject *object;

  object = G_OBJECT_CLASS (ppd_action_trickle_charge_parent_class)->constructor (type,
                                                                                n_construct_params,
                                                                                construct_params);
  g_object_set (object,
                "action-name", "trickle_charge",
                "action-description", "Configure power supply to trickle charge",
                "action-optin", FALSE,
                NULL);

  return object;
}

PpdChargeType
ppd_string_to_charge_type (const gchar *str)
{
  if (g_str_equal (str, "Standard"))
    return PPD_CHARGE_TYPE_STANDARD;
  if (g_str_equal (str, "Fast"))
    return PPD_CHARGE_TYPE_FAST;
  if (g_str_equal (str, "Trickle"))
    return PPD_CHARGE_TYPE_TRICKLE;
  if (g_str_equal (str, "Custom"))
    return PPD_CHARGE_TYPE_CUSTOM;
  if (g_str_equal (str, "Adaptive"))
    return PPD_CHARGE_TYPE_ADAPTIVE;

  return PPD_CHARGE_TYPE_UNKNOWN;
}

static const char *
ppd_charge_type_to_string (PpdChargeType charge_type)
{
  switch (charge_type) {
  case PPD_CHARGE_TYPE_STANDARD:
    return "Standard";
  case PPD_CHARGE_TYPE_FAST:
    return "Fast";
  case PPD_CHARGE_TYPE_TRICKLE:
    return "Trickle";
  case PPD_CHARGE_TYPE_CUSTOM:
    return "Custom";
  case PPD_CHARGE_TYPE_ADAPTIVE:
    return "Adaptive";
  default:
    return "Unknown";
  }
}

static void
set_charge_type (PpdActionTrickleCharge *action,
                 PpdChargeType           charge_type)
{
  PpdActionTrickleCharge *self = PPD_ACTION_TRICKLE_CHARGE (action);
  g_autolist (GUdevDevice) devices = NULL;
  const char *charge_type_value;

  devices = g_udev_client_query_by_subsystem (action->client, "power_supply");
  if (devices == NULL)
    return;

  charge_type_value = ppd_charge_type_to_string (charge_type);

  for (GList *l = devices; l != NULL; l = l->next) {
    GUdevDevice *dev = l->data;
    const char *value;

    if (g_strcmp0 (g_udev_device_get_sysfs_attr (dev, "scope"), "Device") != 0)
      continue;

    value = g_udev_device_get_sysfs_attr_uncached (dev, CHARGE_TYPE_SYSFS_NAME);
    if (!value)
      continue;

    if (g_str_equal (value, charge_type_value))
      continue;

    switch (ppd_string_to_charge_type (value)) {
    case PPD_CHARGE_TYPE_CUSTOM:
    case PPD_CHARGE_TYPE_ADAPTIVE:
      g_debug ("Not setting charge type for '%s' due to '%s'",
               g_udev_device_get_sysfs_path (dev), value);
      break;

    default:
      ppd_utils_write_sysfs (dev, CHARGE_TYPE_SYSFS_NAME, charge_type_value, NULL);
      break;
    }
  }

  self->charge_type = charge_type;
}

static gboolean
ppd_action_trickle_charge_activate_profile (PpdAction   *action,
                                            PpdProfile   profile,
                                            GError     **error)
{
  PpdActionTrickleCharge *self = PPD_ACTION_TRICKLE_CHARGE (action);
  PpdChargeType charge_type;

  switch (profile) {
  case PPD_PROFILE_POWER_SAVER:
    charge_type = PPD_CHARGE_TYPE_TRICKLE;
    break;
  case PPD_PROFILE_BALANCED:
    charge_type = PPD_CHARGE_TYPE_STANDARD;
    break;
  case PPD_PROFILE_PERFORMANCE:
    charge_type = PPD_CHARGE_TYPE_FAST;
    break;
  default:
    g_set_error (error, G_IO_ERROR, G_IO_ERROR_FAILED,
                 "Unknown profile %d", profile);
    return FALSE;
  }

  set_charge_type (self, charge_type);

  return TRUE;
}

static void
uevent_cb (GUdevClient *client,
           gchar       *action,
           GUdevDevice *device,
           gpointer     user_data)
{
  PpdActionTrickleCharge *self = user_data;

  if (g_strcmp0 (action, "add") != 0)
    return;

  if (!g_udev_device_has_sysfs_attr (device, CHARGE_TYPE_SYSFS_NAME))
    return;

  set_charge_type (self, self->charge_type);
}

static void
ppd_action_trickle_charge_finalize (GObject *object)
{
  PpdActionTrickleCharge *driver;

  driver = PPD_ACTION_TRICKLE_CHARGE (object);
  g_clear_object (&driver->client);
  G_OBJECT_CLASS (ppd_action_trickle_charge_parent_class)->finalize (object);
}

static void
ppd_action_trickle_charge_class_init (PpdActionTrickleChargeClass *klass)
{
  GObjectClass *object_class;
  PpdActionClass *driver_class;

  object_class = G_OBJECT_CLASS (klass);
  object_class->constructor = ppd_action_trickle_charge_constructor;
  object_class->finalize = ppd_action_trickle_charge_finalize;

  driver_class = PPD_ACTION_CLASS (klass);
  driver_class->activate_profile = ppd_action_trickle_charge_activate_profile;
}

static void
ppd_action_trickle_charge_init (PpdActionTrickleCharge *self)
{
  const gchar * const subsystem[] = { "power_supply", NULL };

  self->client = g_udev_client_new (subsystem);
  g_signal_connect (G_OBJECT (self->client), "uevent",
                    G_CALLBACK (uevent_cb), self);
}
