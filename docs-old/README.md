<p align="right">
   <strong>中文</strong> | <a href="./README.en.md">English</a>
</p>
<div align="center">

![new-api](/web/public/logo.png)

# New API

🍥Новое поколение шлюза для больших моделей и система управления AI-активами

<a href="https://trendshift.io/repositories/8227" target="_blank"><img src="https://trendshift.io/api/badge/repositories/8227" alt="Calcium-Ion%2Fnew-api | Trendshift" style="width: 250px; height: 55px;" width="250" height="55"/></a>

<p align="center">
  <a href="https://raw.githubusercontent.com/Calcium-Ion/new-api/main/LICENSE">
    <img src="https://img.shields.io/github/license/Calcium-Ion/new-api?color=brightgreen" alt="license">
  </a>
  <a href="https://github.com/Calcium-Ion/new-api/releases/latest">
    <img src="https://img.shields.io/github/v/release/Calcium-Ion/new-api?color=brightgreen&include_prereleases" alt="release">
  </a>
  <a href="https://github.com/users/Calcium-Ion/packages/container/package/new-api">
    <img src="https://img.shields.io/badge/docker-ghcr.io-blue" alt="docker">
  </a>
  <a href="https://hub.docker.com/r/CalciumIon/new-api">
    <img src="https://img.shields.io/badge/docker-dockerHub-blue" alt="docker">
  </a>
  <a href="https://goreportcard.com/report/github.com/Calcium-Ion/new-api">
    <img src="https://goreportcard.com/badge/github.com/Calcium-Ion/new-api" alt="GoReportCard">
  </a>
</p>
</div>

## 📝 Описание проекта

