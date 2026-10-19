import QtQuick

Item {
    Main {}
    Name1 {}
    Name2 {}
    Name3 {}
    Name4 {}

    enum Hello {
        World,
        Kitty
    }
    component MyComponent: Item {}

    property int enumWithBadTypeName: Main.World
}
