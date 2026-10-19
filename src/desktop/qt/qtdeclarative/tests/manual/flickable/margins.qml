// Copyright (C) 2026 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR GPL-3.0-only

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

ApplicationWindow {
    id: root

    width: 700
    height: 500
    visible: true

    title: "Flickable Margins Test"

    property Flickable flickable: stackView.currentItem

    Component {
        id: flickableComponent
        Flickable {
            id: flickable
            contentHeight: verticalCheckBox.checked ? height * 1.618 : -1
            contentWidth: horizontalCheckBox.checked ? width * 1.618 : -1
            leftMargin: leftMarginHandle.value
            topMargin: topMarginHandle.value
            rightMargin: rightMarginHandle.value
            bottomMargin: bottomMarginHandle.value
            flickableDirection: Flickable.AutoFlickIfNeeded
            property Item footerContent: RowLayout {
                Label {
                    text: "Flick Orientation:"
                }
                CheckBox {
                    id: verticalCheckBox
                    checked: true
                    text: "Vertical"
                }
                CheckBox {
                    id: horizontalCheckBox
                    text: "Horizontal"
                }
                Spacer {}
            }
        }
    }
    Component {
        id: listViewComponent
        ListView {
            id: listView
            model: 50
            cacheBuffer: height * 3
            displayMarginBeginning: height
            displayMarginEnd: height
            leftMargin: leftMarginHandle.value
            topMargin: topMarginHandle.value
            rightMargin: rightMarginHandle.value
            bottomMargin: bottomMarginHandle.value
            orientation: verticalRadioButton.checked ? ListView.Vertical : ListView.Horizontal
            delegate: Rectangle {
                opacity: 0.1
                width: ListView.view.orientation === ListView.Vertical ? ListView.view.contentItem.width : 50
                height: ListView.view.orientation === ListView.Horizontal ? ListView.view.contentItem.height : 50
                color: index % 2 == 0 ? root.palette.base : root.palette.alternateBase
                border.color: "orange"
            }
            property Item footerContent: RowLayout {
                Label {
                    text: "ListView Orientation:"
                }
                RadioButton {
                    id: verticalRadioButton
                    checked: true
                    text: "Vertical"
                }
                RadioButton {
                    id: horizontalRadioButton
                    text: "Horizontal"
                }
                Spacer {}
            }
        }
    }

    StackView {
        id: stackView
        initialItem: flickableComponent
        anchors.fill: parent
    }

    Rectangle {
        parent: root.flickable
        anchors.fill: parent?.contentItem
        z: 1
        color: "transparent"
        opacity: 0.8
        border.color: "red"
        border.width: 6
        Label {
            padding: 20
            text: "Content Item"
            color: "red"
        }
    }

    Rectangle {
        // To visualize the Flickable's area in the minimap
        parent: root.flickable
        anchors.fill: parent
        color: "transparent"
        opacity: 0.8
        border.color: root.palette.shadow
        border.width: 6
    }

    Label {
        anchors.centerIn: parent
        horizontalAlignment: Text.AlignHCenter
        text: `Available Flickable Area\n(${(root.flickable.width - root.flickable.leftMargin - root.flickable.rightMargin).toFixed(0)} x ${(root.flickable.height - root.flickable.topMargin - root.flickable.bottomMargin).toFixed(0)})`
        color: root.palette.windowText
    }

    Item {
        parent: root.flickable
        anchors.fill: parent

        MarginHandle {
            id: leftMarginHandle
            edge: Qt.LeftEdge
            text: "Left"
            color: "darkblue"
        }
        MarginHandle {
            id: topMarginHandle
            edge: Qt.TopEdge
            text: "Top"
            color: "darkgreen"
        }
        MarginHandle {
            id: rightMarginHandle
            edge: Qt.RightEdge
            text: "Right"
            color: "darkred"
        }
        MarginHandle {
            id: bottomMarginHandle
            edge: Qt.BottomEdge
            text: "Bottom"
            color: "darkorange"
        }
    }

    // Minimap overview
    ShaderEffectSource {
        id: minimap
        sourceItem: root.flickable
        hideSource: false
        live: true
        scale: 1/2
        width: root.flickable?.width ?? 0
        height: root.flickable?.height ?? 0
        sourceRect: Qt.rect(-width, -height, width * 3, height * 3)
        x: parent.width - width * 2/3
        y: parent.height - height * 2/3
        Label {
            text: "Minimap"
            scale: 1 / parent.scale
            anchors.centerIn: parent
        }
    }

    header: ToolBar {
        padding: 10
        contentItem: RowLayout {
            spacing: 10

            ToolButton {
                text: "Reload Flickable"
                onClicked: stackView.replace(flickableComponent)
            }
            ToolButton {
                text: "Reload ListView"
                onClicked: stackView.replace(listViewComponent)
            }

            ToolSeparator {}

            ToolButton {
                text: "Clear Margins"
                onClicked: {
                    leftMarginHandle.value = 0
                    topMarginHandle.value = 0
                    rightMarginHandle.value = 0
                    bottomMarginHandle.value = 0
                }
            }

            Spacer {}

            Label {
                Layout.fillHeight: true
                verticalAlignment: Text.AlignVCenter
                text:
                    "contentWidth: " + root.flickable.contentWidth.toFixed(0) +
                    "\ncontentItem.width: " + root.flickable.contentItem.width.toFixed(0)
            }
            Label {
                Layout.fillHeight: true
                verticalAlignment: Text.AlignVCenter
                text:
                    "contentHeight: " + root.flickable.contentHeight.toFixed(0) +
                    "\ncontentItem.height: " + root.flickable.contentItem.height.toFixed(0)
            }
        }
    }

    footer: ToolBar {
        padding: 10
        contentItem: root.flickable?.footerContent ?? null
    }

    // A handle to visualize and adjust a Flickable's margin.
    component MarginHandle: Rectangle {
        id: marginHandle

        required property int edge
        required color
        required property string text

        property real value: 20

        width: 12
        height: 12

        anchors {
            left: edge !== Qt.RightEdge ? parent.left : undefined
            right: edge !== Qt.LeftEdge ? parent.right : undefined
            top: edge !== Qt.BottomEdge ? parent.top : undefined
            bottom: edge !== Qt.TopEdge ? parent.bottom : undefined

            leftMargin: edge === Qt.LeftEdge ? value : 0
            rightMargin: edge === Qt.RightEdge ? value : 0
            topMargin: edge === Qt.TopEdge ? value : 0
            bottomMargin: edge === Qt.BottomEdge ? value : 0
        }

        // an increasing x or y for left or top edge means increasing the margin, but for right or bottom edge it means decreasing the margin.
        readonly property int factor: (edge === Qt.LeftEdge || edge === Qt.TopEdge) ? 1 : -1
        function updateMarginValue(delta) {
            value = Math.max(0, value + factor * delta);
        }

        DragHandler {
            id: dragHandler
            target: null
            cursorShape: xAxis.enabled ? Qt.SizeHorCursor : Qt.SizeVerCursor
            xAxis {
                enabled: marginHandle.edge & (Qt.LeftEdge | Qt.RightEdge)
                onActiveValueChanged: delta => marginHandle.updateMarginValue(delta)
            }
            yAxis {
                enabled: !xAxis.enabled
                onActiveValueChanged: delta => marginHandle.updateMarginValue(delta)
            }
        }
        HoverHandler { cursorShape: dragHandler.cursorShape }

        Label {
            anchors.centerIn: parent
            font.pixelSize: 16
            horizontalAlignment: Text.AlignHCenter
            text: marginHandle.text + "\n" + marginHandle.value
            style: Text.Outline
            styleColor: root.palette.window
        }
    }

    component Spacer: Item {
        Layout.fillWidth: true
        Layout.fillHeight: true
    }
}
