use std::io;
use t10_dif_storage::{AsyncDifStorage, DmaBuffer, T10Dif};

fn main() -> std::io::Result<()> {
    let num_blocks = 1024;
    let stride = 4608;
    let mut storage = AsyncDifStorage::new("/dev/sdb1", 128)?;

    // 1. Создаем пачку буферов
    let mut buffers: Vec<DmaBuffer> = (0..num_blocks)
        .map(|_| DmaBuffer::new(stride, 4096).unwrap())
        .collect();

    // Имитируем наполнение данными
    buffers
        .iter_mut()
        .for_each(|b| b.as_mut_slice_len(4096).fill(0xAA));

    println!(
        "--- Начинаем параллельный расчет CRC для {} блоков ---",
        num_blocks
    );

    let start = std::time::Instant::now();

    // 2. РАЙОН: Расчет CRC на всех ядрах
    T10Dif::prepare_batch(&mut buffers, 0, 0x01);

    let duration = start.elapsed();
    println!("Расчет завершен за {:?}", duration);

    // 3. Отправка в io_uring
    for i in 0..num_blocks {
        unsafe {
            storage.submit_write(&buffers[i], i as u64 * stride as u64, i as u64)?;
        }
    }

    // 4. Ожидание завершения
    let mut completed = 0;
    while completed < num_blocks {
        completed += storage.wait_completions().len();
    }

    println!("✅ Все {} блоков защищены и записаны!", num_blocks);
    Ok(())
}
