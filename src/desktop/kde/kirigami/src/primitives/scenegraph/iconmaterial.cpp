/*
 *  SPDX-FileCopyrightText: 2026 Volodymyr Zolotopupov <zvova7890@gmail.com>
 *
 *  SPDX-License-Identifier: LGPL-2.0-or-later
 */

#include "iconmaterial.h"

#include <array>
#include <cstring>

void IconMaterial::updateRenderStateUniforms(const QSGMaterialShader::RenderState &state)
{
    const auto viewport = state.viewportRect();
    const float w = viewport.width();
    const float h = viewport.height();

    const std::array<float, 4> viewportData = {
        w * 0.5f,
        h * 0.5f,
        2.0f / w,
        2.0f / h,
    };

    memcpy(uniformData().data() + sizeof(float) * 24, viewportData.data(), sizeof(viewportData));
}
