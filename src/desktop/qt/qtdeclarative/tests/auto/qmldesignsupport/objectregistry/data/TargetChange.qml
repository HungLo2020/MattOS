import QtQuick
import QtQml.DesignSupport

Item {
    id: root

    function changeTarget()
    {
        reg.target = qtobject
    }

    function changeTargetBack()
    {
        reg.target = item
    }

    Item {
        id: item
        property int testProp: 11
    }

    QtObject {
        id: qtobject
        property int testProp: 22
    }

    ObjectRegistry {
        id: reg
        target: item
        key: "ChangingTarget"
    }
}
