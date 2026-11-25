//! Тексты интерфейса: 0 — русский, 1 — English, 2 — українська. (Сгенерировано; сообщения ядра остаются русскими.)

pub const LANGS: [&str; 3] = ["ru", "en", "uk"];

pub const T: &[(&str, [&str; 3])] = &[
    ("subtitle", ["Каталог фильмов и сериалов одним HTML: постеры, описания, рейтинги, дорожки и кодеки", "Movie and TV catalog in a single HTML: posters, descriptions, ratings, audio tracks and codecs", "Каталог фільмів і серіалів в одному HTML: постери, описи, рейтинги, доріжки та кодеки"]),
    ("drive-or-folder", ["ДИСК ИЛИ ПАПКА", "DRIVE OR FOLDER", "ДИСК АБО ТЕКА"]),
    ("paste", ["Вставить", "Paste", "Вставити"]),
    ("browse", ["Обзор…", "Browse…", "Огляд…"]),
    ("keys-settings", ["Ключи и настройки", "Keys and settings", "Ключі та налаштування"]),
    ("tmdb-key", ["TMDB ключ", "TMDB key", "Ключ TMDB"]),
    ("omdb-keys", ["OMDb ключ(и), через запятую", "OMDb key(s), comma-separated", "Ключ(і) OMDb, через кому"]),
    ("ffprobe-folder", ["Папка ffprobe (пусто = рядом с программой / PATH)", "ffprobe folder (empty = next to the program / PATH)", "Тека ffprobe (порожньо = поруч із програмою / PATH)"]),
    ("skip-folders", ["Пропускать папки (пусто = X, XX, XXX)", "Skip folders (empty = X, XX, XXX)", "Пропускати теки (порожньо = X, XX, XXX)"]),
    ("show-keys", ["показать ключи", "show keys", "показати ключі"]),
    ("save", ["Сохранить", "Save", "Зберегти"]),
    ("scan", ["▶  Сканировать", "▶  Scan", "▶  Сканувати"]),
    ("stop", ["Стоп", "Stop", "Стоп"]),
    ("offline", ["без интернета (быстро, только локальные данные)", "offline (fast, local data only)", "без інтернету (швидко, лише локальні дані)"]),
    ("embed-logos", ["один файл (встроить логотипы в HTML)", "single file (embed logos in HTML)", "один файл (вбудувати логотипи в HTML)"]),
    ("autoscroll", ["автопрокрутка лога", "auto-scroll log", "автопрокрутка логу"]),
    ("open-catalog", ["Открыть каталог", "Open catalog", "Відкрити каталог"]),
    ("log-txt", ["log.txt", "log.txt", "log.txt"]),
    ("drive-folder", ["Папка диска", "Drive folder", "Тека диска"]),
    ("about", ["О программе", "About", "Про програму"]),
    ("description", ["Blockbuster сканирует папки с фильмами и сериалами на выбранном диске и собирает один автономный index.html: постеры, описания, рейтинги IMDb и Rotten Tomatoes, аудиодорожки, субтитры и кодеки. Рядом создаются log.txt (что не найдено — чтобы переименовать файлы) и скрытая папка .catalog_cache.", "Blockbuster scans the movie and TV folders of the chosen drive and builds one self-contained index.html: posters, descriptions, IMDb and Rotten Tomatoes ratings, audio tracks, subtitles and codecs. It also writes log.txt (what was not found, so you can rename the files) and a hidden .catalog_cache folder.", "Blockbuster скануює теки з фільмами та серіалами на вибраному диску й збирає один автономний index.html: постери, описи, рейтинги IMDb і Rotten Tomatoes, аудіодоріжки, субтитри та кодеки. Поруч створюються log.txt (що не знайдено — щоб перейменувати файли) і прихована тека .catalog_cache."]),
    ("links-note", ["Данные берутся из TMDB, OMDb и датасетов IMDb (только при сканировании, без галочки «без интернета»), технические сведения — из ffprobe. Ссылки открываются в браузере.", "Data comes from TMDB, OMDb and the IMDb datasets (only while scanning, unless “offline” is ticked); technical details come from ffprobe. Links open in your browser.", "Дані беруться з TMDB, OMDb і датасетів IMDb (лише під час сканування, без позначки «без інтернету»), технічні відомості — з ffprobe. Посилання відкриваються в браузері."]),
    ("version", ["Версия", "Version", "Версія"]),
    ("previous", ["Предыдущая версия", "Previous version", "Попередня версія"]),
    ("language", ["Язык", "Language", "Мова"]),
    ("theme", ["Тема", "Theme", "Тема"]),
    ("light", ["Светлая", "Light", "Світла"]),
    ("dark", ["Тёмная", "Dark", "Темна"]),
    ("close", ["Закрыть", "Close", "Закрити"]),
    ("project", ["Страница проекта", "Project page", "Сторінка проєкту"]),
    ("st-choose", ["Выберите диск или папку", "Choose a drive or folder", "Виберіть диск або теку"]),
    ("st-notfound", ["Диск или папка не найдены: {}", "Drive or folder not found: {}", "Диск або теку не знайдено: {}"]),
    ("st-scanning", ["Идёт сканирование…", "Scanning…", "Триває сканування…"]),
    ("st-saved", ["Настройки сохранены", "Settings saved", "Налаштування збережено"]),
    ("st-save-fail", ["Не удалось сохранить настройки: {}", "Could not save settings: {}", "Не вдалося зберегти налаштування: {}"]),
    ("st-stopping", ["Останавливаю…", "Stopping…", "Зупиняю…"]),
    ("st-closing", ["Останавливаю сканирование и закрываю окно…", "Stopping the scan and closing the window…", "Зупиняю сканування й закриваю вікно…"]),
    ("st-done", ["Готово", "Done", "Готово"]),
    ("st-stopped", ["Остановлено", "Stopped", "Зупинено"]),
    ("st-error", ["Завершилось с ошибкой", "Finished with an error", "Завершилося з помилкою"]),
    ("log-start", ["Сканирую {} ...", "Scanning {} ...", "Сканую {} ..."]),
    ("log-panic", ["[ОШИБКА] внутренняя ошибка программы (panic). Сообщите, на каком этапе это случилось.", "[ERROR] internal program error (panic). Please report at which step it happened.", "[ПОМИЛКА] внутрішня помилка програми (panic). Повідомте, на якому етапі це сталося."]),
    ("dlg-target", ["Выберите диск или папку с фильмами", "Choose a drive or a folder with movies", "Виберіть диск або теку з фільмами"]),
    ("dlg-ffprobe", ["Папка с ffprobe", "Folder with ffprobe", "Тека з ffprobe"]),
];

/// Строка интерфейса на языке `lang` (0 ru, 1 en, 2 uk); при неизвестном ключе возвращает сам ключ.
pub fn tr(lang: i32, key: &str) -> &'static str {
    let i = if (0..3).contains(&lang) { lang as usize } else { 0 };
    for (k, v) in T.iter() {
        if *k == key {
            return v[i];
        }
    }
    "?"
}

/// Подставляет значение вместо «{}».
pub fn tr_arg(lang: i32, key: &str, arg: &str) -> String {
    tr(lang, key).replace("{}", arg)
}

pub fn lang_code(lang: i32) -> &'static str {
    LANGS[if (0..3).contains(&lang) { lang as usize } else { 0 }]
}

pub fn lang_from_code(code: &str) -> i32 {
    LANGS.iter().position(|c| *c == code).map(|p| p as i32).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_translations_present() {
        for (k, v) in T.iter() {
            for s in v.iter() {
                assert!(!s.is_empty(), "пустой перевод: {}", k);
            }
        }
        assert_eq!(tr(1, "stop"), "Stop");
        assert_eq!(tr_arg(2, "st-notfound", "D:\\"), "Диск або теку не знайдено: D:\\");
        assert_eq!(lang_from_code("uk"), 2);
    }
}
