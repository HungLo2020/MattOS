import QtQuick 2.0

QtObject {
    property string test1: Qt.escapeHtml()
    property string test2: Qt.escapeHtml("<>&\"")
    property string test3: Qt.escapeHtml("<script>alert('XSS')</script>")
    property string test4: Qt.escapeHtml("Normal text")
    property string test5: Qt.escapeHtml("")
}
