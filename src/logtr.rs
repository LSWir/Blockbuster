//! Перевод сообщений сканера в логе: ядро пишет по-русски, при показе (и в файле лога) подставляем фрагменты на выбранном языке.
use std::sync::OnceLock;

const TABLE: &[(&str, &str, &str)] = &[
    ("Папка или диск не найдены", "Folder or drive not found", "Теку або диск не знайдено"),
    ("не удалось записать libraries.json", "could not write libraries.json", "не вдалося записати libraries.json"),
    ("Язык каталога:", "Catalog language:", "Мова каталогу:"),
    ("(описания, жанры и постеры TMDB)", "(TMDB descriptions, genres and posters)", "(описи, жанри та постери TMDB)"),
    ("OMDb/IMDb (ключей:", "OMDb/IMDb (keys:", "OMDb/IMDb (ключів:"),
    ("Онлайн-данные:", "Online data:", "Онлайн-дані:"),
    ("(первый запуск долгий, дальше результаты берутся из кэша)", "(the first run is long, later results come from the cache)", "(перший запуск довгий, далі результати беруться з кешу)"),
    ("(API-ключи не заданы — онлайн-поиск постеров и описаний выключен, см. «Ключи и настройки»)", "(no API keys set — online search for posters and descriptions is off, see “Keys and settings”)", "(API-ключі не задано — онлайн-пошук постерів і описів вимкнено, див. «Ключі та налаштування»)"),
    ("Сканирование:", "Scanning:", "Сканування:"),
    ("(ffmpeg не найден — обложки, вшитые в MKV/MP4, не извлекаются; положите ffmpeg.exe рядом с ffprobe)", "(ffmpeg not found — covers embedded in MKV/MP4 are not extracted; put ffmpeg.exe next to ffprobe)", "(ffmpeg не знайдено — обкладинки, вшиті в MKV/MP4, не витягуються; покладіть ffmpeg.exe поруч із ffprobe)"),
    ("(ffprobe не найден — длительность/разрешение берутся только из .nfo; обложки из MKV не читаются)", "(ffprobe not found — duration/resolution come only from .nfo; covers from MKV are not read)", "(ffprobe не знайдено — тривалість/роздільна здатність беруться лише з .nfo; обкладинки з MKV не читаються)"),
    ("записей", "entries", "записів"),
    ("не удалось сохранить кэш", "could not save the cache", "не вдалося зберегти кеш"),
    ("■ Остановлено пользователем (найденное сохранено в кэш — следующий запуск продолжит с этого места)", "Stopped by the user (what was found is saved to the cache — the next run continues from here)", "Зупинено користувачем (знайдене збережено в кеш — наступний запуск продовжить із цього місця)"),
    ("[ОШИБКА] неверный ключ API", "[ERROR] invalid API key", "[ПОМИЛКА] неправильний ключ API"),
    ("Онлайн-запросы: найдено новых", "Online requests: new found", "Онлайн-запити: знайдено нових"),
    ("взято из кэша", "from cache", "взято з кешу"),
    ("не найдено", "not found", "не знайдено"),
    ("Ошибок сети:", "Network errors:", "Помилок мережі:"),
    ("последняя:", "last:", "остання:"),
    ("Проверьте интернет и доступ к api.themoviedb.org и omdbapi.com (VPN, брандмауэр, антивирус).", "Check your internet connection and access to api.themoviedb.org and omdbapi.com (VPN, firewall, antivirus).", "Перевірте інтернет і доступ до api.themoviedb.org та omdbapi.com (VPN, брандмауер, антивірус)."),
    ("ffprobe: новых", "ffprobe: new", "ffprobe: нових"),
    ("из кэша", "from cache", "з кешу"),
    ("ошибок", "errors", "помилок"),
    ("Кэш не создан — сохранять нечего:", "No cache created — nothing to save:", "Кеш не створено — зберігати нічого:"),
    ("онлайн-поиск выключен (нет ключей или галочка «без интернета»)", "online search is off (no keys, or “offline” is ticked)", "онлайн-пошук вимкнено (немає ключів або позначено «без інтернету»)"),
    ("ни один онлайн-запрос не вернул данных", "no online request returned data", "жоден онлайн-запит не повернув даних"),
    ("ffprobe не обработал ни одного файла", "ffprobe did not process any file", "ffprobe не обробив жодного файлу"),
    ("ffprobe не найден", "ffprobe not found", "ffprobe не знайдено"),
    ("нет данных", "no data", "немає даних"),
    ("Кэш (", "Cache (", "Кеш ("),
    ("; НЕ распознаны (проверьте имена):", "; NOT recognized (check the names):", "; НЕ розпізнано (перевірте імена):"),
    ("Логотипы: распознано", "Logos: recognized", "Логотипи: розпізнано"),
    ("скопировано в", "copied to", "скопійовано в"),
    ("не удалось записать", "could not write", "не вдалося записати"),
    ("Готово! Всего карточек:", "Done! Total cards:", "Готово! Усього карток:"),
    ("Файл:", "File:", "Файл:"),
    ("... обработано серий:", "... episodes processed:", "... оброблено серій:"),
    ("— серий:", "— episodes:", "— серій:"),
    ("ответ не JSON:", "response is not JSON:", "відповідь не JSON:"),
    (", перехожу к следующему (осталось ключей:", ", switching to the next one (keys left:", ", переходжу до наступного (залишилось ключів:"),
    ("ключ …", "key …", "ключ …"),
    ("не работает (лимит или неверный)", "does not work (limit reached or invalid)", "не працює (ліміт або неправильний)"),
    ("все ключи исчерпали лимит или неверны — OMDb отключён до конца запуска", "all keys are out of quota or invalid — OMDb is disabled until the end of the run", "усі ключі вичерпали ліміт або неправильні — OMDb вимкнено до кінця запуску"),
    ("неверный API-ключ — TMDB отключён", "invalid API key — TMDB is disabled", "неправильний API-ключ — TMDB вимкнено"),
    ("(онлайн: нет связи с сервером для", "(online: no connection to the server for", "(онлайн: немає зв'язку із сервером для"),
    ("(онлайн: ничего не найдено для", "(online: nothing found for", "(онлайн: нічого не знайдено для"),
    ("; пробовал:", "; tried:", "; пробував:"),
    ("(найдено по варианту названия", "(found by title variant", "(знайдено за варіантом назви"),
    ("не удалось прочитать постер", "could not read poster", "не вдалося прочитати постер"),
    ("Загружаю рейтинги IMDb", "Loading IMDb ratings", "Завантажую рейтинги IMDb"),
    ("не удалось скачать датасет рейтингов IMDb", "could not download the IMDb ratings dataset", "не вдалося завантажити датасет рейтингів IMDb"),
    ("; использую прошлую копию", "; using the previous copy", "; використовую попередню копію"),
    ("не удалось прочитать датасет рейтингов IMDb", "could not read the IMDb ratings dataset", "не вдалося прочитати датасет рейтингів IMDb"),
    ("Рейтинг IMDb из датасета:", "IMDb rating from the dataset:", "Рейтинг IMDb з датасету:"),
    ("карточек", "cards", "карток"),
    ("не удалось скопировать логотипы в", "could not copy logos to", "не вдалося скопіювати логотипи в"),
    ("логотипы будут встроены в HTML", "logos will be embedded in the HTML", "логотипи буде вбудовано в HTML"),
    ("кэш вне корня диска", "cache is outside the drive root", "кеш поза коренем диска"),
    ("нет доступа на запись в", "no write access to", "немає доступу на запис до"),
    ("права восстановлены", "permissions restored", "права відновлено"),
    ("кэш будет храниться здесь:", "the cache will be stored here:", "кеш зберігатиметься тут:"),
    ("(чтобы вернуть кэш на диск: удалите папку", "(to move the cache back to the drive: delete the folder", "(щоб повернути кеш на диск: видаліть теку"),
    ("от имени администратора или выдайте себе «Полный доступ» в Свойства → Безопасность)", "as administrator or give yourself “Full control” in Properties → Security)", "від імені адміністратора або надайте собі «Повний доступ» у Властивості → Безпека)"),
    ("закрыт для записи — index.html и log.txt будут сохранены в", "is read-only — index.html and log.txt will be saved to", "закрито для запису — index.html і log.txt буде збережено в"),
    ("! корень ", "! root ", "! корінь "),
    ("искали как:", "searched as:", "шукали як:"),
    ("[сериал, серий:", "[TV show, episodes:", "[серіал, серій:"),
    ("Каталог ", "Catalog ", "Каталог "),
    ("Карточек:", "Cards:", "Карток:"),
    ("найдено, но без постера:", "found but without a poster:", "знайдено, але без постера:"),
    ("пропущено папок:", "folders skipped:", "пропущено тек:"),
    ("! Онлайн-поиск был выключен (нет ключей или --offline): список «не найдено» неполон/недостоверен.", "! Online search was off (no keys or --offline): the “not found” list is incomplete/unreliable.", "! Онлайн-пошук був вимкнений (немає ключів або --offline): список «не знайдено» неповний/недостовірний."),
    ("=== НЕ НАЙДЕНО В ИНТЕРНЕТЕ (нет IMDb/TMDB ID) — переименуйте в «Название (Год)» ===", "=== NOT FOUND ONLINE (no IMDb/TMDB ID) — rename to “Title (Year)” ===", "=== НЕ ЗНАЙДЕНО В ІНТЕРНЕТІ (немає IMDb/TMDB ID) — перейменуйте на «Назва (Рік)» ==="),
    ("=== НАЙДЕНО, НО БЕЗ ПОСТЕРА ===", "=== FOUND, BUT WITHOUT A POSTER ===", "=== ЗНАЙДЕНО, АЛЕ БЕЗ ПОСТЕРА ==="),
    ("=== ПРОПУЩЕННЫЕ ПАПКИ (по имени) ===", "=== SKIPPED FOLDERS (by name) ===", "=== ПРОПУЩЕНІ ТЕКИ (за назвою) ==="),
    ("  (нет)", "  (none)", "  (немає)"),
    ("Лог не найденного:", "Log of what was not found:", "Лог незнайденого:"),
    ("не удалось прочитать", "could not read", "не вдалося прочитати"),
    ("не найден по пути:", "not found at:", "не знайдено за шляхом:"),
    ("найден рядом с программой:", "found next to the program:", "знайдено поруч із програмою:"),
    ("не найден", "not found", "не знайдено"),
    ("без постера", "without a poster", "без постера"),
    ("[Фильмы]", "[Movies]", "[Фільми]"),
    ("[Сериалы]", "[TV shows]", "[Серіали]"),
    ("[Мультфильмы]", "[Animation]", "[Мультфільми]"),
    ("[Документальное]", "[Documentaries]", "[Документальне]"),
    ("Фильмы:", "Movies:", "Фільми:"),
    ("Сериалы:", "TV shows:", "Серіали:"),
    ("Мультфильмы:", "Animation:", "Мультфільми:"),
    ("Документальное:", "Documentaries:", "Документальне:"),
    ("МБ)", "MB)", "МБ)"),
    ("не удалось запустить ffprobe", "could not start ffprobe", "не вдалося запустити ffprobe"),
    ("ffprobe вернул пустой ответ", "ffprobe returned an empty response", "ffprobe повернув порожню відповідь"),
];

