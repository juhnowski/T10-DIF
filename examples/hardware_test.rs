use std::io;
use t10_dif_storage::{DifStorage, DmaBuffer, T10Dif};

fn main() -> io::Result<()> {
    // ВАЖНО: Убедись, что /dev/sdb1 существует и у тебя есть права (sudo)
    // ОСТОРОЖНО: Этот код ПЕРЕЗАПИШЕТ данные по указанным смещениям!
    let device_path = "/dev/sdb1";

    println!("--- Тестирование на реальном железе: {} ---", device_path);

    // 1. Открываем устройство. Ошибка на этом этапе обычно означает Lack of Permissions.
    let storage = match DifStorage::open(device_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Ошибка открытия {}: {}. Попробуйте запустить через sudo.",
                device_path, e
            );
            return Err(e);
        }
    };

    // 2. Создаем DMA-буфер.
    // Для реальных дисков выравнивание по 4096 байт — стандарт индустрии (Advanced Format).
    let block_size = 4096;
    let mut write_buf = DmaBuffer::new(block_size, 4096)?;

    // 3. Подготовка данных для записи
    let dummy_data = [0x55u8; 4096]; // Имитация блока данных 4КБ
    let target_lba: u64 = 0; // Пишем в самое начало раздела (LBA 0)

    {
        let entries = write_buf.as_dif_mut();
        // Генерируем DIF для первого блока в буфере
        entries[0] = T10Dif::compute(&dummy_data, 0x99, target_lba as u32);
        println!("Сгенерирован DIF: {:?}", entries[0]);
    }

    // 4. Запись на физический диск
    // Смещение должно быть кратно размеру сектора (обычно 512 или 4096)
    println!("Запись DIF метаданных по смещению 0...");
    storage.write_block(0, &write_buf)?;

    // 5. Чтение обратно с диска
    println!("Чтение DIF метаданных обратно...");
    let mut read_buf = DmaBuffer::new(block_size, 4096)?;
    storage.read_block(0, &mut read_buf)?;

    let read_entry = &read_buf.as_dif_mut()[0];
    println!("Прочитано с диска: {:?}", read_entry);

    // 6. Финальная проверка целостности
    if read_entry.verify(&dummy_data) && read_entry.app_tag == 0x99 {
        println!("✅ ПРОВЕРКА ПРОЙДЕНА: Железо корректно сохранило DIF-метку.");
    } else {
        println!("❌ ОШИБКА: Данные на диске повреждены или не совпадают!");
    }

    Ok(())
}
