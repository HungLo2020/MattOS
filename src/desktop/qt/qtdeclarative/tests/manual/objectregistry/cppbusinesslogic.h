// Copyright (C) 2026 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR GPL-3.0-only

#ifndef CPPBUSINESSLOGIC_H
#define CPPBUSINESSLOGIC_H

#include <QtCore/qobject.h>
#include <QtQml/qqmlengine.h>

#include <QtQmlDesignSupport/qmultiobjectregistryref.h>
#include <QtQmlDesignSupport/qobjectregistryref.h>

class CppBusinessLogic : public QObject
{
    Q_OBJECT
public:
    explicit CppBusinessLogic(QQmlEngine *engine = nullptr, QObject *parent = nullptr);

public slots:
    void handleDeleteObjButtonClicked();
    void handleRepeaterObjectClicked();

private:
    QObjectRegistryRef *m_deleteObjRef = nullptr;
    QMultiObjectRegistryRef *m_repeaterObjRef = nullptr;
    QObjectRegistryRef *m_indexTextRef = nullptr;
};

#endif // CPPBUSINESSLOGIC_H
