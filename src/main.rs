use bytemuck::{Pod, Zeroable};
use libc::{O_DIRECT, c_void, free, posix_memalign, pread, pwrite};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct T10Dif {
    guard_tag: u16,
    app_tag: u16,
    ref_tag: u32,
}

fn write_dif_to_fd(fd: i32, offset: i64, data: &T10Dif) -> std::io::Result<()> {
    unsafe {
        let mut ptr: *mut libc::c_void = std::ptr::null_mut();
        if libc::posix_memalign(&mut ptr, 4096, 4096) != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Ошибка выделения выровненной памяти",
            ));
        }

        std::ptr::write(ptr as *mut T10Dif, *data);

        let res = libc::pwrite(fd, ptr, 4096, offset);
        libc::free(ptr);

        if res == -1 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "/dev/sdb1";

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(O_DIRECT)
        .open(path)?;

    let fd = file.as_raw_fd();
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let size = 4096; // Размер одного блока метаданных (или кратно 512)

    unsafe {
        if posix_memalign(&mut ptr, 4096, size) != 0 {
            return Err("Ошибка выделения выровненной памяти".into());
        }
    };

    // Тест
    let dif_data = T10Dif {
        guard_tag: 0xAAAA,
        app_tag: 0x01,
        ref_tag: 100,
    };

    unsafe {
        // Копируем структуру в выровненный буфер
        std::ptr::copy_nonoverlapping(&dif_data, ptr as *mut T10Dif, 1);

        // Пишем напрямую на диск, минуя кэш ОС
        let bytes_written = pwrite(fd, ptr, size, 8192);
        if bytes_written == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
    }

    // 4. Чтение обратно
    unsafe {
        let bytes_read = pread(fd, ptr, size, 8192);
        if bytes_read != -1 {
            let read_dif = *(ptr as *const T10Dif);
            println!("Прочитано из Direct IO: {:?}", read_dif);
        }
    }

    // TODO: лучше использовать Box или кастомный аллокатор)
    unsafe { free(ptr) };

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::NamedTempFile;

    #[test]
    fn test_dif_write_read() {
        // Создаем временный файл
        let temp_file = NamedTempFile::new().unwrap();
        let fd = temp_file.as_raw_fd();

        let input = T10Dif {
            guard_tag: 0xDEAD,
            app_tag: 0xbeef,
            ref_tag: 42,
        };

        // Записываем (в тестах можно без O_DIRECT для простоты, либо включить его через fcntl)
        write_dif_to_fd(fd, 0, &input).expect("Write failed");

        // Проверяем результат
        let mut file = temp_file.reopen().unwrap();
        let mut buffer = vec![0u8; 8]; // Размер T10Dif
        file.read_exact(&mut buffer).unwrap();

        let output: T10Dif = *bytemuck::from_bytes(&buffer);

        assert_eq!(input.guard_tag, output.guard_tag);
        assert_eq!(input.ref_tag, output.ref_tag);
    }
}