fn sorted() -> &'static Vec<(&'static str, &'static str, &'static str)> {
    static S: OnceLock<Vec<(&'static str, &'static str, &'static str)>> = OnceLock::new();
    S.get_or_init(|| {
        let mut v: Vec<_> = TABLE.to_vec();
        v.sort_by(|a, b| b.0.chars().count().cmp(&a.0.chars().count())); // длинные фрагменты раньше коротких
        v
    })
}

/// lang: 0 — русский (без изменений), 1 — English, 2 — українська.
pub fn translate(lang: i32, s: &str) -> String {
    if lang != 1 && lang != 2 {
        return s.to_string();
    }
    if !s.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)) {
        return s.to_string();
    }
    let mut out = s.to_string();
    for (ru, en, uk) in sorted().iter() {
        if out.contains(ru) {
            out = out.replace(ru, if lang == 1 { en } else { uk });
        }
    }
    if out.contains("from the dataset:") || out.contains("з датасету:") {
        out = out.replace(" из ", if lang == 1 { " of " } else { " з " });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_lines() {
        assert_eq!(translate(0, "Сканирование: D:\\"), "Сканирование: D:\\");
        assert_eq!(translate(1, "Сканирование: D:\\"), "Scanning: D:\\");
        assert_eq!(
            translate(1, "Онлайн-запросы: найдено новых 121, взято из кэша 0, не найдено 6"),
            "Online requests: new found 121, from cache 0, not found 6"
        );
        assert!(translate(2, "  + [Фильмы] a.mkv").contains("[Фільми]"));
    }
}
