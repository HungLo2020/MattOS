import QtQml

QtObject {
    property int b: A.a // qmllint disable renamed-type
    property int e: X.a
}
