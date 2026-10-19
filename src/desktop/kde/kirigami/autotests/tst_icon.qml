/*
 *  SPDX-FileCopyrightText: 2020 Arjen Hiemstra <ahiemstra@heimr.nl>
 *
 *  SPDX-License-Identifier: LGPL-2.0-or-later
 */

import QtQuick
import QtTest
import org.kde.kirigami as Kirigami

TestCase {
    id: testCase
    name: "IconTests"

    width: 400
    height: 400
    visible: true

    when: windowShown

    Component { id: emptyIcon; Kirigami.Icon { } }
    Component { id: sourceOnlyIcon; Kirigami.Icon { source: "document-new" } }
    Component { id: sizeOnlyIcon; Kirigami.Icon { width: 50; height: 50 } }
    Component { id: sizeSourceIcon; Kirigami.Icon { width: 50; height: 50; source: "document-new" } }
    Component { id: minimalSizeIcon; Kirigami.Icon { width: 1; height: 1; source: "document-new" } }
    Component {
        id: absolutePathIcon;
        Kirigami.Icon {
            id: icon
            width: 50;
            height: 50;
            source: Qt.resolvedUrl("stop-icon.svg")
        }
    }
    Component {
        id: portraitIcon
        Kirigami.Icon {
            // A non-square box, wider than tall.
            width: 100
            height: 50
            // A 40x80 image: taller than it is wide.
            source: Qt.resolvedUrl("portrait-icon.png")
        }
    }
    Kirigami.ImageColors {
        id: imageColors
    }

    function test_create_data() {
        return [
            { tag: "Empty", component: emptyIcon },
            { tag: "Source Only", component: sourceOnlyIcon },
            { tag: "Size Only", component: sizeOnlyIcon },
            { tag: "Size & Source", component: sizeSourceIcon },
            { tag: "Minimal Size", component: minimalSizeIcon }
        ]
    }

    // Test creation of Icon objects.
    // It should not crash when certain properties are not specified and also
    // should still work when they are.
    function test_create(data) {
        var icon = createTemporaryObject(data.component, testCase)
        verify(icon)
        verify(waitForRendering(icon))
    }

    function test_portrait_aspect_ratio_data() {
        return [
            { tag: "roundToIconSize", roundToIconSize: true },
            { tag: "no roundToIconSize", roundToIconSize: false },
        ]
    }

    // A portrait image must keep its aspect ratio and not be stretched to a square.
    function test_portrait_aspect_ratio(data) {
        let icon = createTemporaryObject(portraitIcon, testCase, { roundToIconSize: data.roundToIconSize })
        verify(icon)
        verify(waitForRendering(icon))
        // The source loads asynchronously; before it is ready the icon uses a box-sized
        // placeholder, so wait for the real image to be loaded before checking its aspect.
        tryVerify(() => icon.status === Kirigami.Icon.Ready)
        verify(icon.paintedWidth > 0 && icon.paintedHeight > 0)
        verify(icon.paintedWidth < icon.paintedHeight,
               "portrait icon must not be stretched wide: paintedWidth=" + icon.paintedWidth + " paintedHeight=" + icon.paintedHeight)
    }

    function test_absolutepath_recoloring() {
        skip("This test depends too much on environment and other factors to work reliably")

        var icon = createTemporaryObject(absolutePathIcon, testCase)
        verify(icon)
        verify(waitForRendering(icon))

        var image = icon.grabToImage(function(result) {
            // Access pixel data of the captured image
            imageColors.source = result.image
            imageColors.update()
        })
        print(Qt.resolvedUrl("stop-icon.svg"))
        tryCompare(imageColors, "dominant", "#2980b9")
    }
}
