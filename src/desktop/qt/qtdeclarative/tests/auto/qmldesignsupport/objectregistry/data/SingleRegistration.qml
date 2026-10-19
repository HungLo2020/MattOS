import QtQuick
import QtQml.DesignSupport

Item {
    id: root

    Item {
        id: item1
        property int testProp1: 11

        signal testSignal()

        ObjectRegistry {
            target: item1
            key: "SingleRegistration"
        }
    }

    function triggerSignal1()
    {
        item1.testSignal()
    }

    function triggerSignal2()
    {
        item2.testSignal()
    }

    function triggerSignal3()
    {
        qtobject.testSignal()
    }

    function triggerSignal4()
    {
        item3.testSignal()
    }

    Item {
        id: item2
        signal testSignal()
        property int testProp2: 22
        ObjectRegistry.key: "ItemAttachReg"
    }

    QtObject {
        id: qtobject
        signal testSignal()
        property int testProp3: 33
        ObjectRegistry.key: "ObjectAttachReg"
    }

    Item {
        id: item3
        signal testSignal()
        property int testProp4: 44
    }

    ObjectRegistry {
        target: item3
        key: "RegNotInsideObject"
    }
}
