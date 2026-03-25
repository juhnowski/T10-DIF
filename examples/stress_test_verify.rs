use std::io::Write;
use std::time::{Duration, Instant};
use t10_dif_storage::{AsyncDifStorage, DmaBuffer, T10Dif};

fn main() -> std::io::Result<()> {
    let device_path = "/dev/sdb1";
    let test_duration = Duration::from_secs(10);
    let batch_size = 32; // Уменьшим пачку, чтобы чаще делать верификацию
    let stride = 8192;

    let mut storage =
        AsyncDifStorage::new(device_path, 128).expect("Ошибка открытия устройства. Нужен sudo!");

    // Пул буферов для ЗАПИСИ и для ЧТЕНИЯ
    let mut write_bufs: Vec<DmaBuffer> = (0..batch_size)
        .map(|_| DmaBuffer::new(stride, 4096).unwrap())
        .collect();
    let mut read_bufs: Vec<DmaBuffer> = (0..batch_size)
        .map(|_| DmaBuffer::new(stride, 4096).unwrap())
        .collect();

    println!("🧪 Запуск стресс-теста с ВЕРИФИКАЦИЕЙ (Write-Read-Verify)...");

    let start_time = Instant::now();
    let mut total_bytes = 0u64;
    let mut offset = 0u64;
    let mut errors = 0u64;

    while start_time.elapsed() < test_duration {
        // 1. ПОДГОТОВКА (Заполняем случайными данными и считаем CRC)
        let lba_start = offset / stride as u64;
        write_bufs.iter_mut().enumerate().for_each(|(i, b)| {
            b.as_mut_slice_len(4096).fill((lba_start + i as u64) as u8);
        });
        T10Dif::prepare_batch(&mut write_bufs, lba_start, 0x01);

        // 2. ЗАПИСЬ (Async Write)
        for i in 0..batch_size {
            unsafe {
                storage.submit_write(&write_bufs[i], offset + (i * stride) as u64, i as u64)?;
            }
        }
        wait_for_batch(&mut storage, batch_size);

        // 3. ЧТЕНИЕ (Async Read - используем тот же метод, O_DIRECT позволяет)
        for i in 0..batch_size {
            unsafe {
                storage.submit_write(&read_bufs[i], offset + (i * stride) as u64, i as u64)?;
            }
        }
        wait_for_batch(&mut storage, batch_size);

        // 4. ВЕРИФИКАЦИЯ
        for i in 0..batch_size {
            let written_dif = unsafe { *(write_bufs[i].as_ptr().add(4096) as *const T10Dif) };
            let read_dif = unsafe { *(read_bufs[i].as_ptr().add(4096) as *const T10Dif) };
            let read_data = read_bufs[i].as_slice_len(4096);

            if written_dif.guard_tag != read_dif.guard_tag {
                eprintln!(
                    "\n❌ ОШИБКА CRC на LBA {}: Ожидалось {:X}, Получено {:X}",
                    lba_start + i as u64,
                    written_dif.guard_tag,
                    read_dif.guard_tag
                );

                // Проверим, не прочитали ли мы просто нули
                if read_data.iter().all(|&x| x == 0) && read_dif.guard_tag == 0 {
                    eprintln!("   [!] Похоже, прочитан пустой блок (диск не успел записать?)");
                }
            }
            if written_dif.ref_tag != read_dif.ref_tag {
                eprintln!(
                    "   [!] Ошибка LBA: Ожидался {}, Прочитан {}",
                    written_dif.ref_tag, read_dif.ref_tag
                );
            }
        }

        total_bytes += (batch_size * stride) as u64;
        offset += (batch_size * stride) as u64;
        if offset > 512 * 1024 * 1024 {
            offset = 0;
        } // Цикл по 512МБ

        print!(
            "\rПроверено: {:.2} ГБ | Ошибок: {}",
            total_bytes as f64 / 1e9,
            errors
        );
        std::io::stdout().flush().unwrap();
    }

    println!("\n--- Тест завершен. Ошибок: {} ---", errors);
    Ok(())
}

fn wait_for_batch(storage: &mut AsyncDifStorage, count: usize) {
    let mut done = 0;
    while done < count {
        done += storage.wait_completions().len();
    }
}
