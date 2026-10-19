// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use precomputed_hash::PrecomputedHash;
use cssparser::ToCss;

#[derive(Eq, PartialEq, Clone, Default, Debug)]
pub struct Identifier(String);

impl PrecomputedHash for Identifier {
    fn precomputed_hash(&self) -> u32 {
        // let Identifier(contents) = self;
        return 0;
    }
}

impl ToCss for Identifier {
    fn to_css<W>(&self, dest: &mut W) -> std::fmt::Result
    where
    W: std::fmt::Write {
        dest.write_str(&self.0)
    }
}

impl <'a> From<&'a str> for Identifier {
    fn from(value: &'a str) -> Self {
        return Identifier(value.to_string());
    }
}

impl std::borrow::Borrow<String> for Identifier {
    fn borrow(&self) -> &String {
        &self.0
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Into<String> for Identifier {
    fn into(self) -> String {
        self.0
    }
}

impl Into<String> for &Identifier {
    fn into(self) -> String {
        self.0.clone()
    }
}
