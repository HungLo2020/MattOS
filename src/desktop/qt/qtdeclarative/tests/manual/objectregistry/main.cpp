// Copyright (C) 2026 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR GPL-3.0-only

#include "cppbusinesslogic.h"

#include <QtGui/qguiapplication.h>
#include <QtQml/qqmlapplicationengine.h>
#include <QtQml/qqmlcontext.h>
#include <QtQuick/qquickwindow.h>

int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);

    QQmlApplicationEngine engine;
    QObject::connect(
        &engine,
        &QQmlApplicationEngine::objectCreationFailed,
        &app,
        []() { QCoreApplication::exit(-1); },
        Qt::QueuedConnection);
    engine.loadFromModule("ObjectRegistryTest", "Main");

    CppBusinessLogic logic(&engine);

//#define TWO_ENGINE_TEST
#ifdef TWO_ENGINE_TEST
    QQmlApplicationEngine engine2;
    QObject::connect(
        &engine2,
        &QQmlApplicationEngine::objectCreationFailed,
        &app,
        []() { QCoreApplication::exit(-1); },
        Qt::QueuedConnection);
    engine2.loadFromModule("ObjectRegistryTest", "Main");

    auto rootObj = engine.rootObjects().constFirst();
    QQuickWindow *w = qobject_cast<QQuickWindow*>(rootObj);
    if (!w)
        w = rootObj->findChild<QQuickWindow*>();
    if (w)
        w->setPosition(QPoint{50, 50});

    auto rootObj2 = engine2.rootObjects().constFirst();
    QQuickWindow *w2 = qobject_cast<QQuickWindow*>(rootObj2);
    if (!w2)
        w2 = rootObj2->findChild<QQuickWindow*>();
    if (w2)
        w2->setPosition(QPoint{w->position().x() + w->width() + 5, w->position().y()});

    CppBusinessLogic logic2(&engine2);
#endif

    return app.exec();
}
