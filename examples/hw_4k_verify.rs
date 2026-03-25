use std::time::{Duration, Instant};
use t10_dif_storage::{AsyncDifStorage, DmaBuffer, T10Dif};

fn main() -> std::io::Result<()> {
    let device_path = "/dev/sdb1";
    let stride = 8192; // 4K Data + 4K DIF Slot
    let batch_size = 16;

    let mut storage = AsyncDifStorage::new(device_path, 64)?;
    let mut w_bufs: Vec<DmaBuffer> = (0..batch_size)
        .map(|_| DmaBuffer::new_aligned_pair().unwrap())
        .collect();
    let mut r_bufs: Vec<DmaBuffer> = (0..batch_size)
        .map(|_| DmaBuffer::new_aligned_pair().unwrap())
        .collect();

    let mut current_offset = 0u64;

    for cycle in 1..=5 {
        let lba_base = current_offset / 4096;
        println!(
            "\n--- Цикл {} | Offset: {} | LBA: {} ---",
            cycle, current_offset, lba_base
        );

        // 1. Подготовка
        for (i, buf) in w_bufs.iter_mut().enumerate() {
            buf.data_part_mut().fill(0xAA); // Заполняем паттерном
            let lba = (lba_base + (i * 2) as u64) as u32; // i*2 потому что stride=8192 (2 сектора)
            *buf.dif_part_mut() = T10Dif::compute(buf.data_part_mut(), 0x01, lba);
        }

        // 2. ЗАПИСЬ
        for i in 0..batch_size {
            unsafe {
                storage.submit_pair_write(
                    &w_bufs[i],
                    current_offset + (i as u64 * stride),
                    i as u64,
                )?;
            }
        }
        wait_all(&mut storage, batch_size);

        // Даем диску "выдохнуть"
        std::thread::sleep(Duration::from_millis(50));

        // 3. ЧТЕНИЕ
        for i in 0..batch_size {
            unsafe {
                storage.submit_pair_read(
                    &mut r_bufs[i],
                    current_offset + (i as u64 * stride),
                    i as u64,
                )?;
            }
        }
        wait_all(&mut storage, batch_size);

        // 4. ДИАГНОСТИКА
        for i in 0..batch_size {
            // Берем DIF для записи и чтения
            let w = w_bufs[i].dif_part_mut();

            // Чтобы не было конфликта заимствования,
            // получаем r_dif и r_data через обычные срезы или по очереди
            let r_dif = *r_bufs[i].dif_part_mut(); // Копируем структуру (она Copy)
            let r_data = r_bufs[i].data_part_mut(); // Теперь это единственное заимствование

            if w.guard_tag != r_dif.guard_tag {
                let all_zeros = r_data.iter().all(|&x| x == 0);
                println!(
                    "❌ ОШИБКА LBA {}: Ожидался CRC {:X}, Получен {:X}. Весь блок в нулях? {}",
                    w.ref_tag, w.guard_tag, r_dif.guard_tag, all_zeros
                );
            } else {
                println!("✅ LBA {} проверен", w.ref_tag);
            }
        }
        current_offset += (batch_size as u64 * stride);
    }
    Ok(())
}

fn wait_all(s: &mut AsyncDifStorage, n: usize) {
    let mut count = 0;
    while count < n {
        count += s.wait_completions().len();
    }
}
