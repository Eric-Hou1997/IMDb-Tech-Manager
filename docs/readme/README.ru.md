<div align="center">

# IMDb Tech Manager

[简体中文](../../README.md) | [繁體中文](./README.zh-Hant.md) | [English](./README.en.md) | [Français](./README.fr.md) | **Русский** | [日本語](./README.ja.md) | [Español](./README.es.md) | [ไทย](./README.th.md)

[![Release](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=release)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Downloads](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=downloads)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)

<img src="../../macos/assets/ITM_logo.png" alt="IMDb Tech Manager" width="220">

Управление техническими характеристиками фильмов и метаданными медиатеки.

</div>

## О проекте

IMDb Tech Manager (ITM) получает и структурирует IMDb Technical Specifications. Данные о камерах, объективах, форматах съёмки, звуке, соотношении сторон и производственных процессах можно безопасно сохранять в NFO. Приложение также предоставляет Inspector, предварительный просмотр, отмену, пакетные задачи и управление техническими тегами с помощью локальных правил или ИИ.

ITM можно использовать вместе с [Tech Card Manager (TCM)](https://github.com/Eric-Hou1997/Tech-Card-Manager):

```text
IMDb → ITM → NFO / Technical Specifications → TCM → отображение в Emby
```

## Версия и платформа

- Стабильная версия: [`v4.1.0`](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/tag/v4.1.0)
- Платформа: macOS 12 или новее, Apple Silicon (arm64)
- Архив: `ITM-v4.1.0-MacOS-AArch64-APP.zip`
- Имя приложения в архиве: `IMDb Tech Manager.app`
- Release содержит SHA-256, подпись OTA Ed25519, инструкции и список изменений

## Основные возможности

- Получение, кэширование и структурирование IMDb Technical Specifications
- Безопасная запись NFO только в `<technicalspecs>`
- Inspector, предварительный просмотр, проверка хэша, резервное копирование и отмена
- Раздельное владение тегами External, Generated и Manual
- Независимые поиск, фильтры, выбор и пакетные задачи для фильмов и сериалов
- Сменяемые провайдеры ИИ OpenAI-compatible и Anthropic
- Реальный учёт HTTP-запросов, токенов, кэша и стоимости
- OTA с отдельными ошибками прокси, лимитов GitHub, отсутствующих ресурсов, загрузки и подписи

## Языки

Упрощённый китайский, традиционный китайский и английский (США) встроены в приложение. Французский, русский, японский, испанский и тайский распространяются отдельными пакетами в Release `v4.1.0` и загружаются только после скачивания и проверки.

Web UI, Go Core, Python Engine и нативные меню macOS используют общее состояние языка. Язык журнала и пояснений фиксируется при запуске задачи. Переключение интерфейса не переписывает старые журналы, кэш, NFO, сведения о владении или пользовательские запросы.

## Границы безопасности

- Spec Agent изменяет только `<technicalspecs>` и никогда не меняет корневые `<tag>`.
- Автоматизация работает только с Generated Tech Tags, владение которыми доказано.
- External, Manual, TMM и теги других приложений сохраняются.
- Проверяются или сохраняются XML, UTF-8 BOM, окончания строк, режим файла, резервная копия и атомарная замена.
- При неоднозначном пути, конфликте владения или изменении файла после просмотра операция безопасно останавливается.

## Интерфейс

![Управление данными](../images/data-management.png)

![Управление тегами](../images/tag-management.png)

## Дорожная карта

Готово: выпуск для Apple Silicon, безопасность и владение NFO, Local/AI-теги, управление задачами и расходами, встроенный трёхъязычный интерфейс, пять загружаемых языков, обновления с привязкой к версии и регрессионные проверки.

В работе: нормализация Technical Specifications, больше испытаний на реальных медиатеках, совместимость ИИ-провайдеров, восстановление после ошибок, Developer ID и notarization, а также другие платформы и медиасерверы.

## Разработка и лицензия

Перед разработкой прочитайте [`AGENTS.md`](../../AGENTS.md). Архитектура языковых пакетов описана в [`docs/language-packs.md`](../language-packs.md).

Проект распространяется по [Apache License 2.0](../../LICENSE). IMDb, Emby и другие товарные знаки принадлежат их владельцам. Проект не связан, не авторизован и не одобрен IMDb.com, Inc. или Emby LLC.
