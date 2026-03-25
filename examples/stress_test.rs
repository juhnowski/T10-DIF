use std::time::{Duration, Instant};
use t10_dif_storage::{AsyncDifStorage, DmaBuffer, T10Dif};

fn main() -> std::io::Result<()> {
    let device_path = "/dev/sdb1"; // ПРЕДУПРЕЖДЕНИЕ: Данные будут перезаписаны!
    let test_duration = Duration::from_secs(10); // Длительность теста
    let batch_size = 64; // Количество блоков в одной пачке (QD64)
    let stride = 4608; // 4096 + 512

    let mut storage = AsyncDifStorage::new(device_path, 128)
        .expect("Ошибка открытия устройства. Запустите через sudo!");

    // 1. Предварительно выделяем пул буферов
    let mut buffers: Vec<DmaBuffer> = (0..batch_size)
        .map(|_| DmaBuffer::new(stride, 4096).unwrap())
        .collect();

    println!("🚀 Начинаем 10-секундный стресс-тест на {}...", device_path);
    println!(
        "Размер пачки (Depth): {}, Размер блока: {} байт",
        batch_size, stride
    );

    let start_time = Instant::now();
    let mut total_bytes: u64 = 0;
    let mut total_blocks: u64 = 0;
    let mut offset: u64 = 0;

    while start_time.elapsed() < test_duration {
        // 2. Параллельная подготовка данных и CRC (Rayon)
        // Каждый раз меняем LBA, чтобы имитировать реальную запись
        T10Dif::prepare_batch(&mut buffers, offset / stride as u64, 0x01);

        // 3. Отправка пачки в io_uring
        for i in 0..batch_size {
            unsafe {
                // Если дошли до конца раздела (условно 1ГБ для теста), сбрасываем офсет
                if offset > 1024 * 1024 * 1024 {
                    offset = 0;
                }

                storage.submit_write(&buffers[i], offset, i as u64)?;
                offset += stride as u64;
            }
        }

        // 4. Ждем завершения всей пачки (блокирующий вызов)
        let mut completed_in_batch = 0;
        while completed_in_batch < batch_size {
            let ids = storage.wait_completions();
            completed_in_batch += ids.len();
        }

        total_blocks += batch_size as u64;
        total_bytes += (batch_size * stride) as u64;

        // Вывод промежуточной статистики каждую секунду
        if total_blocks % (batch_size as u64 * 10) == 0 {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = (total_bytes as f64 / 1024.0 / 1024.0) / elapsed;
            print!(
                "\rПрогресс: {:.1}с | Скорость: {:.2} MB/s | Записано: {} ГБ",
                elapsed,
                speed,
                total_bytes / 1024 / 1024 / 1024
            );
            use std::io::Write;
            std::io::stdout().flush().unwrap();
        }
    }

    let final_elapsed = start_time.elapsed().as_secs_f64();
    let final_speed = (total_bytes as f64 / 1024.0 / 1024.0) / final_elapsed;

    println!("\n\n--- Результаты стресс-теста ---");
    println!("Время работы: {:.2} с", final_elapsed);
    println!(
        "Всего записано: {:.2} ГБ",
        total_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    );
    println!("Средняя скорость: {:.2} MB/s", final_speed);
    println!("Всего блоков с DIF: {}", total_blocks);

    Ok(())
}
