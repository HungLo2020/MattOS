/*
 *  SPDX-FileCopyrightText: 2026 Nate Graham <nate@kde.org>
 *
 *  SPDX-License-Identifier: LGPL-2.0-or-later
 */

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import QtTest

TestCase {
    name: "Badge"
    visible: true
    when: windowShown

    width: 500
    height: 500

    Component {
        id: textBadgeComponent
        Kirigami.Badge {
            text: "boom"
        }
    }

    Component {
        id: iconBadgeComponent
        Kirigami.Badge {
            icon.name: "edit-bomb-symbolic"
        }
    }

    Component {
        id: iconAndTextBadgeComponent
        Kirigami.Badge {
            icon.name: "edit-bomb-symbolic"
            text: "boom"
        }
    }

    function test_textBadge() {
        const badge = createTemporaryObject(textBadgeComponent, this);
        verify(badge);
    }

    function test_iconBadge() {
        const badge = createTemporaryObject(iconBadgeComponent, this);
        verify(badge);
    }

    function test_iconAndTextBadge() {
        const badge = createTemporaryObject(iconAndTextBadgeComponent, this);
        verify(badge);
    }
}
