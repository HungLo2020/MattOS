#include <QApplication>
#include <QFontDatabase>
#include <QFontInfo>
#include <QImage>
#include <QPainter>
#include <QPalette>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QRawFont>
#include <QVariant>
#include <KColorSchemeManager>
#include <fstream>
#include <iostream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <utility>

static void require(bool condition, const char *message)
{
    if (!condition) throw std::runtime_error(message);
}

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    try {
        std::ifstream mappings("/proc/self/maps");
        std::ostringstream loaded;
        loaded << mappings.rdbuf();
        require(loaded.str().find("KDEPlasmaPlatformTheme6.so") != std::string::npos,
                "native KDE platform theme was not loaded");
        const auto fixed = QFontDatabase::systemFont(QFontDatabase::FixedFont);
        const QFontInfo fixedInfo(fixed);
        std::cout << "font=" << app.font().toString().toStdString()
                  << " fixed=" << fixed.toString().toStdString()
                  << " resolved=" << fixedInfo.family().toStdString() << '\n';
        require(QFontInfo(app.font()).family() == QStringLiteral("Fira Sans"), "wrong desktop font");
        require(app.font().pointSize() == 10, "wrong desktop font size");
        require(fixedInfo.family() == QStringLiteral("Fira Mono") && fixedInfo.fixedPitch(), "wrong fixed font");
        require(fixed.pointSize() == 11, "wrong fixed font size");
        require(app.font().weight() == QFont::Normal && fixed.weight() == QFont::Normal, "wrong font weight");

        // Dolphin constructs this manager; check its resulting palette too.
        KColorSchemeManager::instance();
        std::cout << "window=" << app.palette().color(QPalette::Window).name().toStdString()
                  << " base=" << app.palette().color(QPalette::Base).name().toStdString() << '\n';
        require(app.palette().color(QPalette::Window) == QColor("#2e3440"), "wrong Nordic window color");
        require(app.palette().color(QPalette::Base) == QColor("#3b4252"), "wrong Nordic view color");

        for (auto item : {std::pair<const char *, uint>{"Noto Sans SC", 0x4e2d},
                          {"Noto Sans TC", 0x9f8d}, {"Noto Sans HK", 0x9f8d},
                          {"Noto Sans JP", 0x3042}, {"Noto Sans KR", 0xac00}}) {
            const QFont font(QString::fromLatin1(item.first), 14);
            require(QFontInfo(font).family() == QString::fromLatin1(item.first), "CJK family is unavailable");
            require(QRawFont::fromFont(font).supportsCharacter(item.second), "CJK glyph is unavailable");
        }
        QImage image(160, 100, QImage::Format_ARGB32_Premultiplied);
        image.fill(Qt::transparent);
        {
            QPainter painter(&image);
            painter.setFont(QFont(QStringLiteral("Noto Color Emoji"), 40));
            painter.drawText(5, 70, QString::fromUtf8("\xf0\x9f\x98\x80"));
        }
        int colored = 0;
        for (int y = 0; y < image.height(); ++y)
            for (int x = 0; x < image.width(); ++x) {
                const auto pixel = image.pixelColor(x, y);
                if (pixel.alpha() > 100 && (pixel.red() != pixel.green() || pixel.green() != pixel.blue())) ++colored;
            }
        require(colored > 20, "emoji did not render in color");

        QQmlEngine engine;
        QQmlComponent component(&engine);
        component.setData("import QtQml\nimport org.kde.bluezqt as Bluez\nQtObject { property QtObject manager: Bluez.Manager }", QUrl());
        std::unique_ptr<QObject> object(component.create());
        if (!object) std::cerr << component.errorString().toStdString();
        require(object && object->property("manager").value<QObject *>(), "Bluetooth QML integration failed");
        std::cout << "Desktop palette, fonts, CJK glyphs, color emoji and Bluetooth QML passed\n";
        return 0;
    } catch (const std::exception &error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
