/*
 *  SPDX-FileCopyrightText: 2023 ivan tkachenko <me@ratijas.tk>
 *
 *  SPDX-License-Identifier: LGPL-2.0-or-later
 */

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import QtTest

TestCase {
    id: root

    name: "MenuDialogTest"
    visible: true
    when: windowShown

    width: 300
    height: 300

    Component {
        id: menuDialogComponent
        Kirigami.MenuDialog {
            readonly property Kirigami.Action actionA: Kirigami.Action {
                text: "Action A"
            }

            preferredWidth: 200

            actions: [actionA]
        }
    }

    Component {
        id: spyComponent
        SignalSpy {}
    }

    function findChildIf(parent: Item, predicate /*(Item) -> bool*/): Item {
        for (const child of parent.children) {
            if (predicate(child)) {
                return child;
            } else {
                const item = findChildIf(child, predicate);
                if (item !== null) {
                    return item;
                }
            }
        }
        return null;
    }

    function test_closed() {
        const dialog = createTemporaryObject(menuDialogComponent, this);
        verify(dialog);

        const { actionA } = dialog;

        const dialogClosedSpy = createTemporaryObject(spyComponent, this, {
            target: dialog,
            signalName: "closed",
        });
        const actionSpy = createTemporaryObject(spyComponent, this, {
            target: actionA,
            signalName: "triggered",
        });

        dialog.open();
        tryVerify(() => dialog.opened);

        const delegate = findChildIf(dialog.contentItem, item => item.action === actionA) as QQC2.ItemDelegate;
        verify(delegate);

        mouseClick(delegate);
        compare(actionSpy.count, 1);
        expectFailContinue("", "closed signal is not actually emitted")
        compare(dialogClosedSpy.count, 1);
        tryVerify(() => !dialog.visible);
    }
}
