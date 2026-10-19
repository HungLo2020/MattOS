import QtQuick
import QtQuick.Controls

Row {
    width: 320
    height: 240
    spacing: 2

    Rectangle {
        height: parent.height
        width: parent.width / 2 - 1
        color: btn1.checked ? "yellow" : "lightblue"

        Button {
            id: btn1
            text: "In"
            checkable: true
            anchors.centerIn: parent

            Menu {
                id: menu1
                MenuItem {
                    text: "Option 1"
                }
                MenuItem {
                    text: "Option 2"
                }
            }

            TapHandler {
                objectName: "in button"
                acceptedButtons: Qt.RightButton
                onTapped: (eventPoint, _) => menu1.popup(eventPoint.pressPosition)
            }
        }
    }

    Rectangle {
        height: parent.height
        width: parent.width / 2 - 1
        color: btn2.checked ? "yellow" : "lightblue"

        Button {
            id: btn2
            text: "Out"
            checkable: true
            anchors.centerIn: parent

            Menu {
                id: menu2
                MenuItem {
                    text: "Option 1"
                }
                MenuItem {
                    text: "Option 2"
                }
            }
        }

        TapHandler {
            objectName: "in button parent"
            acceptedButtons: Qt.RightButton
            onTapped:
                (eventPoint, _) => {
                    const buttonPoint = btn2.mapFromItem(parent, eventPoint.pressPosition)
                    menu2.popup(buttonPoint)
                }
        }
    }
}
