import QtQuick
import QtQml.DesignSupport

Item {
    id: root

    Item {
        id: testItem1
        property int testProp1: 1
        signal testSignal1()

        ObjectRegistry {
            target: testItem1
            key: "MultiRegistration"
        }
    }

    Item {
        id: testItem2
        property int testProp2: 2
        signal testSignal2()

        ObjectRegistry {
            target: testItem2
            key: "MultiRegistration"
        }
    }

    function triggerSignal1()
    {
        testItem1.testSignal1();
    }

    function triggerSignal2()
    {
        testItem2.testSignal2();
    }
}
