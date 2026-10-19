import QtQuick
import QtQml.DesignSupport

Item {
    id: root

    Repeater {
        id: repeater
        model: 6

        property int testValue: 9

        Rectangle {
            required property int index
            property int testValue: repeater.testValue
            x: 10 * index
            y: 20

            ObjectRegistry.key: "RepeatedRect"

            border.width: index
            border.color: "#123456"
        }
    }

    function resetToFour()
    {
        repeater.model = 4
    }
}
