# Сервер стаканов L2

Сервер на Rust для хранения и анализа биржевых стаканов. Он принимает снимки, сохраняет их в ClickHouse и отдаёт данные по gRPC. Для выбранного инструмента можно получить историю, лучшие цены покупки и продажи, спред и соотношение объёмов сторон. По истории сервер строит график средней цены между лучшим bid и ask.

В стакане L2 заявки сгруппированы по цене: на каждом уровне указаны цена и общий объём. Здесь используются полные снимки стакана. Клиент относится к следующей части работы.

## Функциональные требования

| № | Требование | Метод | Проверка |
|---|---|---|---|
| 1 | Принимать и сохранять пакеты снимков стакана. | `Ingest` | Сервер сохраняет пакет и возвращает число снимков. Если хотя бы один снимок неверный, весь пакет отклоняется до записи. |
| 2 | Показывать инструменты, для которых есть данные. | `ListSymbols` | После загрузки инструмент появляется в списке. Список отсортирован по алфавиту. |
| 3 | Возвращать последний снимок инструмента. | `GetLatest` | Сервер выбирает самый поздний снимок. При одинаковом времени берёт больший `sequence`. Если данных нет, возвращает `NOT_FOUND`. |
| 4 | Возвращать снимки за указанный период. | `GetSnapshots` | Ответ содержит снимки из периода `[from_ms, to_ms)`, в порядке времени и `sequence`. |
| 5 | Возвращать нужное число лучших уровней стакана. | `GetDepth` | Можно запросить от 1 до 100 уровней на сторону. При глубине 1 возвращаются лучший bid и лучший ask. |
| 6 | Рассчитывать показатели последнего стакана. | `GetSummary` | В ответе есть лучшие цены, спред, mid price, суммы объёмов и imbalance. |
| 7 | Показывать, за какой период есть данные. | `GetRange` | Сервер возвращает время первого и последнего снимка, а также их число без повторов. |
| 8 | Возвращать историю mid price. | `GetMidPrices` | Каждая точка содержит время, `sequence` и среднее лучших цен bid и ask. |
| 9 | Проверять доступность сервера и базы. | `Health`, `GET /health` | Если база отвечает, сервер возвращает `ok`. Иначе возвращает `UNAVAILABLE` по gRPC или `503` по HTTP. |
| 10 | Строить график mid price за период. | `GET /v1/plot/{symbol}` | График возвращается в SVG. На нём указаны инструмент, время и цены. Если снимков за период нет, сервер возвращает `404`. |

## Нефункциональные требования

| № | Требование | Реализация |
|---|---|---|
| 1 | Хранить исходные цены и объёмы без потери точности. | Цены и объёмы хранятся в `u64` с масштабом `10⁶`. Для вычисляемых показателей используется `double`. |
| 2 | Ограничивать размер запросов и ответов. | До 100 снимков в пакете, 100 уровней на сторону и 100 снимков в ответе. Входное сообщение gRPC ограничено 1 MiB. |
| 3 | Ограничивать нагрузку на базу. | Семафор ограничивает число операций до 32. При превышении возвращается `RESOURCE_EXHAUSTED`. |
| 4 | Ограничивать время запроса. | На запрос HTTP или gRPC отводится 10 секунд, на выполнение запроса в ClickHouse 5 секунд. |
| 5 | Сохранять данные после перезапуска. | Данные ClickHouse сохраняются в постоянном томе Docker. |
| 6 | Не показывать повторные снимки при повторной загрузке. | Снимок определяется символом, временем и `sequence`. Повторы убираются при чтении через `ReplacingMergeTree` и `FINAL`. |
| 7 | Защищать запись данных. | Для `Ingest` нужен `x-write-token`. Токен задаётся через окружение. Порты по умолчанию доступны только локально. |
| 8 | Обрабатывать ошибки без аварийного завершения. | Ошибки возвращаются через `Result`. `unwrap`, `expect`, `panic!` и unsafe запрещены настройками Cargo и Clippy. |
| 9 | Запускать и проверять проект стандартными командами. | Devbox, Cargo и Docker Compose. Проверки запускаются одной командой и в GitHub Actions. |
| 10 | Записывать ошибки и корректно завершать работу. | Ошибки пишутся в лог через `tracing`. `RUST_LOG` задаёт уровень логов. Ctrl+C завершает работу обоих серверов. |

## Как устроен сервер

Сервер работает на Tokio. Tonic и Prost отвечают за gRPC, Axum за HTTP и SVG. Для ClickHouse используется официальный Rust клиент. Сама база работает в контейнере, сервер обращается к ней по HTTP.

Обработчики разделяют `Arc<AppState>` с клиентом базы и семафором. Уровни стакана проходят через `BTreeMap`: это позволяет найти повторные цены и сразу получить нужный порядок. Bids сортируются по убыванию цены, asks по возрастанию. В базу уровни отправляются как `Vec`. Повторные ключи снимков внутри пакета тоже проверяются до записи.

Методы и сообщения описаны в [proto/market.proto](proto/market.proto). Проверять API можно через `grpcurl`.

