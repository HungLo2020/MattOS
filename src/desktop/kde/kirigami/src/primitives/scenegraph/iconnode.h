/*
 *  SPDX-FileCopyrightText: 2026 Volodymyr Zolotopupov <zvova7890@gmail.com>
 *
 *  SPDX-License-Identifier: LGPL-2.0-or-later
 */

#pragma once

#include "shadernode.h"

class IconNode : public ShaderNode
{
public:
    IconNode() = default;

protected:
    QSGMaterial *createMaterialVariant(QSGMaterialType *variant) override;
};
