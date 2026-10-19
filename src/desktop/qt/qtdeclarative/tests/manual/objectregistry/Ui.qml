// Copyright (C) 2026 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR GPL-3.0-only

// This file simulates a UI generated with a design tool, which developers are not
// allowed to touch as design tool would overwrite their changes.
// To allow access to UI objects in hand-written business logic elsewhere, the design tool has
// registered the required items with ObjectRegistry.

import QtQuick
import QtQml.DesignSupport

Item {
    id: root

    Rectangle {
        id: firstRectangle
        x: 10
        y: 50
        width: 100
        height: 100
        color: "red"

        objectName: "First Rectangle"

        ObjectRegistry.key: "FirstRect"
    }

    Rectangle {
        id: secondRectangle
        x: 110
        y: 50
        width: 100
        height: 100
        color: "blue"

        objectName: "Second Rectangle"

        ObjectRegistry.key: "SecondRect"
    }

    Rectangle {
        id: swapButton
        x: 10
        y: 10
        width: 95
        height: 30
        color: "yellow"
        border.width: 2
        Text {
            id: swapButtonText
            text: "Swap colors"
            anchors.fill: parent
            horizontalAlignment: Qt.AlignHCenter
            verticalAlignment: Qt.AlignVCenter
        }
        MouseArea {
            anchors.fill: parent
            ObjectRegistry.key: "MyButton"
        }
    }

    // Registration outside registered item, so registration won't get deleted with item.
    // This is to verify the target on the registration gets set to null if it is deleted,
    // leading to deregistration of the old target without need to connect destroyed signal.
    ObjectRegistry {
        target: deleteObjButton
        key: "CppDeleteObjButton"
    }

    // Duplicate target/key combination. The other one will get deleted if deleteObjButton is
    // pressed. This registration will also deregister if the other one does, as there is just
    // one actual registration for each target/key combination in the backend.
    ObjectRegistry {
        target: swapButtonText
        key: "SwapButtonText"
    }

    Rectangle {
        id: deleteObjButton

        signal clicked()

        x: swapButton.width + 20
        y: 10
        width: 95
        height: 30
        color: "yellow"
        border.width: 2
        Text {
            id: deleteObjText
            text: "Delete button"
            anchors.fill: parent
            horizontalAlignment: Qt.AlignHCenter
            verticalAlignment: Qt.AlignVCenter

        }
        MouseArea {
            id: buttonMouseArea
            onClicked: deleteObjButton.clicked()
            anchors.fill: parent
        }

        // Duplicate entry pointing elsewhere.
        // This one will get deleted if deleteObjButton is pressed.
        ObjectRegistry {
            target: swapButtonText
            key: "SwapButtonText"
        }
    }

    Repeater {
        model: 4
        Rectangle {
            id: qmlRepeaterDelegate

            required property int index

            width: 30
            height: 30
            x: index * 35 + 10
            y: 160
            radius: 5
            border.width: 2
            border.color: "black"
            color: "green"

            Text {
                anchors.fill: parent
                text: index
                horizontalAlignment: Qt.AlignHCenter
                verticalAlignment: Qt.AlignVCenter
            }

            MouseArea {
                // This property allows reference side access the repeater index of this mouse area
                property int repeaterIndex: qmlRepeaterDelegate.index

                anchors.fill: parent
                ObjectRegistry.key: "RepeatedRect"
            }
        }
    }

    Text {
        x: 10
        y: 200
        text: "Not clicked"
        ObjectRegistry.key: "IndexText"
    }

    Repeater {
        model: 4
        Rectangle {
            id: ccpRepeaterDelegate

            required property int index

            width: 30
            height: 30
            x: index * 35 + 10
            y: 230
            radius: 5
            border.width: 2
            border.color: "black"
            color: "red"

            Text {
                anchors.fill: parent
                text: index
                horizontalAlignment: Qt.AlignHCenter
                verticalAlignment: Qt.AlignVCenter
            }

            // This repeater is handled on C++ side.
            MouseArea {
                // This property allows reference side access the repeater index of this mouse area
                property int repeaterIndex: ccpRepeaterDelegate.index

                anchors.fill: parent
                ObjectRegistry.key: "CppRepeatedRect"
            }
        }
    }

    Text {
        x: 10
        y: 270
        text: "Not clicked"
        ObjectRegistry.key: "CppIndexText"
    }

    Rectangle {
        color: "lightgray"
        x: 250
        y: 10
        width: 250
        height: listView.height + 100

        Column {
            spacing: 4
            Row {
                spacing: 4
                Rectangle {
                    width: 100
                    height: 30
                    color: "yellow"
                    border.width: 2
                    Text {
                        anchors.fill: parent
                        text: "Add"
                        horizontalAlignment: Qt.AlignHCenter
                        verticalAlignment: Qt.AlignVCenter
                    }
                    MouseArea {
                        anchors.fill: parent
                        ObjectRegistry.key: "AddButton"
                    }
                }
                Rectangle {
                    width: 100
                    height: 30
                    color: "yellow"
                    border.width: 2
                    Text {
                        anchors.fill: parent
                        text: "Remove"
                        horizontalAlignment: Qt.AlignHCenter
                        verticalAlignment: Qt.AlignVCenter
                    }
                    MouseArea {
                        anchors.fill: parent
                        ObjectRegistry.key: "RemoveButton"
                    }
                }
            }

            ListView {
                id: listView
                x: 5
                width: 240
                height: contentHeight + 10
                currentIndex: -1

                property int myValue: 42

                delegate: Text {
                    id: delegateItem
                    required property string name
                    required property string number
                    signal clicked()
                    text: name + ": " + number
                    ObjectRegistry.key: "ListDelegate"
                    MouseArea {
                        id: listDelegateMouseArea
                        onClicked: delegateItem.clicked()
                        anchors.fill: parent
                    }
                }

                // ListView is registered so that business logic can set the model
                ObjectRegistry.key: "ListView"
            }

            Text {
                id: listLabel
                x: 5
                text: "Delegate not clicked"
                ObjectRegistry.key: "List Label"
                font.bold: true
                font.italic: true
                font.family: "Arial"
            }
        }
    }

    // Test registering two objects with one key warning
    Item {
        ObjectRegistry.key: "DoubleRegister"
    }
    Item {
        ObjectRegistry.key: "DoubleRegister"
    }
}
