import QtQuick
import QtQml.DesignSupport

Item {
    id: root

    Item {
        id: testItem1

        ObjectRegistry {
            id: reg1
            target: testItem1
            key: "SingleBefore"
        }
    }

    Item {
        id: testItem2

        ObjectRegistry {
            id: reg2
            target: testItem2
            key: "MultiBefore"
        }
    }

    Item {
        id: testItem3

        ObjectRegistry {
            id: reg3
            target: testItem3
            key: "MultiBefore"
        }
    }

    function triggerKeyChange()
    {
        reg1.key = "SingleAfter"
        reg2.key = "MultiAfter"
        reg3.key = "MultiAfter"
    }

}
