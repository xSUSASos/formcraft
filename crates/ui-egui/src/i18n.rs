//! RU/EN localisation of the interface. Strings are written in English in the code and wrapped
//! in [`tr`]; the Russian table translates them. Command labels are translated by command id
//! ([`command_label`]), so plugins keep their own labels. Missing entries fall back to English.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    En,
    Ru,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Ru];
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::Ru as u8 { Lang::Ru } else { Lang::En }
}

/// English text → Russian. Keep sorted by area; keys are the exact English strings used in
/// the code.
const RU: &[(&str, &str)] = &[
    // tabs, groups, workspace
    ("SOLID", "ТЕЛО"),
    ("SKETCH", "ЭСКИЗ"),
    ("INSPECT", "АНАЛИЗ"),
    ("TOOLS", "ИНСТРУМЕНТЫ"),
    ("DESIGN", "КОНСТРУИРОВАНИЕ"),
    ("CREATE", "СОЗДАТЬ"),
    ("MODIFY", "ИЗМЕНИТЬ"),
    ("CONSTRUCT", "ПОСТРОИТЬ"),
    ("CONSTRAINTS", "ЗАВИСИМОСТИ"),
    ("FINISH SKETCH", "ЗАВЕРШИТЬ ЭСКИЗ"),
    ("ADD-INS", "НАДСТРОЙКИ"),
    ("UTILITY", "УТИЛИТЫ"),
    ("SCRIPTS", "СКРИПТЫ"),
    // browser
    ("BROWSER", "БРАУЗЕР"),
    ("Document Settings", "Параметры документа"),
    ("Named Views", "Именованные виды"),
    ("Origin", "Начало координат"),
    ("Sketches", "Эскизы"),
    ("Bodies", "Тела"),
    ("Units: mm", "Единицы: мм"),
    ("(none)", "(нет)"),
    // view cube
    ("TOP", "ВЕРХ"),
    ("FRONT", "ПЕРЕД"),
    ("RIGHT", "ПРАВО"),
    ("LEFT", "ЛЕВО"),
    ("BACK", "ЗАД"),
    ("BOTTOM", "НИЗ"),
    // dialogs and buttons
    ("OK", "OK"),
    ("Cancel", "Отмена"),
    ("Select", "Выбрать"),
    ("Accept", "Принять"),
    ("Reject", "Отклонить"),
    ("Generate", "Создать"),
    ("Stop", "Стоп"),
    ("Show", "Показать"),
    ("Close", "Закрыть"),
    ("Help", "Справка"),
    ("Preferences", "Настройки"),
    ("ASSISTANT", "АССИСТЕНТ"),
    ("Use selection", "Использовать выбор"),
    ("Describe a part…", "Опишите деталь…"),
    ("Search commands…", "Поиск команд…"),
    ("TEXT COMMANDS", "ТЕКСТОВЫЕ КОМАНДЫ"),
    ("Timeline: no features yet", "Таймлайн: пока нет элементов"),
    ("Scripts and Add-Ins", "Скрипты и надстройки"),
    ("Install from .zip…", "Установить из .zip…"),
    ("Open folder", "Открыть папку"),
    ("Run", "Запустить"),
    ("Run file…", "Запустить файл…"),
    ("Unload", "Выгрузить"),
    ("Reload", "Перезагрузить"),
    ("Log", "Журнал"),
    // settings
    ("SETTINGS", "НАСТРОЙКИ"),
    ("Theme", "Тема"),
    ("Light", "Светлая"),
    ("Dark", "Тёмная"),
    ("Language", "Язык"),
    ("Interface scale", "Масштаб интерфейса"),
    ("Animations", "Анимации"),
    ("Assistant backend", "Сервер ассистента"),
    ("Model", "Модель"),
    ("Server URL", "Адрес сервера"),
    ("Check connection", "Проверить соединение"),
    ("Apply", "Применить"),
    ("Open logs folder", "Открыть папку журналов"),
    ("Reset layout", "Сбросить раскладку"),
    // marking menu / misc
    ("Edit with AI…", "Изменить с ИИ…"),
    ("Create Sketch on Face", "Эскиз на грани"),
    ("Ready", "Готово"),
    ("Fit", "Вписать"),
    ("Group", "Группа"),
    // tabs, groups (title case)
    ("Solid", "Тело"),
    ("Sketch", "Эскиз"),
    ("Inspect", "Анализ"),
    ("Tools", "Инструменты"),
    ("Create", "Создать"),
    ("Modify", "Изменить"),
    ("Construct", "Построить"),
    ("Constraints", "Зависимости"),
    ("Finish Sketch", "Завершить эскиз"),
    ("Add-Ins", "Надстройки"),
    ("Utility", "Утилиты"),
    ("Design", "Конструирование"),
    ("Not available yet", "Пока недоступно"),
    // app bar and menus
    ("File, View, Help", "Файл, Вид, Справка"),
    ("Save (Ctrl+S)", "Сохранить (Ctrl+S)"),
    ("Undo (Ctrl+Z)", "Отменить (Ctrl+Z)"),
    ("Redo (Ctrl+Y)", "Повторить (Ctrl+Y)"),
    ("Assistant (describe a part to build)", "Ассистент (опишите деталь)"),
    ("Search commands (S)", "Поиск команд (S)"),
    ("Close (new design)", "Закрыть (новый проект)"),
    ("New Design", "Новый проект"),
    ("Open…", "Открыть…"),
    ("Save", "Сохранить"),
    ("Export…", "Экспорт…"),
    ("View", "Вид"),
    ("About FormCraft", "О программе FormCraft"),
    ("Browser", "Браузер"),
    ("Timeline", "Таймлайн"),
    ("Text Commands", "Текстовые команды"),
    ("ViewCube", "Видовой куб"),
    ("Navigation Bar", "Панель навигации"),
    ("Float Browser", "Плавающий браузер"),
    ("Reset Layout", "Сбросить раскладку"),
    ("Layout reset", "Раскладка сброшена"),
    ("Float", "Открепить"),
    ("Pin to the canvas", "Закрепить на холсте"),
    ("Collapse", "Свернуть"),
    ("Show Text Commands", "Показать текстовые команды"),
    ("Hide", "Скрыть"),
    ("failed", "— ошибка"),
    ("more", "ещё"),
    // browser tree and selection
    ("Fully constrained", "Полностью определён"),
    ("degrees of freedom", "ст. свободы"),
    ("Failed to compute", "Не удалось построить"),
    ("Face", "Грань"),
    ("Edge", "Ребро"),
    ("Selected:", "Выбрано:"),
    ("face", "грань"),
    ("edge", "ребро"),
    ("of", "тела"),
    ("items selected", "объектов выбрано"),
    ("selected", "выбрано"),
    ("HOME", "ДОМ"),
    ("Home", "Домой"),
    ("View:", "Вид:"),
    ("custom", "произвольный"),
    ("Section Analysis off", "Анализ сечения выключен"),
    (
        "Timeline: drag icons to reorder, drag the marker to roll back, right-click for more",
        "Таймлайн: перетаскивайте значки для смены порядка, маркер — для отката, правая кнопка — меню",
    ),
    (
        "Unsaved changes: save (Ctrl+S) or click again to discard them",
        "Есть несохранённые изменения: сохраните (Ctrl+S) или нажмите ещё раз, чтобы отбросить",
    ),
    ("This feature cannot be edited", "Этот элемент нельзя изменить"),
    ("AI proposal rejected", "Предложение ИИ отклонено"),
    ("Look At: sketch plane", "Смотреть на плоскость эскиза"),
    ("Sketch finished", "Эскиз завершён"),
    (
        "Select sketch entities, then pick the constraint in the Sketch Palette",
        "Выберите объекты эскиза, затем зависимость в палитре эскиза",
    ),
    (
        "Create a sketch first (Create Sketch, then pick a plane)",
        "Сначала создайте эскиз (Создать эскиз, затем выберите плоскость)",
    ),
    (
        "Sketch: L line, R rectangle, C circle, D dimension, Esc select",
        "Эскиз: L линия, R прямоугольник, C окружность, D размер, Esc выбор",
    ),
    // create sketch dialog
    ("Create Sketch", "Создать эскиз"),
    ("Select a plane:", "Выберите плоскость:"),
    ("plane", "плоскость"),
    ("Top (ground)", "Сверху (земля)"),
    ("Front", "Спереди"),
    ("Right", "Справа"),
    ("Selected face", "Выбранная грань"),
    ("Selected face (select a planar face first)", "Выбранная грань (сначала выберите плоскую грань)"),
    ("Rename", "Переименовать"),
    ("Rename…", "Переименовать…"),
    // navigation bar
    ("Orbit (Shift + middle drag)", "Орбита (Shift + перетаскивание средней кнопкой)"),
    ("Pan (middle drag)", "Панорама (средняя кнопка)"),
    ("Zoom (wheel)", "Масштаб (колесо)"),
    ("Fit (F6, double-click middle button)", "Вписать (F6, двойной щелчок средней кнопкой)"),
    ("Visual Style", "Визуальный стиль"),
    ("Grid", "Сетка"),
    ("Orthographic / Perspective", "Ортогональная / перспективная"),
    ("Shaded", "С тонированием"),
    ("Shaded with Edges", "С тонированием и рёбрами"),
    ("Wireframe", "Каркас"),
    // dialogs
    ("Edit", "Изменить"),
    ("Cancel (Esc)", "Отмена (Esc)"),
    ("OK (Enter)", "OK (Enter)"),
    ("Clear", "Очистить"),
    ("(default)", "(по умолчанию)"),
    ("Use the faces/edges selected on the canvas", "Использовать грани/рёбра, выбранные на холсте"),
    ("e.g. top", "напр. top"),
    ("e.g. vertical", "напр. vertical"),
    ("mm", "мм"),
    ("deg", "град"),
    ("New Body", "Новое тело"),
    ("Join", "Объединение"),
    ("Cut", "Вырезание"),
    ("Intersect", "Пересечение"),
    ("One side", "В одну сторону"),
    ("Symmetric", "Симметрично"),
    ("Angle", "Угол"),
    ("Arcs", "Дуги"),
    ("Axis", "Ось"),
    ("Base", "Основание"),
    ("Body", "Тело"),
    ("Center", "Центр"),
    ("Circles", "Окружности"),
    ("Comment", "Комментарий"),
    ("Corner", "Угол (вершина)"),
    ("Counterbore", "Цековка"),
    ("Countersink", "Зенковка"),
    ("Density", "Плотность"),
    ("Depth", "Глубина"),
    ("Diameter", "Диаметр"),
    ("Direction", "Направление"),
    ("Distance", "Расстояние"),
    ("Edges", "Рёбра"),
    ("Expression", "Выражение"),
    ("Faces", "Грани"),
    ("Feature", "Элемент"),
    ("Filter", "Фильтр"),
    ("Height", "Высота"),
    ("Index", "Индекс"),
    ("Keep tools", "Сохранить инструменты"),
    ("Length", "Длина"),
    ("Lines", "Линии"),
    ("Material", "Материал"),
    ("Name", "Имя"),
    ("Offset", "Смещение"),
    ("Operation", "Операция"),
    ("Path", "Путь"),
    ("Plane", "Плоскость"),
    ("Points", "Точки"),
    ("Polylines", "Ломаные"),
    ("Profiles", "Профили"),
    ("Prompt", "Запрос"),
    ("Radius", "Радиус"),
    ("Rectangles", "Прямоугольники"),
    ("Size", "Размер"),
    ("Suppressed", "Подавлен"),
    ("Target", "Цель"),
    ("Thickness", "Толщина"),
    ("Through all", "Насквозь"),
    ("Tool", "Инструмент"),
    ("Value", "Значение"),
    ("Visible", "Видимость"),
    ("Width", "Ширина"),
    ("Abs", "ABS"),
    ("Aluminium", "Алюминий"),
    ("Brass", "Латунь"),
    ("Cast iron", "Чугун"),
    ("Copper", "Медь"),
    ("Nylon", "Нейлон"),
    ("Petg", "PETG"),
    ("Pla", "PLA"),
    ("Stainless", "Нержавеющая сталь"),
    ("Steel", "Сталь"),
    ("Titanium", "Титан"),
    ("Wood oak", "Дуб"),
    // assistant
    ("Assistant", "Ассистент"),
    ("Rejected", "Отклонено"),
    ("Generating…", "Генерация…"),
    ("stopped", "остановлено"),
    ("not configured", "не настроен"),
    ("GB VRAM", "ГБ видеопамяти"),
    ("offline", "не в сети"),
    ("status not checked yet", "состояние ещё не проверено"),
    ("attempts", "попыток"),
    ("s", "с"),
    (
        "Describe a part or a change, e.g. “flange D100, 4 holes D9 on D80, 10 thick”.",
        "Опишите деталь или изменение, например «фланец D100, 4 отверстия D9 на D80, толщина 10».",
    ),
    ("Change the selection, e.g. “4 holes M5 in the corners”", "Измените выбранное, например «4 отверстия M5 в углах»"),
    // add-ins
    ("No add-ins installed.", "Надстройки не установлены."),
    ("Commands:", "Команды:"),
    ("Unloaded:", "Выгружена:"),
    ("(empty log)", "(журнал пуст)"),
    ("No scripts. Put .py files into the scripts folder.", "Скриптов нет. Положите .py файлы в папку скриптов."),
    // timeline
    ("Move to beginning", "К началу"),
    ("Step back", "Шаг назад"),
    ("Step forward", "Шаг вперёд"),
    ("Move to end", "К концу"),
    ("Timeline settings", "Настройки таймлайна"),
    ("features — click to expand", "элементов — щёлкните, чтобы развернуть"),
    ("Error:", "Ошибка:"),
    ("Warning:", "Предупреждение:"),
    ("suppressed", "подавлен"),
    ("rolled back", "откачен"),
    ("Edit Feature", "Изменить элемент"),
    ("Unsuppress Features", "Снять подавление"),
    ("Suppress Features", "Подавить элементы"),
    ("Roll History Marker Here", "Переместить маркер сюда"),
    ("Explain", "Объяснить"),
    ("Describe this feature in words", "Описать этот элемент словами"),
    ("Delete", "Удалить"),
    // marking menu, shortcut box, parameters
    ("Repeat", "Повторить"),
    ("Unpin", "Открепить"),
    ("Pin to favourites", "В избранное"),
    ("Do it with AI", "Сделать с ИИ"),
    ("No matching command", "Нет подходящей команды"),
    ("Parameters", "Параметры"),
    ("+ Add user parameter", "+ Добавить параметр"),
    ("name", "имя"),
    (
        "Expressions may use other parameters, units (mm, cm, in, deg) and functions.",
        "В выражениях можно использовать другие параметры, единицы (mm, cm, in, deg) и функции.",
    ),
    // sketch palette
    ("Sketch palette", "Палитра эскиза"),
    ("Sketch Palette", "Палитра эскиза"),
    ("Options", "Опции"),
    ("Look At", "Смотреть на"),
    ("Look straight at the sketch plane", "Смотреть строго на плоскость эскиза"),
    ("Snap", "Привязка"),
    ("Show Constraints", "Показать зависимости"),
    ("Constraint badges are not drawn yet", "Значки зависимостей пока не рисуются"),
    ("Show Dimensions", "Показать размеры"),
    ("Slice", "Сечение"),
    ("Dimension", "Размер"),
    ("Line", "Линия"),
    ("Rectangle", "Прямоугольник"),
    ("Circle", "Окружность"),
    ("3-Point Arc", "Дуга по 3 точкам"),
    ("Coincident", "Совпадение"),
    ("Point on curve", "Точка на кривой"),
    ("Horizontal", "Горизонтально"),
    ("Vertical", "Вертикально"),
    ("Parallel", "Параллельно"),
    ("Perpendicular", "Перпендикулярно"),
    ("Tangent", "Касательно"),
    ("Equal", "Равенство"),
    ("Concentric", "Концентричность"),
    ("Midpoint", "Середина"),
    ("Fix", "Зафиксировать"),
    ("select suitable geometry first", "сначала выберите подходящую геометрию"),
    ("Sketch failed", "Эскиз не построен"),
];

