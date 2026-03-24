use bytemuck::{Pod, Zeroable};
use crc::{Algorithm, Crc};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

/// Спецификация CRC-16 T10-DIF
/// Полином: 0x8BB7, Начальное значение: 0x0000, Конечный XOR: 0x0000
const T10_DIF_CRC: Algorithm<u16> = Algorithm {
    width: 16,
    poly: 0x8BB7,
    init: 0x0000,
    refin: false,
    refout: false,
    xorout: 0x0000,
    check: 0x45a3, // Контрольное значение для строки "123456789"
    residue: 0x0000,
};

const CRC_CALC: Crc<u16> = Crc::<u16>::new(&T10_DIF_CRC);

impl T10Dif {
    /// Создает новую структуру DIF и автоматически вычисляет Guard Tag (CRC)
    /// data_block — это массив 4096 КБ
    pub fn compute(data_block: &[u8], app_tag: u16, ref_tag: u32) -> Self {
        let guard_tag = CRC_CALC.checksum(data_block);

        Self {
            guard_tag,
            app_tag,
            ref_tag,
        }
    }

    /// Проверяет, соответствует ли Guard Tag переданным данным
    pub fn verify(&self, data_block: &[u8]) -> bool {
        let expected_crc = CRC_CALC.checksum(data_block);
        self.guard_tag == expected_crc
    }
}

/// Структура метаданных T10-DIF (8 байт)
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct T10Dif {
    pub guard_tag: u16,
    pub app_tag: u16,
    pub ref_tag: u32,
}

/// Умный указатель для выровненной памяти (DMA-friendly)
pub struct DmaBuffer {
    ptr: *mut libc::c_void,
    size: usize,
}

impl DmaBuffer {
    pub fn new(size: usize, align: usize) -> io::Result<Self> {
        let mut ptr: *mut libc::c_void = std::ptr::null_mut();
        unsafe {
            if libc::posix_memalign(&mut ptr, align, size) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self { ptr, size })
    }

    /// Предоставляет доступ к памяти как к срезу структур DIF
    pub fn as_dif_mut(&mut self) -> &mut [T10Dif] {
        let count = self.size / std::mem::size_of::<T10Dif>();
        unsafe { std::slice::from_raw_parts_mut(self.ptr as *mut T10Dif, count) }
    }

    pub fn as_ptr(&self) -> *mut libc::c_void {
        self.ptr
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        unsafe { libc::free(self.ptr) };
    }
}

/// Основной обработчик хранилища DIF
pub struct DifStorage {
    file: File,
}

impl DifStorage {
    /// Открывает устройство или файл с поддержкой Direct IO
    pub fn open(path: &str) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_DIRECT)
            .open(path)?;
        Ok(Self { file })
    }

    /// Записывает блок метаданных (размер buffer должен быть кратен 512/4096)
    pub fn write_block(&self, offset: u64, buffer: &DmaBuffer) -> io::Result<()> {
        let res = unsafe {
            libc::pwrite(
                self.file.as_raw_fd(),
                buffer.as_ptr(),
                buffer.size,
                offset as i64,
            )
        };
        if res == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Читает блок метаданных
    pub fn read_block(&self, offset: u64, buffer: &mut DmaBuffer) -> io::Result<()> {
        let res = unsafe {
            libc::pread(
                self.file.as_raw_fd(),
                buffer.as_ptr(),
                buffer.size,
                offset as i64,
            )
        };
        if res == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}
