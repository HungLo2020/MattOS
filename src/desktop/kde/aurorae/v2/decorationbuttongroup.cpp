/*
    SPDX-FileCopyrightText: 2025 Martín Durán <mduran@nvan.es>

    SPDX-License-Identifier: GPL-2.0-or-later
*/

#include "decorationbuttongroup.h"
#include "decorationbutton.h"

#include <KDecoration3/Decoration>
#include <QHoverEvent>

namespace Aurorae
{

void DecorationButtonGroup::setButtonGroupHover(bool hover)
{
    if (m_buttonGroupHover == hover) {
        return;
    }

    m_buttonGroupHover = hover;

    if (hover) {
        decoration()->installEventFilter(this);
    } else {
        decoration()->removeEventFilter(this);
    }
}

QRectF DecorationButtonGroup::boundingRect() const
{
    QRectF rect;
    const auto buttonList = buttons();
    for (const auto button : buttonList) {
        if (!button->isVisible()) {
            continue;
        }
        rect = rect.united(button->geometry());
    }
    return rect;
}

bool DecorationButtonGroup::eventFilter(QObject *obj, QEvent *event)
{
    if (event->type() == QEvent::HoverMove) {
        const bool hovered = boundingRect().contains(static_cast<QHoverEvent *>(event)->pos());

        if (m_hovered != hovered) {
            applyHoveredToAllButtons(hovered);
            m_hovered = hovered;
        }
    } else if (event->type() == QEvent::HoverLeave && m_hovered) {
        m_hovered = false;
        applyHoveredToAllButtons(false);
    }
    return KDecoration3::DecorationButtonGroup::eventFilter(obj, event);
}

void DecorationButtonGroup::applyHoveredToAllButtons(bool hovered)
{
    const auto buttonList = buttons();
    for (const auto button : buttonList) {
        static_cast<DecorationButton *>(button)->setParentHovered(hovered);
    }
}

}
