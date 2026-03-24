use t10_dif_storage::{AsyncDifStorage, DmaBuffer, T10Dif};

fn main() -> std::io::Result<()> {
    let mut storage = AsyncDifStorage::new("metadata.dif", 32)?;

    // Создаем пул буферов (в реальности лучше использовать массив или Vec)
    let mut buffers: Vec<DmaBuffer> = (0..4)
        .map(|_| DmaBuffer::new(4096, 4096).unwrap())
        .collect();

    println!("Отправка 4-х асинхронных запросов DIF...");

    for i in 0..4 {
        let offset = i as u64 * 4096;
        // Заполняем данными перед отправкой
        {
            let entries = buffers[i].as_dif_mut();
            entries[0] = T10Dif::compute(&[0u8; 4096], 0x1, i as u32);
        }

        unsafe {
            storage.submit_write(&buffers[i], offset, i as u64)?;
        }
    }

    // Ждем подтверждения от диска
    let finished = storage.wait_completions();
    for id in finished {
        println!("Запрос #{} успешно записан на диск через io_uring", id);
    }

    Ok(())
}
