use bytemuck::{Pod, Zeroable};
use memmap2::MmapMut;
use std::fs::OpenOptions;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct T10Dif {
    guard_tag: u16, // CRC16
    app_tag: u16,   // Application Tag
    ref_tag: u32,   // Reference Tag (LBA)
}

fn main() -> std::io::Result<()> {
    let path = "metadata.dif";
    let block_count = 1024; // TODO: вынести в конфиг
    let file_size = block_count * std::mem::size_of::<T10Dif>();

    // Открываем файл метаданных, для этого нам нужно иметь свою ФС
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;
    file.set_len(file_size as u64)?;

    let mut mmap = unsafe { MmapMut::map_mut(&file)? };
    let dif_entries = bytemuck::cast_slice_mut::<u8, T10Dif>(&mut mmap);

    // Тест: редактируем метаданные для 5-го блока
    dif_entries[4] = T10Dif {
        guard_tag: 0xABCD,
        app_tag: 0x0001,
        ref_tag: 4, // LBA
    };

    // Читаем
    println!("Guard Tag блока 4: {:X}", dif_entries[4].guard_tag);

    mmap.flush()?;
    Ok(())
}
