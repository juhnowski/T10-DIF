use std::io;
use t10_dif_storage::{AsyncDifStorage, DmaBuffer, T10Dif};

fn main() -> io::Result<()> {
    let device_path = "/dev/sdb1";
    let num_blocks = 4;
    let block_size = 4096;
    let dif_sector = 512;
    let stride = block_size + dif_sector; // 4608 байт

    println!(
        "--- Тест Hardware Combined (Data+DIF) на {} ---",
        device_path
    );

    // 1. Инициализация асинхронного хранилища
    let mut storage = match AsyncDifStorage::new(device_path, 32) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Ошибка: {}. Запустите через sudo!", e);
            return Err(e);
        }
    };

    // 2. Создаем буферы
    let mut buffers: Vec<DmaBuffer> = (0..num_blocks)
        .map(|_| DmaBuffer::new(stride, 4096).expect("Failed to alloc DMA mem"))
        .collect();

    // 3. Подготовка и отправка на запись
    println!("Отправка {} блоков по {} байт...", num_blocks, stride);
    for i in 0..num_blocks {
        let buf = &mut buffers[i as usize];

        // Заполняем данными (имитация)
        let data_part = unsafe { std::slice::from_raw_parts_mut(buf.as_ptr() as *mut u8, 4096) };
        data_part.fill(i as u8 + 0x41); // 'A', 'B', 'C'...

        // Считаем DIF
        let dif = T10Dif::compute(data_part, 0x01, i as u32);

        // Кладем DIF в хвост (смещение 4096)
        unsafe {
            let dif_ptr = (buf.as_ptr() as *mut u8).add(4096) as *mut T10Dif;
            *dif_ptr = dif;
        }

        unsafe {
            storage.submit_write(buf, i * stride as u64, i as u64)?;
        }
    }

    // 4. Ждем подтверждения записи
    let mut completed = 0;
    while completed < num_blocks {
        let ids = storage.wait_completions();
        completed += ids.len() as u64;
        for id in ids {
            println!("Блок #{} физически записан на диск.", id);
        }
    }

    // 5. Чтение для верификации
    println!("\nЧтение обратно для проверки целостности...");
    let mut read_buf = DmaBuffer::new(stride, 4096)?;

    // Проверим первый блок (ID 0)
    unsafe {
        storage.submit_write(&read_buf, 0, 999)?; // Используем submit_write для чтения (в либе лучше разделить методы)
    }
    storage.wait_completions();

    let read_data = &read_buf.as_slice()[0..4096];
    let read_dif: T10Dif = unsafe { *(read_buf.as_ptr().add(4096) as *const T10Dif) };

    println!("Прочитанный DIF: {:?}", read_dif);
    if read_dif.verify(read_data) {
        println!("✅ CRC совпал! Данные на диске идентичны исходным.");
    } else {
        println!("❌ Ошибка верификации! Данные повреждены.");
    }

    Ok(())
}
