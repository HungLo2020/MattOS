// Copyright (C) 2017 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR BSD-3-Clause

pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Window
import QtWayland.Compositor

WaylandOutput {
    id: root
    property alias surfaceArea: background
    required property list<ShellSurfaceItem> itemList
    required property var customExtension
    signal setDecorations(bool shown)

    sizeFollowsWindow: true

    window: Window {
        id: screen
        width: 1600
        height: 900
        visible: true

        Rectangle {
            id: sidebar
            width: 250
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            color: "lightgray"
            Column {
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                spacing: 5

                Repeater {
                    model: root.itemList
                    Rectangle {
                        id: delegateItem
                        required property var modelData
                        height: 54
                        width: sidebar.width - 5
                        color: "white"
                        radius: 5
                        Text {
                            text: "window: " + delegateItem.modelData.shellSurface?.toplevel.title + "\n["
                                  + delegateItem.modelData.shellSurface?.toplevel.appId
                                  + (delegateItem.modelData.isCustom ? "]\nfont size: "
                                  + delegateItem.modelData.fontSize : "]\nNo extension")
                            color: delegateItem.modelData.isCustom ? "black" : "darkgray"
                        }
                        MouseArea {
                            enabled: delegateItem.modelData.isCustom
                            anchors.fill: parent
                            onWheel: (wheel) => {
                                if (wheel.angleDelta.y > 0)
                                    delegateItem.modelData.fontSize++
                                else if (wheel.angleDelta.y < 0 && delegateItem.modelData.fontSize > 3)
                                    delegateItem.modelData.fontSize--
                            }
                            onDoubleClicked: {
                                root.customExtension.close(delegateItem.modelData.surface)
                            }
                        }
                    }
                }
                Text {
                    visible: root.itemList.length > 0
                    width: sidebar.width - 5
                    text: "Mouse wheel to change font size. Double click to close"
                    wrapMode: Text.Wrap
                }
            }
        }

        WaylandMouseTracker {
            id: mouseTracker
            anchors.left: sidebar.right
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom

            windowSystemCursorEnabled: !clientCursor.visible
            Image {
                id: background
                anchors.fill: parent
                fillMode: Image.Tile
                source: "qrc:/images/background.png"
                smooth: false
            }
            WaylandCursorItem {
                id: clientCursor
                x: mouseTracker.mouseX
                y: mouseTracker.mouseY

                seat: root.compositor.defaultSeat
            }

            Rectangle {
                property bool decor: true
                anchors.top: parent.top
                anchors.right: parent.right
                width: 100
                height: 100
                color: decor ? "#DEC0DE" : "#FACADE"
                Text {
                    anchors.fill: parent
                    text: "Toggle window decorations"
                    wrapMode: Text.WordWrap
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        parent.decor = !parent.decor
                        root.setDecorations(parent.decor);
                    }
                }
            }
        }
    }
}
