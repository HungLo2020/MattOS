import QtQuick
import QtQml.DesignSupport

Item {
    id: root

    property var singleObj: singleRef.object
    property var multiObj0: multiRef.objects.length === 2 ? multiRef.objects[0] : null
    property var multiObj1: multiRef.objects.length === 2 ? multiRef.objects[1] : null

    SingleRegistration{}
    MultiRegistration{}

    ObjectRegistryRef {
        id: singleRef
        key: "SingleRegistration"
    }

    MultiObjectRegistryRef {
        id: multiRef
        key: "MultiRegistration"
    }
}
