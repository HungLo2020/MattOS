/*
 * Copyright (c) 2024 Advanced Micro Devices
 *
 * This program is free software; you can redistribute it and/or modify it
 * under the terms of the GNU General Public License version 3 as published by
 * the Free Software Foundation.
 *
 */

#pragma once

#include "ppd-action.h"

#define PPD_TYPE_ACTION_AMDGPU_DPM (ppd_action_amdgpu_dpm_get_type())
G_DECLARE_FINAL_TYPE (PpdActionAmdgpuDpm, ppd_action_amdgpu_dpm, PPD, ACTION_AMDGPU_DPM, PpdAction)
