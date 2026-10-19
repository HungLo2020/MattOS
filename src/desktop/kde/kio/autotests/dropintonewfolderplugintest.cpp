/*
    This file is part of the KDE project
    SPDX-FileCopyrightText: 2026 Méven Car <meven@kde.org>

    SPDX-License-Identifier: GPL-2.0-or-later
*/

#include <QAction>
#include <QDialog>
#include <QDialogButtonBox>
#include <QPushButton>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>
#include <QWidget>

#include <KFileItem>
#include <KFileItemListProperties>
#include <KPluginFactory>
#include <KPluginMetaData>

#include <KIO/DndPopupMenuPlugin>

#ifndef DROPINTONEWFOLDER_PLUGIN_PATH
#error "DROPINTONEWFOLDER_PLUGIN_PATH must be defined by the build system"
#endif

class DropIntoNewFolderPluginTest : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void testSkipsNonLocalDestination()
    {
        auto plugin = loadPlugin();
        QVERIFY(plugin);
        const auto actions = plugin->setup(propsFor({QUrl(QStringLiteral("file:///tmp/some/file"))}), QUrl(QStringLiteral("sftp://host/dir")));
        QVERIFY2(actions.isEmpty(), "the plugin must not offer the action for a non-local destination");
        delete plugin;
    }

    void testOffersActionForLocalDir()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString file = dir.filePath(QStringLiteral("a.txt"));
        QVERIFY(createFile(file));

        auto plugin = loadPlugin();
        QVERIFY(plugin);
        const auto actions = plugin->setup(propsFor({QUrl::fromLocalFile(file)}), QUrl::fromLocalFile(dir.path()));
        QCOMPARE(actions.size(), 1);
        QVERIFY(actions.first()->isEnabled());
        delete plugin;
    }

    // Regression test for the lifetime bug: KIO::DropJob owns the plugin and destroys it the moment
    // the action is triggered. The New Folder dialog is created asynchronously afterwards, so it must
    // not be a child of the plugin - otherwise it is destroyed before it can appear and the feature
    // silently does nothing. Deleting the plugin here reproduces what DropJob does.
    //
    // The dialog is cancelled rather than accepted on purpose: accepting it would move the files and
    // then start a fire-and-forget OpenFileManagerWindowJob, which depends on D-Bus and a running file
    // manager and is not suitable for (nor the subject of) a unit test.
    void testDialogSurvivesPluginDeletion()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString src = dir.filePath(QStringLiteral("a.txt"));
        QVERIFY(createFile(src));
        const QString destDir = dir.filePath(QStringLiteral("dest"));
        QVERIFY(QDir(dir.path()).mkdir(QStringLiteral("dest")));

        // Give the plugin an active window to parent its dialog to (it uses QApplication::activeWindow()).
        QWidget window;
        window.show();
        window.activateWindow();

        auto plugin = loadPlugin();
        QVERIFY(plugin);
        const auto actions = plugin->setup(propsFor({QUrl::fromLocalFile(src)}), QUrl::fromLocalFile(destDir));
        QCOMPARE(actions.size(), 1);

        actions.first()->trigger();
        // Simulate DropJob deleting the plugin right after the action fired.
        delete plugin;

        // The New Folder dialog must still appear - it would not if it had been parented to (and thus
        // destroyed with) the plugin. This is the assertion that fails without the fix.
        QDialog *dialog = nullptr;
        QTRY_VERIFY(dialog = findDialog());

        auto *buttons = dialog->findChild<QDialogButtonBox *>();
        QVERIFY(buttons);
        buttons->button(QDialogButtonBox::Cancel)->click();
        QTRY_VERIFY(!findDialog());
    }

private:
    static KIO::DndPopupMenuPlugin *loadPlugin()
    {
        KPluginMetaData md(QString::fromUtf8(DROPINTONEWFOLDER_PLUGIN_PATH));
        if (!md.isValid()) {
            return nullptr;
        }
        return KPluginFactory::instantiatePlugin<KIO::DndPopupMenuPlugin>(md, nullptr, {}).plugin;
    }

    static KFileItemListProperties propsFor(const QList<QUrl> &urls)
    {
        KFileItemList items;
        items.reserve(urls.size());
        for (const QUrl &url : urls) {
            items << KFileItem(url);
        }
        return KFileItemListProperties(items);
    }

    static bool createFile(const QString &path)
    {
        QFile f(path);
        return f.open(QIODevice::WriteOnly) && f.write("x") == 1;
    }

    static QDialog *findDialog()
    {
        const auto widgets = QApplication::topLevelWidgets();
        for (QWidget *w : widgets) {
            if (auto *d = qobject_cast<QDialog *>(w)) {
                return d;
            }
            if (auto *d = w->findChild<QDialog *>()) {
                return d;
            }
        }
        return nullptr;
    }
};

QTEST_MAIN(DropIntoNewFolderPluginTest)

#include "dropintonewfolderplugintest.moc"
