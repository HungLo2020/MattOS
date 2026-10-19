// Copyright (C) 2017 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR GPL-3.0-only

import QtQuick
import QtTest
import QtQuick.Controls
import QtQuick.Templates as T

TestCase {
    id: testCase
    width: 400
    height: 400
    visible: true
    when: windowShown
    name: "Action"

    Component {
        id: component
        Action { }
    }

    Component {
        id: signalSpy
        SignalSpy { }
    }

    function init() {
        failOnWarning(/.?/)
    }

    function test_empty() {
        let control = createTemporaryObject(component, testCase)
        verify(control)
    }

    function test_enabled() {
        let action = createTemporaryObject(component, testCase)
        verify(action)

        let spy = createTemporaryObject(signalSpy, testCase, {target: action, signalName: "triggered"})
        verify(spy.valid)

        action.trigger()
        compare(spy.count, 1)

        action.enabled = false
        action.trigger()
        compare(spy.count, 1)

        action.enabled = undefined // reset
        action.trigger()
        compare(spy.count, 2)
    }

    Component {
        id: buttonAndMenu
        Item {
            objectName: "container"

            property alias button: button
            property alias menu: menu
            property alias menuItem: menuItem
            property alias action: sharedAction
            property var lastSource

            Action {
                id: sharedAction
                objectName: "sharedAction"
                text: "Shared"
                shortcut: "Ctrl+B"
                onTriggered: (source) => lastSource = source
            }
            Button {
                id: button
                objectName: "button"
                action: sharedAction

                Menu {
                    id: menu
                    objectName: "menu"
                    contentItem.objectName: "menuContentItem"

                    MenuItem {
                        id: menuItem
                        objectName: "menuItem"
                        action: sharedAction
                    }
                }
            }
        }
    }

    function test_shared() {
        let container = createTemporaryObject(buttonAndMenu, testCase)
        verify(container)

        keyClick(Qt.Key_B, Qt.ControlModifier)
        compare(container.lastSource, container.button)

        container.menu.open()
        keyClick(Qt.Key_B, Qt.ControlModifier)
        compare(container.lastSource, container.menuItem)

        tryVerify(function() { return !container.menu.visible })
        keyClick(Qt.Key_B, Qt.ControlModifier)
        compare(container.lastSource, container.button)

        container.button.visible = false
        keyClick(Qt.Key_B, Qt.ControlModifier)
        compare(container.lastSource, container.action)
    }

    Component {
        id: actionAndRepeater
        Item {
            property alias action: testAction
            Action {
                id: testAction
                shortcut: "Ctrl+A"
            }
            Repeater {
                model: 1
                Button {
                    action: testAction
                }
            }
        }
    }

    function test_repeater() {
        let container = createTemporaryObject(actionAndRepeater, testCase)
        verify(container)

        let spy = signalSpy.createObject(container, {target: container.action, signalName: "triggered"})
        verify(spy.valid)

        keyClick(Qt.Key_A, Qt.ControlModifier)
        compare(spy.count, 1)
    }

    Component {
        id: shortcutBinding
        Item {
            Action {
                id: action
                shortcut: StandardKey.Copy
            }

            Shortcut {
                id: indirectShortcut
                sequences: [ action.shortcut ]
            }

            Shortcut {
                id: directShortcut
                sequences: [ StandardKey.Copy ]
            }

            property alias indirect: indirectShortcut;
            property alias direct: directShortcut
        }
    }

    function test_shortcutBinding() {
        let container = createTemporaryObject(shortcutBinding, testCase);
        verify(container)
        compare(container.indirect.nativeText, container.direct.nativeText);
    }

    Component {
        id: shortcutCleanup
        Item {
            property alias page: page
            property alias action: action
            property alias menu: menu
            Item {
                id: page
                Action {
                    id: action
                    text: "action"
                    shortcut: "Insert"
                }
                Menu {
                    id: menu
                    MenuItem { action: action }
                }
            }
        }
    }

    function test_shortcutCleanup() {
        {
            let container = createTemporaryObject(shortcutCleanup, testCase);
            verify(container)
            container.action.shortcut = "Delete"
            container.menu.open()
            container.page.destroy()
            tryVerify(function() { return !container.page })
        }
        keyClick(Qt.Key_Delete, Qt.NoModifier)
    }


    Component {
        id: hiddenPopupComponent

        Item {
            property alias popup: popup

            Popup {
                id: popup

                property alias button: button

                Button {
                    id: button
                    objectName: popup.objectName + "Button"
                    text: "Pressing R once should close this popup"
                    action: Action {
                        objectName: popup.objectName + "Action"
                        shortcut: "r"
                    }
                    onClicked: popup.close()
                }
            }
        }
    }

    function test_actionInHiddenPopupShouldNotTrigger() {
        let container = createTemporaryObject(hiddenPopupComponent, testCase)
        verify(container)

        // An Action in a Button that's not visible shouldn't grab shortcuts.
        let hiddenPopup = container.popup
        let hiddenButtonClickedSpy = createTemporaryObject(signalSpy, testCase,
            { target: hiddenPopup.button, signalName: "clicked" })
        keyClick(Qt.Key_R)
        verify(hiddenButtonClickedSpy.valid)
        expectFail("", "### Qt 7: don't allow Actions in hidden items to be triggered: QTBUG-145384")
        compare(hiddenButtonClickedSpy.count, 0)
    }
}
