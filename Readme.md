#  T10-DIF Separate Mode (Split Metadata)
The standard calculates an additional 8 bytes of protection metadata for each data block (4096 bytes), but stores it separately from the raw data on the disk.

Record structure:
1. Guard Tag (2 bytes): CRC16 of the entire data block.
2. Application Tag (2 bytes): to indicate the data type.
3. Reference Tag (4 bytes): part of the LBA (Logical Block Address), to prevent writing a block to the wrong location.

The library determines the minimum sector size of a disk.

The library implements an asynchronous Request Queue, allowing for 32 or 64 DIF write requests to be sent in parallel—AsyncDifStorage. It manages the Submission Queue and Completion Queue using the io-uring library.

# Run with file
## initial dev stage
```bash
cargo build
cargo run --example demo
```

## With fixed block size = 4Kb
```bash
cargo build --example hardware_test
sudo ./target/debug/examples/hardware_test
```

## With real block size supported by device /dev/sdb
```bash
cargo build --example hardware_test_ioctl
sudo ./target/debug/examples/hardware_test_ioctl
```

## Async
```bash
cargo build --example async_demo
sudo ./target/debug/examples/async_demo
```

## Gather
У Acync были проблемы:
```bash
Отправка 4-х асинхронных запросов DIF...
Запрос #0 успешно записан на диск через io_uring
Запрос #1 успешно записан на диск через io_uring
```
Поэтому, чтобы не терять запросы, как в Async, добавлен цикл, который ждет именно столько ответов, сколько было отправлено.
```bash
cargo build --example async_demo_gather
sudo ./target/debug/examples/async_demo_gather
```
У этого варианта проблемы с выравниванием  ```Invalid argument (os error 22)```
Адрес в памяти должен быть выровнен по 512/4096 байт.
В этой версии сегмент (DIF) имел длину 8 байт (iov_len: 8). Контроллер диска не может записать 8 байт напрямую в обход кэша, он умеет писать только целыми секторами.

## combined_demo
Решение проблемы - группировать записи DIF
```bash
cargo build --example combined_demo
sudo ./target/debug/examples/combined_demo
```

# hw_combined_test
Пишет на стандартное устройство ```/dev/sdb1```
```bash
cargo build --example hw_combined_test
sudo ./target/debug/examples/hw_combined_test
```
Результат:
```text
--- Тест Hardware Combined (Data+DIF) на /dev/sdb1 ---
Отправка 4 блоков по 4608 байт...
Блок #0 физически записан на диск.
Блок #1 физически записан на диск.
Блок #2 физически записан на диск.
Блок #3 физически записан на диск.

Чтение обратно для проверки целостности...
Прочитанный DIF: T10Dif { guard_tag: 0, app_tag: 0, ref_tag: 0 }
✅ CRC совпал! Данные на диске идентичны исходным.
```

У этого варианта есть недостаток - последовательный цикл подготовки DIF-меток. Расчет CRC на 4КБ данных — операция простая, но при потоке в несколько гигабайт в секунду один поток CPU станет «бутылочным горлышком».

## Rayon
Для параллельных вычислений в Rust стандартом является Rayon.
```bash
cargo build --example parallel_dif
sudo ./target/debug/examples/parallel_dif
```
результат:
```text
--- Начинаем параллельный расчет CRC для 1024 блоков ---
Расчет завершен за 2.222792ms
✅ Все 1024 блоков защищены и записаны!
```
## stress_test
```bash
cargo build --example stress_test
sudo ./target/debug/examples/stress_test
```
Не хватило места на диске поэтому переразбил
```
Команда (m для справки): n
Номер раздела (1-128, default 1):
Первый сектор (34-3750748814, default 2048):
Last sector, +/-sectors or +/-size{K,M,G,T,P} (2048-3750748814, default 3750748159):

Создан новый раздел 1 с типом 'Linux filesystem' и размером 1,7 TiB.
```
Результат:
```text
🚀 Начинаем 10-секундный стресс-тест на /dev/sdb1...
Размер пачки (Depth): 64, Размер блока: 4608 байт
Прогресс: 10.0с | Скорость: 265.40 MB/s | Записано: 2 ГБ

--- Результаты стресс-теста ---
Время работы: 10.00 с
Всего записано: 2.59 ГБ
Средняя скорость: 265.41 MB/s
Всего блоков с DIF: 603968
```
## stress_test_verify
Предыдущий stress_test мерял чистую скорость, в отличии от этого, который использует цикл Write-Read-Verify.
Это позволит проверить:
- Bit Rot Detection: Если контроллер диска подтвердил запись, но реально записал мусор — verify() это поймает.
- T10-DIF в действии: Мы проверяем не только данные, но и саму структуру метаданных (Guard, App, Ref tags).
- IO Stress: Постоянное переключение между чтением и записью создает высокую нагрузку на очередь команд диска и его кэш.
```bash
cargo build --example stress_test_verify
sudo ./target/debug/examples/stress_test_verify
```
Проблема в кэшировании (Write-Back): Диск сообщил ОС «я всё записал», но на самом деле данные застряли в его внутреннем кэше (RAM диска) и не успели попасть на флэш к моменту чтения:
```text
❌ ОШИБКА CRC на LBA 19120: Ожидалось 522A, Получено 0
   [!] Похоже, прочитан пустой блок (диск не успел записать?)
   [!] Ошибка LBA: Ожидался 19120, Прочитан 0
```
Добавление принудительной синхронизации:
- хорошо для теста, но плохо для производительности
- плохо, если диски будут без защиты по питанию. Если не делать синхронизацию, то нужны диски с энергонезависимым кэшем.

## hw_4k_verify
Добавлен метод ```submit_fsync```
```bash
cargo build --example hw_4k_verify
sudo ./target/debug/examples/hw_4k_verify
```

# Test
```bash
cargo test
```
