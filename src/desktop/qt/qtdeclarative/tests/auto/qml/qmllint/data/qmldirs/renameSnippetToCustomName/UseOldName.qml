import QtQuick

Item {
    property UseOldName myProperty
    function f(x: UseOldName): UseOldName {
        return x;
    }
}