/// Translate a UI string (English in, current language out).
pub fn tr(en: &str) -> &str {
    tr_in(current(), en)
}

pub fn tr_in(lang: Lang, en: &str) -> &str {
    match lang {
        Lang::En => en,
        Lang::Ru => RU.iter().find(|(k, _)| *k == en).map_or(en, |(_, v)| *v),
    }
}

/// Russian labels of built-in commands (by id).
const RU_COMMANDS: &[(&str, &str)] = &[
    ("sketch.create", "Создать эскиз"),
    ("solid.extrude", "Выдавить"),
    ("solid.revolve", "Вращать"),
    ("solid.hole", "Отверстие"),
    ("solid.box", "Параллелепипед"),
    ("solid.cylinder", "Цилиндр"),
    ("solid.sphere", "Сфера"),
    ("ai.generate", "Сгенерировать"),
    ("solid.press_pull", "Нажать/вытянуть"),
    ("solid.fillet", "Скругление"),
    ("solid.chamfer", "Фаска"),
    ("solid.shell", "Оболочка"),
    ("solid.combine", "Объединить"),
    ("solid.move", "Переместить"),
    ("doc.parameters", "Изменить параметры"),
    ("construct.offset_plane", "Плоскость со смещением"),
    ("inspect.measure", "Измерить"),
    ("inspect.section", "Анализ сечения"),
    ("inspect.properties", "Свойства"),
    ("tools.scripts", "Скрипты и надстройки"),
    ("tools.text_commands", "Текстовые команды"),
    ("tools.settings", "Настройки"),
    ("sketch.line", "Линия"),
    ("sketch.rectangle", "Прямоугольник"),
    ("sketch.circle", "Окружность"),
    ("sketch.arc", "Дуга по 3 точкам"),
    ("sketch.dimension", "Размер"),
    ("sketch.coincident", "Совпадение"),
    ("sketch.horizontal", "Горизонтально/вертикально"),
    ("sketch.parallel", "Параллельно"),
    ("sketch.perpendicular", "Перпендикулярно"),
    ("sketch.tangent", "Касательно"),
    ("sketch.equal", "Равенство"),
    ("sketch.fix", "Зафиксировать"),
    ("sketch.finish", "Завершить эскиз"),
    ("doc.new", "Новый проект"),
    ("doc.open", "Открыть…"),
    ("doc.save", "Сохранить"),
    ("doc.export", "Экспорт…"),
    ("doc.import", "Импорт STEP"),
    ("doc.undo", "Отменить"),
    ("doc.redo", "Повторить"),
    ("model.inspect", "Сведения о модели"),
    ("body.rename", "Переименовать тело"),
    ("body.visibility", "Показать/скрыть тело"),
    ("feature.edit", "Изменить элемент"),
    ("feature.delete", "Удалить элемент"),
    ("feature.rename", "Переименовать элемент"),
    ("feature.suppress", "Подавить элемент"),
    ("select.clear", "Снять выбор"),
    ("app.about", "О программе FormCraft"),
    ("ai.explain", "Объяснить"),
    ("doc.parameter", "Задать параметр"),
    ("doc.delete_parameter", "Удалить параметр"),
    ("construct.midplane", "Средняя плоскость"),
    ("construct.axis", "Ось через две точки"),
];

/// The label of a command in the current language (`fallback` = its English label).
pub fn command_label<'a>(id: &str, fallback: &'a str) -> &'a str {
    match current() {
        Lang::En => fallback,
        Lang::Ru => RU_COMMANDS.iter().find(|(k, _)| *k == id).map_or(fallback, |(_, v)| *v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_and_falls_back() {
        assert_eq!(tr_in(Lang::Ru, "Cancel"), "Отмена");
        assert_eq!(tr_in(Lang::En, "Cancel"), "Cancel");
        assert_eq!(tr_in(Lang::Ru, "Something new"), "Something new");
        let mut keys: Vec<&str> = RU.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(n, keys.len(), "duplicate keys in the RU table");
    }
}
