/*
 * Copyright (c) 2020 Bastien Nocera <hadess@hadess.net>
 *
 * This program is free software; you can redistribute it and/or modify it
 * under the terms of the GNU General Public License version 3 as published by
 * the Free Software Foundation.
 *
 */

#include "ppd-profile.h"
#include "ppd-enums.h"

const char *
ppd_profile_to_str (PpdProfile profile)
{
  g_autoptr(GFlagsClass) klass = g_type_class_ref (PPD_TYPE_PROFILE);
  GFlagsValue *value = g_flags_get_first_value (klass, profile);
  const gchar *name = value ? value->value_nick : "";
  return name;
}

PpdProfile
ppd_profile_from_str (const char *str)
{
  g_autoptr(GFlagsClass) klass = g_type_class_ref (PPD_TYPE_PROFILE);
  GFlagsValue *value = g_flags_get_value_by_nick (klass, str);
  PpdProfile profile = value ? value->value : PPD_PROFILE_UNSET;
  return profile;
}

gboolean
ppd_profile_has_single_flag (PpdProfile profile)
{
  g_autoptr(GFlagsClass) klass = g_type_class_ref (PPD_TYPE_PROFILE);
  GFlagsValue *value = g_flags_get_first_value (klass, profile);
  if (value && value->value == profile)
    return TRUE;

  return FALSE;
}

const char *
ppd_power_changed_reason_to_str (PpdPowerChangedReason reason)
{
  g_autoptr(GEnumClass) klass = g_type_class_ref (PPD_TYPE_POWER_CHANGED_REASON);
  GEnumValue *value = g_enum_get_value (klass, reason);
  const gchar *name = value ? value->value_nick : "";
  return name;
}
