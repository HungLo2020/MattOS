// Copyright (C) 2017 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR BSD-3-Clause

pragma ComponentBehavior: Bound
import QtQuick
import QtWayland.Compositor
import QtWayland.Compositor.XdgShell

import io.qt.examples.customextension 1.0

WaylandCompositor {
    id: comp

    property alias customExtension: custom
    property list<ShellSurfaceItem> itemList: []

    function itemForSurface(surface) {
        for (const elem of itemList) {
            if (elem.surface === surface)
                return elem
        }
    }

    CompositorScreen {
        id: output
        compositor: comp
        itemList: comp.itemList
        customExtension: comp.customExtension
        onSetDecorations: (shown) => comp.setDecorations(shown)
    }

    Component {
        id: chromeComponent
        ShellSurfaceItem {
            id: chrome

            required property Window window
            property bool isCustom
            property int fontSize: 12

            onSurfaceDestroyed: {
                const index = comp.itemList.indexOf(chrome)
                if (index > -1)
                    comp.itemList.splice(index, 1)
                chrome.destroy()
            }

            transform: [
                Rotation {
                    id: xRot
                    origin.x: chrome.width / 2; origin.y: chrome.height / 2
                    angle: 0
                    axis { x: 1; y: 0; z: 0 }
                },
                Rotation {
                    id: yRot
                    origin.x: chrome.width / 2; origin.y: chrome.height / 2
                    angle: 0
                    axis { x: 0; y: 1; z: 0 }
                }
            ]

            NumberAnimation {
                id: spinAnimation
                running: false
                loops: 2
                target: yRot
                property: "angle"
                from: 0; to: 360
                duration: 400
            }

            function doSpin(ms) {
                console.log("spin " + ms)
                // using the 'ms' argument is left as an exercise for the reader...
                spinAnimation.start()
            }

            NumberAnimation {
                id: bounceAnimation
                running: false
                target: chrome
                property: "y"
                from: 0
                to: window.height - chrome.height
                easing.type: Easing.OutBounce
                duration: 1000
            }

            function doBounce(ms) {
                console.log("bounce " + ms)
                // using the 'ms' argument is left as an exercise for the reader...
                bounceAnimation.start()
            }

//! [setFontSize]
            onFontSizeChanged: {
                custom.setFontSize(surface, fontSize)
            }
//! [setFontSize]
        }
    }

    Component {
        id: customObjectComponent
        Rectangle {
            id: customItem
            property var obj

            width: 100
            height: 100
            radius: width / 2
            x: Math.random() * (output.surfaceArea.width - 100)
            y: Math.random() * (output.surfaceArea.height - 100)
            color: obj.color

            Text {
                id: label
                anchors.centerIn: parent
                text: customItem.obj.text
            }

            MouseArea {
                anchors.fill: parent
                onClicked: customItem.obj.sendClicked()
            }

            Connections {
                target: customItem.obj
                function onResourceDestroyed() {
                    customItem.destroy()
                }
            }
        }
    }

    XdgShell {
        onToplevelCreated: (toplevel, xdgSurface) => {
            const item = chromeComponent.createObject(output.surfaceArea,
                                                      { "shellSurface": xdgSurface, "window": output.window } )
            item.x = Math.random() * output.surfaceArea.width / 2
            item.y = Math.random() * output.surfaceArea.height / 2
            comp.itemList.push(item)
        }
    }

//! [CustomExtension]
    CustomExtension {
        id: custom

        onSurfaceAdded: (surface) => {
            const item = comp.itemForSurface(surface)
            item.isCustom = true
        }

        onBounce: (surface, ms) => {
            const item = comp.itemForSurface(surface)
            item.doBounce(ms)
        }

        onSpin: (surface, ms) => {
            const item = comp.itemForSurface(surface)
            item.doSpin(ms)
        }

        onCustomObjectCreated: (obj) => {
            const item = customObjectComponent.createObject(output.surfaceArea, { "obj": obj } )
        }
    }

    function setDecorations(shown) {
        for (const elem of itemList) {
            if (elem.isCustom)
                custom.showDecorations(elem.surface.client, shown)
        }
    }
//! [CustomExtension]
}
