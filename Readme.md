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
У этого арианта проблемы с выравниванием  ```Invalid argument (os error 22)```
Адрес в памяти должен быть выровнен по 512/4096 байт.
В этой версии сегмент (DIF) имел длину 8 байт (iov_len: 8). Контроллер диска не может записать 8 байт напрямую в обход кэша, он умеет писать только целыми секторами.

## combined_demo
Решение проблемы - группировать записи DIF
```bash
cargo build --example combined_demo
sudo ./target/debug/examples/combined_demo
```


# Test
```bash
cargo test
```
