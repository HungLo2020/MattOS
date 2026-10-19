import QtQuick

Item {
    Main {} // bad: old name
    Name1 {} // good: new name
    Name2 {} // good: new name
    Name3 {} // good: new name
    Name4 {} // good: new name

    property int myEnum: Main.Kitty // bad: old name
    property int myEnum1: Name1.Kitty // good: new name
    property int myEnum2: Name2.Kitty // good: new name
    property int myEnum3: Name3.Kitty // good: new name
    property int myEnum4: Name4.Kitty // good: new name

    function f(a: Main.MyComponent): Main.MyComponent {
    } // bad: old name
    function g(a: Name1.MyComponent): Name1.MyComponent {
    } // good: new name
}
