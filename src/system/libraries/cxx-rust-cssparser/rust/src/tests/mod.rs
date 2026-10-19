// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

// Helper macro to generate multiple test functions from a list of names,
// functions, input values and expected values.
macro_rules! test_cases {
    ( $($label:ident : $eval:ident $inp:expr, $exp:expr);* $(;)? ) => {
        $(
            #[test]
            fn $label() {
                $eval($inp, $exp);
            }
        )*
    }
}

mod propertysyntax;
mod propertyvalue;
mod selectorparser;
mod selector;
mod propertyfunction;
