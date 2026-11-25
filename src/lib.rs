//! Blockbuster — ядро: сканирование библиотеки, поиск данных в интернете, генерация index.html.

macro_rules! lazy_rx {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static crate::rx::Rx {
            static C: std::sync::OnceLock<crate::rx::Rx> = std::sync::OnceLock::new();
            C.get_or_init(|| crate::rx::Rx::new($pat))
        }
    };
    ($name:ident, $pat:expr, i) => {
        fn $name() -> &'static crate::rx::Rx {
            static C: std::sync::OnceLock<crate::rx::Rx> = std::sync::OnceLock::new();
            C.get_or_init(|| crate::rx::Rx::ci($pat))
        }
    };
}

pub mod rx;
pub mod util;
pub mod jv;
pub mod ctx;
pub mod titles;
pub mod nfo;
pub mod probe;
pub mod online;
pub mod scan;
pub mod output;
pub mod protocol;
pub mod run;
pub mod config;
pub mod i18n;
pub mod logtr;
