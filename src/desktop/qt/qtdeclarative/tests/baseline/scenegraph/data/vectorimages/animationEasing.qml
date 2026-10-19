import QtQuick
import QtQuick.VectorImage

Rectangle { 
    id: topLevelItem
    width: 800
    height: 350

    ListModel {
        id: files
        ListElement { src: "../shared/svg/animationEasing.svg" }
        ListElement { src: "../shared/svg/animationEasingPerKeyframe.svg" }
    }

    Grid {
        anchors.fill: parent
        Repeater {
            model: files
            VectorImage {
                source: src
                preferredRendererType: VectorImage.CurveRenderer
            }
        }
    }
}
