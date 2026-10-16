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
    ("Сделать с ИИ", "Сделать с ИИ"),
    ("Ready", "Готово"),
    ("Fit", "Вписать"),
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
