// Copyright (C) 2026 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR GPL-3.0-only

// This file simulates hand-written C++ business logic part of the application.
// The UI elements are accessed via ObjectRegistry references.

#include "cppbusinesslogic.h"

CppBusinessLogic::CppBusinessLogic(QQmlEngine *engine, QObject *parent)
    : QObject{parent}
{
    // Note: If the registered object is dynamically created by e.g. asynchronous loader
    // or in response to user activity, it may not yet be resolved at the time the reference is
    // created. In those cases you should connect to QObjectRegistryRef's objectChanged signal
    // or QMultiObjectRegistryRef's objectAdded signal and handle object connections there.

    m_deleteObjRef = new QObjectRegistryRef(engine, "CppDeleteObjButton", this);
    connect(m_deleteObjRef->object(), SIGNAL(clicked()),
            this, SLOT(handleDeleteObjButtonClicked()));

    m_repeaterObjRef = new QMultiObjectRegistryRef(engine, "CppRepeatedRect", this);
    const auto repeaterObjects = m_repeaterObjRef->objectsList();
    for (auto obj : repeaterObjects) {
        connect(obj, SIGNAL(clicked(QQuickMouseEvent*)),
                this, SLOT(handleRepeaterObjectClicked()));
    }

    // Simple case, only properties are used from this reference, so no connections needed
    m_indexTextRef = new QObjectRegistryRef(engine, "CppIndexText", this);
}

void CppBusinessLogic::handleDeleteObjButtonClicked()
{
    m_deleteObjRef->object()->deleteLater();
}

void CppBusinessLogic::handleRepeaterObjectClicked()
{
    m_indexTextRef->object()->setProperty(
        "text",
        QString("Cpp clicked index: %1").arg(sender()->property("repeaterIndex").toInt()));
}
