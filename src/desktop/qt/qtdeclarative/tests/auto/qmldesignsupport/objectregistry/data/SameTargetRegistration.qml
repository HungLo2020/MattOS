import QtQuick
import QtQml.DesignSupport

Item {
    id: root

    property alias target1: reg1.target
    property alias target2: reg2.target
    property alias target3: reg3.target
    property alias target4: reg4.target
    property alias target5: reg5.target
    property alias target6: reg6.target

    Item {
        id: testItem1
        property int testProp1: 1

        ObjectRegistry.key: "SameTargetMulti"
    }

    ObjectRegistry {
        id: reg1
        target: testItem1
        key: "SameTargetMulti"
    }

    Item {
        id: testItem2
        property int testProp2: 2

        ObjectRegistry.key: "SameTargetMulti"
    }

    ObjectRegistry {
        id: reg2
        target: testItem2
        key: "SameTargetMulti"
    }

    Item {
        id: testItem3
        property int testProp3: 3
        ObjectRegistry.key: "SameTargetSingle"
    }

    ObjectRegistry {
        id: reg3
        target: testItem3
        key: "SameTargetSingle"
    }

    Item {
        id: testItem4
        property int testProp4: 4

        ObjectRegistry.key: "SameTargetMultiDifferentkey1"
    }

    ObjectRegistry {
        id: reg4
        target: testItem4
        key: "SameTargetMultiDifferentkey2"
    }

    Item {
        id: testItem5
        property int testProp5: 5

        ObjectRegistry.key: "SameTargetMultiDifferentkey1"
    }

    ObjectRegistry {
        id: reg5
        target: testItem5
        key: "SameTargetMultiDifferentkey2"
    }

    Item {
        id: testItem6
        property int testProp6: 6
        ObjectRegistry.key: "SameTargetDifferentKey1"
    }

    ObjectRegistry {
        id: reg6
        target: testItem6
        key: "SameTargetDifferentKey2"
    }
}
