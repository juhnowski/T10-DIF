use std::fs::File;
use t10_dif_storage::{AsyncDifStorage, DmaBuffer, T10Dif};

fn main() -> std::io::Result<()> {
    let path = "interleaved_storage.dif";
    let num_blocks = 4;
    let stride = 4608; // 4096 + 512

    // Подготовка файла
    {
        let f = std::fs::File::create(path)?;
        f.set_len(num_blocks * stride)?;
    }

    let mut storage = AsyncDifStorage::new(path, 32)?;
    let mut buffers: Vec<DmaBuffer> = (0..num_blocks)
        .map(|_| DmaBuffer::new_combined().unwrap())
        .collect();

    for i in 0..num_blocks {
        let buf = &mut buffers[i as usize];

        // 1. Заполняем данные (имитация)
        buf.data_part_mut().fill(i as u8);

        // 2. Считаем DIF для этих данных
        let dif = T10Dif::compute(buf.data_part_mut(), 0x01, i as u32);

        // 3. Кладем DIF в "хвост" буфера
        *buf.dif_part_mut() = dif;

        unsafe {
            storage.submit_combined_write(buf, i * stride, i)?;
        }
    }

    // Ожидание завершения...
    let mut count = 0;
    while count < num_blocks {
        let done = storage.wait_completions();
        count += done.len() as u64;
    }
    println!("✅ Все блоки (Data+DIF) записаны успешно!");

    Ok(())
}
