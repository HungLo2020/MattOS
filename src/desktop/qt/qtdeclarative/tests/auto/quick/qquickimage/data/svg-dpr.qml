import QtQuick

Item {
    Image {
        objectName: "svg-sync"
        asynchronous: false
        source: "heart.svg"
    }

    Image {
        objectName: "svg-async"
        asynchronous: true
        source: "heart.svgz"
    }
}
