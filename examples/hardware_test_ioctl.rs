use t10_dif_storage::{DifStorage, DmaBuffer, T10Dif};

fn main() -> std::io::Result<()> {
    let device_path = "/dev/sdb1";
    let storage = DifStorage::open(device_path)?;

    // 1. Узнаем реальный размер сектора диска
    let sector_size = storage.get_sector_size()?;
    println!("Логический сектор диска: {} байт", sector_size);

    // 2. Создаем буфер, выровненный ИМЕННО по этому размеру
    // O_DIRECT будет работать максимально эффективно
    let mut buffer = DmaBuffer::new(sector_size, sector_size)?;

    // 3. Работаем с данными
    let data_to_protect = vec![0u8; sector_size];
    {
        let entries = buffer.as_dif_mut();
        // Заполняем первую запись DIF в буфере
        entries[0] = T10Dif::compute(&data_to_protect, 0x01, 0);
    }

    // 4. Записываем точно в начало сектора
    storage.write_block(0, &buffer)?;

    println!("✅ Записано с учетом выравнивания секторов!");
    Ok(())
}
