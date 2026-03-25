use std::fs::File;
use t10_dif_storage::{AsyncDifStorage, DmaBuffer, T10Dif};

fn main() -> std::io::Result<()> {
    let path = "combined.dif";
    let num_requests = 4;
    let block_size_with_dif = 5120; // 4096 (data) + 1024 (DmaBuffer size для DIF)

    // 1. Предварительно создаем файл и задаем ему размер
    {
        let f = File::create(path)?;
        f.set_len(num_requests as u64 * block_size_with_dif)?;
        println!("--- Файл {} подготовлен ---", path);
    }

    let mut storage = AsyncDifStorage::new("combined.dif", 32)?;
    let num_requests = 4;

    let mut data_vec: Vec<DmaBuffer> = (0..num_requests)
        .map(|_| DmaBuffer::new(4096, 4096).unwrap())
        .collect();
    let mut dif_vec: Vec<DmaBuffer> = (0..num_requests)
        .map(|_| DmaBuffer::new(4096, 4096).unwrap())
        .collect();

    for i in 0..num_requests {
        // Теперь as_slice() работает!
        let dif_entry = T10Dif::compute(data_vec[i].as_slice(), 0x1, i as u32);

        // Кладём запись в буфер DIF
        dif_vec[i].as_dif_mut()[0] = dif_entry;

        unsafe {
            // Пишем данные + DIF атомарно по смещению (например, 5КБ на каждый блок)
            storage.submit_gather_write(&data_vec[i], &dif_vec[i], i as u64 * 5120, i as u64)?;
        }
    }

    let mut count = 0;
    while count < num_requests {
        let finished = storage.wait_completions();
        count += finished.len();
        for id in finished {
            println!("Запрос #{} (Data + DIF) успешно завершен", id);
        }
    }

    Ok(())
}
