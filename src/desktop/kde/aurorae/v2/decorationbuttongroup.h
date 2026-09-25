/*
    SPDX-FileCopyrightText: 2025 Martín Durán <mduran@nvan.es>

    SPDX-License-Identifier: GPL-2.0-or-later
*/

#pragma once

#include <KDecoration3/DecorationButtonGroup>

namespace Aurorae
{

class DecorationButtonGroup : public KDecoration3::DecorationButtonGroup
{
    Q_OBJECT

public:
    using KDecoration3::DecorationButtonGroup::DecorationButtonGroup;

    bool eventFilter(QObject *obj, QEvent *event) override;

    void setButtonGroupHover(bool hover);

private:
    QRectF boundingRect() const;
    void applyHoveredToAllButtons(bool hovered);

    bool m_hovered = false;
    bool m_buttonGroupHover = false;
};

}
