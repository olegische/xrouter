# Документация Midjourney Proxy API

**Введение**: Документация Midjourney Proxy API

## Список интерфейсов
Поддерживаемые интерфейсы:
+ [x] /mj/submit/imagine
+ [x] /mj/submit/change
+ [x] /mj/submit/blend
+ [x] /mj/submit/describe
+ [x] /mj/image/{id} (Получение изображения через этот интерфейс, **обязательно укажите адрес сервера в системных настройках!!**)
+ [x] /mj/task/{id}/fetch (Адрес изображения, возвращаемый этим интерфейсом, перенаправляется через One API)
+ [x] /task/list-by-condition
+ [x] /mj/submit/action (поддерживается только midjourney-proxy-plus, аналогично для следующих)
+ [x] /mj/submit/modal
+ [x] /mj/submit/shorten
+ [x] /mj/task/{id}/image-seed
+ [x] /mj/insight-face/swap (InsightFace)

## Список моделей

### Поддерживается midjourney-proxy

- mj_imagine (создание изображения)
- mj_variation (вариация)
- mj_reroll (перерисовка)
- mj_blend (смешивание)
- mj_upscale (увеличение)
- mj_describe (изображение в текст)

### Поддерживается только midjourney-proxy-plus

- mj_zoom (масштабирование)
- mj_shorten (сокращение подсказок)
- mj_modal (отправка через окно, локальная перерисовка и настраиваемое масштабирование должны добавляться вместе с mj_modal)
- mj_inpaint (отправка локальной перерисовки, должна добавляться вместе с mj_modal)
- mj_custom_zoom (настраиваемое масштабирование, должно добавляться вместе с mj_modal)
- mj_high_variation (сильная вариация)
- mj_low_variation (слабая вариация)
- mj_pan (панорамирование)
- swap_face (замена лица)

## Настройка цен моделей (настраивается в Настройки-Операционные настройки-Настройка фиксированных цен моделей)
```json
{
  "mj_imagine": 0.1,
  "mj_variation": 0.1,
  "mj_reroll": 0.1,
  "mj_blend": 0.1,
  "mj_modal": 0.1,
  "mj_zoom": 0.1,
  "mj_shorten": 0.1,
  "mj_high_variation": 0.1,
  "mj_low_variation": 0.1,
  "mj_pan": 0.1,
  "mj_inpaint": 0,
  "mj_custom_zoom": 0,
  "mj_describe": 0.05,
  "mj_upscale": 0.05,
  "swap_face": 0.05
}
```
Цены для mj_inpaint и mj_custom_zoom установлены в 0, поскольку эти две модели должны использоваться вместе с mj_modal, поэтому цена определяется mj_modal.

## Настройка канала

### Интеграция с midjourney-proxy(plus)

1. Разверните Midjourney-Proxy и настройте учетную запись midjourney и т.д. (настоятельно рекомендуется установить ключ), [адрес проекта](https://github.com/novicezk/midjourney-proxy)

2. Добавьте канал в управлении каналами, выберите тип канала **Midjourney Proxy**, если это версия plus, выберите **Midjourney Proxy Plus**, модели см. в списке моделей выше
3. В поле **Прокси** укажите адрес развернутого midjourney-proxy, например: http://localhost:8080
4. В поле ключа введите ключ midjourney-proxy, если ключ не установлен, можно ввести что угодно

### Интеграция с вышестоящим new api

1. Добавьте канал в управлении каналами, выберите тип канала **Midjourney Proxy Plus**, модели см. в списке моделей выше
2. В поле **Прокси** укажите адрес вышестоящего new api, например: http://localhost:3000
3. В поле ключа введите ключ вышестоящего new api
