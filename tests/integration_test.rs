#[cfg(test)]
mod tests {
    use t10_dif_storage::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_write_read_cycle() {
        // Создаем временный файл для теста (имитируем диск)
        let temp = NamedTempFile::new().expect("Failed to create temp file");
        let path = temp.path().to_str().unwrap();

        // Подготавливаем размер файла (O_DIRECT требует, чтобы файл не был пустым)
        temp.as_file().set_len(4096 * 2).unwrap();

        let storage = DifStorage::open(path).expect("Failed to open storage");
        let mut buffer = DmaBuffer::new(4096, 4096).unwrap();

        // Заполняем данными
        let entries = buffer.as_dif_mut();
        entries[0] = T10Dif {
            guard_tag: 0xAAAA,
            app_tag: 1,
            ref_tag: 100,
        };
        entries[1] = T10Dif {
            guard_tag: 0xBBBB,
            app_tag: 2,
            ref_tag: 101,
        };

        // Пишем и читаем
        storage.write_block(0, &buffer).expect("Write fail");

        let mut read_buffer = DmaBuffer::new(4096, 4096).unwrap();
        storage.read_block(0, &mut read_buffer).expect("Read fail");

        assert_eq!(read_buffer.as_dif_mut()[0].guard_tag, 0xAAAA);
        assert_eq!(read_buffer.as_dif_mut()[1].ref_tag, 101);
    }

    #[test]
    fn example_usage() {
        // Тестовый блок данных СХД (4КБ)
        let data_block = [0u8; 4096];
        let lba = 1024; // Logical Block Address

        // Генерируем метаданные DIF
        let dif_entry = T10Dif::compute(&data_block, 0x0001, lba);
        println!("Сгенерированный CRC: {:04X}", dif_entry.guard_tag);

        // Проверяем целостность данных
        assert!(dif_entry.verify(&data_block));
    }
}