> [!NOTE]  
> Этот проект является открытым исходным кодом, разработанным на основе [One API](https://github.com/songquanpeng/one-api)

> [!IMPORTANT]  
> - Этот проект предназначен только для личного обучения, не гарантирует стабильность и не предоставляет техническую поддержку.
> - Пользователи должны соблюдать [условия использования](https://openai.com/policies/terms-of-use) OpenAI и **законодательство**, не использовать в незаконных целях.
> - В соответствии с требованиями [«Временных мер по управлению услугами генеративного искусственного интеллекта»](http://www.cac.gov.cn/2023-07/13/c_1690898327029107.htm), не предоставляйте публике в Китае какие-либо незарегистрированные услуги генеративного искусственного интеллекта.

## 📚 Документация

Подробная документация доступна на нашей официальной Wiki: [https://docs.newapi.pro/](https://docs.newapi.pro/)

## ✨ Основные особенности

New API предоставляет богатый функционал, подробное описание можно найти в [описании функций](https://docs.newapi.pro/wiki/features-introduction):

1. 🎨 Полностью новый пользовательский интерфейс
2. 🌍 Поддержка нескольких языков
3. 💰 Поддержка функции онлайн-пополнения (EasyPay)
4. 🔍 Поддержка запроса использования квоты по ключу (в сочетании с [neko-api-key-tool](https://github.com/Calcium-Ion/neko-api-key-tool))
5. 🔄 Совместимость с базой данных оригинальной версии One API
6. 💵 Поддержка оплаты моделей по количеству запросов
7. ⚖️ Поддержка взвешенного случайного выбора каналов
8. 📈 Панель данных (консоль)
9. 🔒 Группировка токенов, ограничения моделей
10. 🤖 Поддержка дополнительных методов авторизации (LinuxDO, Telegram, OIDC)
11. 🔄 Поддержка моделей Rerank (Cohere и Jina), [документация API](https://docs.newapi.pro/api/jinaai-rerank)
12. ⚡ Поддержка OpenAI Realtime API (включая канал Azure), [документация API](https://docs.newapi.pro/api/openai-realtime)
13. ⚡ Поддержка формата Claude Messages, [документация API](https://docs.newapi.pro/api/anthropic-chat)
14. Поддержка входа в интерфейс чата через маршрут /chat2link
15. 🧠 Поддержка настройки reasoning effort через суффикс названия модели:
    1. Модели OpenAI серии o
        - Добавление суффикса `-high` для high reasoning effort (например: `o3-mini-high`)
        - Добавление суффикса `-medium` для medium reasoning effort (например: `o3-mini-medium`)
        - Добавление суффикса `-low` для low reasoning effort (например: `o3-mini-low`)
    2. Модели Claude с режимом мышления
        - Добавление суффикса `-thinking` для включения режима мышления (например: `claude-3-7-sonnet-20250219-thinking`)
16. 🔄 Функция преобразования мышления в контент
17. 🔄 Функция ограничения скорости для пользователей по моделям
18. 💰 Поддержка тарификации кэша, при включении позволяет взимать плату по установленному соотношению при попадании в кэш:
    1. Установите опцию `Коэффициент кэширования подсказок` в `Системные настройки-Операционные настройки`
    2. Установите `Коэффициент кэширования подсказок` в канале, диапазон 0-1, например, 0.5 означает 50% оплаты при попадании в кэш
    3. Поддерживаемые каналы:
        - [x] OpenAI
        - [x] Azure
        - [x] DeepSeek
        - [x] Claude

## Поддержка моделей

Эта версия поддерживает различные модели, подробности см. в [документации API-Relay API](https://docs.newapi.pro/api):

1. Сторонние модели **gpts** (gpt-4-gizmo-*)
2. Сторонний канал [Midjourney-Proxy(Plus)](https://github.com/novicezk/midjourney-proxy), [документация API](https://docs.newapi.pro/api/midjourney-proxy-image)
3. Сторонний канал [Suno API](https://github.com/Suno-API/Suno-API), [документация API](https://docs.newapi.pro/api/suno-music)
4. Пользовательские каналы с возможностью указания полного адреса вызова
5. Модели Rerank ([Cohere](https://cohere.ai/) и [Jina](https://jina.ai/)), [документация API](https://docs.newapi.pro/api/jinaai-rerank)
6. Формат Claude Messages, [документация API](https://docs.newapi.pro/api/anthropic-chat)
7. Dify, в настоящее время поддерживается только chatflow

## Конфигурация переменных окружения

Подробное описание конфигурации см. в [Руководстве по установке-Конфигурация переменных окружения](https://docs.newapi.pro/installation/environment-variables):

- `GENERATE_DEFAULT_TOKEN`: Генерировать ли начальный токен для новых пользователей, по умолчанию `false`
- `STREAMING_TIMEOUT`: Тайм-аут потокового ответа, по умолчанию 60 секунд
- `DIFY_DEBUG`: Выводить ли информацию о рабочем процессе и узлах канала Dify, по умолчанию `true`
- `FORCE_STREAM_OPTION`: Переопределять ли параметр stream_options клиента, по умолчанию `true`
- `GET_MEDIA_TOKEN`: Учитывать ли токены изображений, по умолчанию `true`
- `GET_MEDIA_TOKEN_NOT_STREAM`: Учитывать ли токены изображений в непотоковом режиме, по умолчанию `true`
- `UPDATE_TASK`: Обновлять ли асинхронные задачи (Midjourney, Suno), по умолчанию `true`
- `COHERE_SAFETY_SETTING`: Настройки безопасности модели Cohere, возможные значения: `NONE`, `CONTEXTUAL`, `STRICT`, по умолчанию `NONE`
- `GEMINI_VISION_MAX_IMAGE_NUM`: Максимальное количество изображений для модели Gemini, по умолчанию `16`
- `MAX_FILE_DOWNLOAD_MB`: Максимальный размер загружаемого файла в МБ, по умолчанию `20`
- `CRYPTO_SECRET`: Ключ шифрования для шифрования содержимого базы данных
- `AZURE_DEFAULT_API_VERSION`: Версия API канала Azure по умолчанию, по умолчанию `2025-04-01-preview`
- `NOTIFICATION_LIMIT_DURATION_MINUTE`: Продолжительность ограничения уведомлений, по умолчанию `10` минут
- `NOTIFY_LIMIT_COUNT`: Максимальное количество уведомлений пользователя в течение указанного периода, по умолчанию `2`

## Развертывание

Подробное руководство по развертыванию см. в [Руководстве по установке-Методы развертывания](https://docs.newapi.pro/installation):

> [!TIP]
> Последний образ Docker: `calciumion/new-api:latest`  

### Особенности многомашинного развертывания
- Необходимо установить переменную окружения `SESSION_SECRET`, иначе это приведет к несогласованности состояния входа при многомашинном развертывании
- Если используется общий Redis, необходимо установить `CRYPTO_SECRET`, иначе это приведет к невозможности получения содержимого Redis при многомашинном развертывании

### Требования к развертыванию
- Локальная база данных (по умолчанию): SQLite (при развертывании Docker необходимо монтировать директорию `/data`)
- Удаленная база данных: MySQL версии >= 5.7.8, PgSQL версии >= 9.6

### Методы развертывания

#### Развертывание с использованием функции Docker панели BT
Установите панель BT (версия **9.2.0** и выше), найдите **New-API** в магазине приложений и установите.
[Иллюстрированное руководство](./docs/BT.md)

#### Развертывание с использованием Docker Compose (рекомендуется)
```shell
# Скачать проект
git clone https://github.com/Calcium-Ion/new-api.git
cd new-api
# При необходимости отредактируйте docker-compose.yml
# Запуск
docker-compose up -d
```

#### Прямое использование образа Docker
```shell
# Использование SQLite
docker run --name new-api -d --restart always -p 3000:3000 -e TZ=Asia/Shanghai -v /home/ubuntu/data/new-api:/data calciumion/new-api:latest

# Использование MySQL
docker run --name new-api -d --restart always -p 3000:3000 -e SQL_DSN="root:123456@tcp(localhost:3306)/oneapi" -e TZ=Asia/Shanghai -v /home/ubuntu/data/new-api:/data calciumion/new-api:latest
```

## Повторные попытки канала и кэширование
Функция повторных попыток канала реализована, вы можете установить количество повторных попыток в `Настройки->Операционные настройки->Общие настройки`, **рекомендуется включить** функцию кэширования.

### Метод настройки кэширования
1. `REDIS_CONN_STRING`: Установка Redis в качестве кэша
2. `MEMORY_CACHE_ENABLED`: Включение кэширования в памяти (если установлен Redis, ручная настройка не требуется)

## Документация API

Подробную документацию API см. в [Документации API](https://docs.newapi.pro/api):

- [Интерфейс чата (Chat)](https://docs.newapi.pro/api/openai-chat)
- [Интерфейс изображений (Image)](https://docs.newapi.pro/api/openai-image)
- [Интерфейс переупорядочивания (Rerank)](https://docs.newapi.pro/api/jinaai-rerank)
- [Интерфейс реального времени (Realtime)](https://docs.newapi.pro/api/openai-realtime)
- [Интерфейс чата Claude (messages)](https://docs.newapi.pro/api/anthropic-chat)

## Связанные проекты
- [One API](https://github.com/songquanpeng/one-api): Оригинальный проект
- [Midjourney-Proxy](https://github.com/novicezk/midjourney-proxy): Поддержка интерфейса Midjourney
- [chatnio](https://github.com/Deeptrain-Community/chatnio): Комплексное решение следующего поколения для B/C-конечных AI
- [neko-api-key-tool](https://github.com/Calcium-Ion/neko-api-key-tool): Запрос использования квоты по ключу

Другие проекты на основе New API:
- [new-api-horizon](https://github.com/Calcium-Ion/new-api-horizon): Высокопроизводительная оптимизированная версия New API
- [VoAPI](https://github.com/VoAPI/VoAPI): Версия с улучшенным фронтендом на основе New API

## Поддержка

Если у вас есть вопросы, обратитесь к [Поддержке](https://docs.newapi.pro/support):
- [Общение сообщества](https://docs.newapi.pro/support/community-interaction)
- [Обратная связь по проблемам](https://docs.newapi.pro/support/feedback-issues)
- [Часто задаваемые вопросы](https://docs.newapi.pro/support/faq)

## 🌟 История звезд

[![Star History Chart](https://api.star-history.com/svg?repos=Calcium-Ion/new-api&type=Date)](https://star-history.com/#Calcium-Ion/new-api&Date)
