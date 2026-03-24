use std::fs::File;
use t10_dif_storage::{DifStorage, DmaBuffer, T10Dif};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "storage_demo.dif";

    // 1. Подготавливаем файл (Direct IO требует предсозданный файл нужного размера)
    {
        let f = File::create(path)?;
        f.set_len(4096)?;
        println!("--- Файл метаданных создан: {} ---", path);
    }

    // 2. Открываем наше хранилище
    let storage = DifStorage::open(path)?;

    // 3. Создаем выровненный буфер (DMA) для метаданных
    // Вмещает 512 записей по 8 байт = 4096 байт
    let mut buffer = DmaBuffer::new(4096, 4096)?;

    // 4. Имитируем данные, которые мы хотим защитить
    let fake_data_block = [0u8; 4096]; // Блок данных, который пойдет на основной диск
    let lba = 0x12345;

    println!("Генерируем DIF для LBA: {:X}", lba);

    {
        let entries = buffer.as_dif_mut();
        // Рассчитываем CRC16-T10 для первого блока в буфере
        entries[0] = T10Dif::compute(&fake_data_block, 0x01, lba as u32);
        println!("Guard Tag (CRC): {:04X}", entries[0].guard_tag);
    }

    // 5. Записываем метаданные на диск через O_DIRECT
    storage.write_block(0, &buffer)?;
    println!("Запись DIF на диск завершена успешно.");

    // 6. Читаем обратно для проверки
    let mut read_buffer = DmaBuffer::new(4096, 4096)?;
    storage.read_block(0, &mut read_buffer)?;

    let read_entry = read_buffer.as_dif_mut()[0];
    println!("Прочитано из файла: {:?}", read_entry);

    // 7. Верификация
    if read_entry.verify(&fake_data_block) {
        println!("✅ Верификация пройдена: данные и метаданные совпадают!");
    } else {
        println!("❌ ОШИБКА: Контрольная сумма не совпала!");
    }

    Ok(())
}
