/***************************************************************************
                          ecma_parser.cpp  -  description
                             -------------------
    begin                : Feb 10 2012
    author               : 2012 Jesse Crossen
    email                : jesse.crossen@gmail.com
 ***************************************************************************/
/***************************************************************************
 *                                                                         *
 *   SPDX-License-Identifier: GPL-2.0-or-later
 *                                                                         *
 ***************************************************************************/
#include "plugin_katesymbolviewer.h"

void KatePluginSymbolViewerView::parseEcmaSymbols(void)
{
    // make sure there is an active view to attach to
    if (!m_mainWindow->activeView()) {
        return;
    }

    // a parsed class/function identifier
    QString identifier;
    // temporary characters
    QChar current, next, string_start = u'\0';
    // whether we are in a multiline comment
    bool in_comment = false;
    // indices into the string
    int c, function_start = 0;
    // a list of inserted nodes with the index being the brace depth at insertion
    QList<QTreeWidgetItem *> nodes;

    QTreeWidgetItem *node = nullptr;

    if (m_treeOn->isChecked()) {
        m_symbols->setRootIsDecorated(1);
    } else {
        m_symbols->setRootIsDecorated(0);
    }

    // read the document line by line
    KTextEditor::Document *kv = m_mainWindow->activeView()->document();
    for (int i = 0; i < kv->lines(); i++) {
        // get a line to process, trimming off whitespace
        QString cl = kv->line(i).trimmed();
        QString stripped; // the current line stripped of all comments and strings
        bool in_string = false;
        for (c = 0; c < cl.length(); c++) {
            // get the current character and the next
            current = cl.at(c);
            if ((c + 1) < cl.length()) {
                next = cl.at(c + 1);
            } else {
                next = u'\0';
            }
            // skip the rest of the line if we find a line comment
            if ((!in_comment) && (current == u'/') && (next == u'/')) {
                break;
            }
            // open/close multiline comments
            if ((!in_string) && (current == u'/') && (next == u'*')) {
                in_comment = true;
                c++;
                continue;
            } else if ((in_comment) && (current == u'*') && (next == u'/')) {
                in_comment = false;
                c++;
                continue;
            }
            // open strings
            if ((!in_comment) && (!in_string)) {
                if ((current == u'\'') || (current == u'"')) {
                    string_start = current;
                    in_string = true;
                    continue;
                }
            }
            // close strings
            if (in_string) {
                // skip escaped backslashes
                if ((current == u'\\') && (next == u'\\')) {
                    c++;
                    continue;
                }
                // skip escaped string closures
                if ((current == u'\\') && (next == string_start)) {
                    c++;
                    continue;
                } else if (current == string_start) {
                    in_string = false;
                    continue;
                }
            }
            // add anything outside strings and comments to the stripped line
            if ((!in_comment) && (!in_string)) {
                stripped += current;
            }
        }

        // scan the stripped line
        for (c = 0; c < stripped.length(); c++) {
            current = stripped.at(c);

            // look for class definitions (for ActionScript)
            if ((current == u'c') && (stripped.indexOf(QLatin1String("class "), c) == c)) {
                identifier.clear();
                c += 6;
                for (/*c = c*/; c < stripped.length(); c++) {
                    current = stripped.at(c);
                    // look for the beginning of the class itself
                    if ((current == u'(') || (current == u'{')) {
                        c--;
                        break;
                    } else {
                        identifier += current;
                    }
                }
                // trim whitespace
                identifier = identifier.trimmed();
                // get the node to add the class entry to
                if ((m_treeOn->isChecked()) && (!nodes.isEmpty())) {
                    node = new QTreeWidgetItem(nodes.last());
                    if (m_expandOn->isChecked()) {
                        m_symbols->expandItem(node);
                    }
                } else {
                    node = new QTreeWidgetItem(m_symbols);
                }
                // add an entry for the class
                node->setText(0, identifier);
                node->setIcon(0, m_icon_class);
                node->setText(1, QString::number(i, 10));
                if (m_expandOn->isChecked()) {
                    m_symbols->expandItem(node);
                }
            } // (look for classes)

            // look for function definitions
            if ((current == u'f') && (stripped.indexOf(QLatin1String("function "), c) == c)) {
                function_start = c;
                c += 8;
                // look for the beginning of the parameters
                identifier.clear();
                for (/*c = c*/; c < stripped.length(); c++) {
                    current = stripped.at(c);
                    // look for the beginning of the function definition
                    if ((current == u'(') || (current == u'{')) {
                        c--;
                        break;
                    } else {
                        identifier += current;
                    }
                }
                // trim off whitespace
                identifier = identifier.trimmed();
                // if we have an anonymous function, back up to see if it's assigned to anything
                if (!(identifier.length() > 0)) {
                    QChar ch = u'\0';
                    for (int end = function_start - 1; end >= 0; end--) {
                        ch = stripped.at(end);
                        // skip whitespace
                        if ((ch == u' ') || (ch == u'\t')) {
                            continue;
                        }
                        // if we hit an assignment or object property operator,
                        //  get the preceding identifier
                        if ((ch == u'=') || (ch == u':')) {
                            end--;
                            while (end >= 0) {
                                ch = stripped.at(end);
                                if ((ch != u' ') && (ch != u'\t')) {
                                    break;
                                }
                                end--;
                            }
                            int start = end;
                            while (start >= 0) {
                                ch = stripped.at(start);
                                if (((ch >= u'a') && (ch <= u'z')) || ((ch >= u'A') && (ch <= u'Z')) || ((ch >= u'0') && (ch <= u'9')) || (ch == u'_')) {
                                    start--;
                                } else {
                                    start++;
                                    break;
                                }
                            }
                            identifier = stripped.mid(start, (end - start) + 1);
                            break;
                        }
                        // if we hit something else, we're not going to be able
                        //  to read an assignment identifier
                        else {
                            break;
                        }
                    }
                }
                // if we have a function identifier, make a node
                if (identifier.length() > 0) {
                    // make a node for the function
                    QTreeWidgetItem *parent = nullptr;
                    if (!nodes.isEmpty()) {
                        parent = nodes.last();
                    }
                    if ((m_treeOn->isChecked()) && (parent != nullptr)) {
                        node = new QTreeWidgetItem(parent);
                    } else {
                        node = new QTreeWidgetItem(m_symbols);
                    }
                    // mark the parent as a class (if it's not the root level)
                    if (parent != nullptr) {
                        parent->setIcon(0, m_icon_class);
                        // mark this function as a method of the parent
                        node->setIcon(0, m_icon_function);
                    }
                    // mark root-level functions as classes
                    else {
                        node->setIcon(0, m_icon_class);
                    }
                    // add the function
                    node->setText(0, identifier);
                    node->setText(1, QString::number(i, 10));
                    if (m_expandOn->isChecked()) {
                        m_symbols->expandItem(node);
                    }
                }
            } // (look for functions)

            // look for QML id: ....
            if (QStringView(stripped).mid(c, 3) == QLatin1String("id:")) {
                c += 3;
                identifier.clear();
                // parse the id name
                for (/*c = c*/; c < stripped.length(); c++) {
                    current = stripped.at(c);
                    // look for the beginning of the id
                    if (current == u';') {
                        c--;
                        break;
                    } else {
                        identifier += current;
                    }
                }

                identifier = identifier.trimmed();

                // if we have an id, make a node
                if (identifier.length() > 0) {
                    QTreeWidgetItem *parent = nullptr;
                    if (!nodes.isEmpty()) {
                        parent = nodes.last();
                    }
                    if ((m_treeOn->isChecked()) && (parent != nullptr)) {
                        node = new QTreeWidgetItem(parent);
                    } else {
                        node = new QTreeWidgetItem(m_symbols);
                    }

                    // mark the node as a class
                    node->setIcon(0, m_icon_class);

                    // add the id
                    node->setText(0, identifier);
                    node->setText(1, QString::number(i, 10));
                    if (m_expandOn->isChecked()) {
                        m_symbols->expandItem(node);
                    }
                }
            }

            // keep track of brace depth
            if (current == u'{') {
                // if a node has been added at this level or above,
                //  use it to extend the stack
                if (node != nullptr) {
                    nodes.append(node);
                    // if no node has been added, extend the last node to this depth
                } else if (!nodes.isEmpty()) {
                    nodes.append(nodes.last());
                }
            } else if (current == u'}') {
                // pop the last node off the stack
                node = nullptr;
                if (!nodes.isEmpty()) {
                    nodes.removeLast();
                }
            }
        } // (scan the stripped line)

    } // (iterate through lines of the document)
}
