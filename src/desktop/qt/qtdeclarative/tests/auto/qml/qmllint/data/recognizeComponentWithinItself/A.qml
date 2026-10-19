import QtQml

QtObject {
    id: self
    property int a: 5
    function doSomething() : int {
        return self.a // self is of type A
    }
}