```mermaid
flowchart LR
    Source[Поставщик снимков L2] -->|gRPC Ingest с токеном| Server[Сервер Rust]
    Client[Будущий клиент] -->|gRPC запросы| Server
    Server -->|Чтение и запись по HTTP| Database[(ClickHouse)]
    Database --- Volume[Постоянный Docker том]
    Browser[Просмотр графика] -->|HTTP GET| Plot[Axum]
    Plot -->|История снимков| Database
    Plot -->|SVG| Browser
    Server --- Shared[Arc и лимит 32 операций]
```

```mermaid
sequenceDiagram
    participant P as Поставщик
    participant S as Сервер
    participant D as ClickHouse
    participant C as Клиент
    P->>S: Пакет полных снимков и токен
    S->>S: Проверка токена, уровней и ключей
    S->>S: BTreeMap сортирует bids и asks
    S->>D: INSERT снимков
    D-->>S: Запись завершена
    S-->>P: Число принятых снимков
    C->>S: GetSummary по символу
    S->>D: Последний снимок с FINAL
    D-->>S: Bids и asks
    S->>S: Расчёт спреда, mid price и imbalance
    S-->>C: Показатели стакана
```

## Запуск

Для запуска нужны Devbox, Nix и запущенный Docker. Rustup, protoc и grpcurl установит Devbox.

```sh
devbox install
devbox run db
export WRITE_TOKEN=local-homework-token
devbox run start
```

Если Rust 1.92.0 и protoc уже установлены:

```sh
docker compose up -d --wait
export WRITE_TOKEN=local-homework-token
cargo run
```

При запуске сервер создаёт таблицу, если её ещё нет. Пример настроек лежит в [.env.example](.env.example). Их нужно передать через переменные окружения: сам файл `.env` сервер не читает.

| Переменная | Значение по умолчанию |
|---|---|
| `WRITE_TOKEN` | Нужно задать, минимум 16 байт |
| `CLICKHOUSE_URL` | `http://127.0.0.1:8123` |
| `CLICKHOUSE_DATABASE` | `market` |
| `CLICKHOUSE_USER` | `market` |
| `CLICKHOUSE_PASSWORD` | `local-market-password` |
| `GRPC_ADDR` | `127.0.0.1:50051` |
| `HTTP_ADDR` | `127.0.0.1:3000` |
| `CLIENT_ORIGIN` | `http://localhost:5173` |
| `RUST_LOG` | `info` |

## Пример работы

Откройте второй терминал и выполните `devbox shell`. Затем загрузите три снимка из [examples/snapshots.json](examples/snapshots.json):

```sh
grpcurl -plaintext -import-path proto -proto market.proto -H 'x-write-token: local-homework-token' -d @ localhost:50051 market.v1.MarketData/Ingest < examples/snapshots.json
```

Проверка подключения к базе и список инструментов:

```sh
grpcurl -plaintext -import-path proto -proto market.proto -d '{}' localhost:50051 market.v1.MarketData/Health
grpcurl -plaintext -import-path proto -proto market.proto -d '{}' localhost:50051 market.v1.MarketData/ListSymbols
```

Последний стакан, лучшие уровни, показатели и доступный период:

```sh
grpcurl -plaintext -import-path proto -proto market.proto -d '{"symbol":"BTC-USDT"}' localhost:50051 market.v1.MarketData/GetLatest
grpcurl -plaintext -import-path proto -proto market.proto -d '{"symbol":"BTC-USDT","levels":1}' localhost:50051 market.v1.MarketData/GetDepth
grpcurl -plaintext -import-path proto -proto market.proto -d '{"symbol":"BTC-USDT"}' localhost:50051 market.v1.MarketData/GetSummary
grpcurl -plaintext -import-path proto -proto market.proto -d '{"symbol":"BTC-USDT"}' localhost:50051 market.v1.MarketData/GetRange
```

После загрузки в базе будет три снимка `BTC-USDT`. Если отправить тот же файл ещё раз, при чтении по прежнему вернутся три снимка.

История стакана и mid price:

```sh
grpcurl -plaintext -import-path proto -proto market.proto -d '{"symbol":"BTC-USDT","fromMs":"1791540000000","toMs":"1791540003000","limit":100}' localhost:50051 market.v1.MarketData/GetSnapshots
grpcurl -plaintext -import-path proto -proto market.proto -d '{"symbol":"BTC-USDT","fromMs":"1791540000000","toMs":"1791540003000","limit":100}' localhost:50051 market.v1.MarketData/GetMidPrices
```

После загрузки примера откройте [график BTC-USDT](http://localhost:3000/v1/plot/BTC-USDT?from_ms=1791540000000&to_ms=1791540003000). Для проверки базы через HTTP откройте [health](http://localhost:3000/health).

### Что получится

В последнем снимке лучший bid равен `62001`, его объём `3`. Лучший ask равен `62003`, его объём `2`. Поэтому показатели будут такими:

| Показатель | Значение |
|---|---|
| Спред | `62003 − 62001 = 2` |
| Mid price | `(62001 + 62003) / 2 = 62002` |
| Imbalance | `(3 − 2) / (3 + 2) = 0.2` |
| История mid price | `62000.5 → 62004 → 62002` |

Этот график получен от сервера после загрузки примера:

![История mid price BTC-USDT](docs/mid-price.svg)

[SVG файл](docs/mid-price.svg). Сохранить график локально:

```sh
curl --fail 'http://localhost:3000/v1/plot/BTC-USDT?from_ms=1791540000000&to_ms=1791540003000' -o /tmp/mid-price.svg
```

