import QtQuick

Item {
    Name1 {}
    Name2 {}
    Name3 {}
    Name4 {}

    property int myEnum1: Name1.Kitty
    property int myEnum2: Name2.Kitty
    property int myEnum3: Name3.Kitty
    property int myEnum4: Name4.Kitty

    function g(a: Name1.MyComponent): Name1.MyComponent {
    } // good: new name

    component Main: Item {}
    Main {} // should not complain
}
