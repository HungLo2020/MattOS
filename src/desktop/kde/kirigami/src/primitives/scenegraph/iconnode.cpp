/*
 *  SPDX-FileCopyrightText: 2026 Volodymyr Zolotopupov <zvova7890@gmail.com>
 *
 *  SPDX-License-Identifier: LGPL-2.0-or-later
 */

#include "iconnode.h"

#include "iconmaterial.h"

QSGMaterial *IconNode::createMaterialVariant(QSGMaterialType *variant)
{
    return new IconMaterial(variant);
}
