use bytemuck::{Pod, Zeroable};
use crc::{Algorithm, Crc};
use io_uring::{IoUring, opcode, types};
use libc::ioctl;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

// Магическое число для получения размера логического сектора в Linux
const BLKSSZGET: u64 = 0x1204;

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

    /// Предоставляет доступ к памяти как к срезу байтов (нужно для CRC)
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr as *const u8, self.size) }
    }

    /// Создает буфер для блока 4096 данных + 512 метаданных (итого 4608)
    pub fn new_combined() -> io::Result<Self> {
        Self::new(4096 + 512, 4096)
    }

    /// Срез для области данных (первые 4096 байт)
    pub fn data_part_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr as *mut u8, 4096) }
    }

    /// Ссылка на структуру DIF (начинается сразу после 4096 байт)
    pub fn dif_part_mut(&mut self) -> &mut T10Dif {
        unsafe {
            let dif_ptr = (self.ptr as *mut u8).add(4096) as *mut T10Dif;
            &mut *dif_ptr
        }
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
    /// Определяет размер логического сектора устройства (обычно 512 или 4096)
    pub fn get_sector_size(&self) -> io::Result<usize> {
        let mut sector_size: i32 = 0;
        let fd = self.file.as_raw_fd();

        unsafe {
            if ioctl(fd, BLKSSZGET, &mut sector_size) == -1 {
                // Если это не блочное устройство (а обычный файл),
                // возвращаем стандартные 4096 для совместимости
                return Ok(4096);
            }
        }

        if sector_size <= 0 {
            return Ok(4096);
        }

        Ok(sector_size as usize)
    }

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

pub struct AsyncDifStorage {
    file: std::fs::File,
    ring: IoUring,
}

impl AsyncDifStorage {
    pub fn new(path: &str, queue_depth: u32) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_DIRECT)
            .open(path)?;

        // Создаем ринг с очередью глубиной queue_depth (например, 32)
        let ring = IoUring::new(queue_depth)?;
        Ok(Self { file, ring })
    }

    /// Отправляет запрос на запись DIF в очередь (не блокирует)
    /// user_data — ID запроса, чтобы потом сопоставить результат
    pub unsafe fn submit_write(
        &mut self,
        buffer: &DmaBuffer,
        offset: u64,
        user_data: u64,
    ) -> std::io::Result<()> {
        let write_e = opcode::Write::new(
            types::Fd(self.file.as_raw_fd()),
            buffer.as_ptr() as *const _,
            buffer.size as u32,
        )
        .offset(offset)
        .build()
        .user_data(user_data);

        // В новых версиях Rust нужно явно писать unsafe блок внутри unsafe fn
        unsafe {
            self.ring
                .submission()
                .push(&write_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ Full"))?;
        }

        self.ring.submit()?;
        Ok(())
    }

    /// Gather-запись: записывает данные из нескольких буферов в одно место на диске
    pub unsafe fn submit_gather_write(
        &mut self,
        data_buf: &DmaBuffer,
        dif_buf: &DmaBuffer,
        offset: u64,
        user_data: u64,
    ) -> std::io::Result<()> {
        let iov = [
            libc::iovec {
                iov_base: data_buf.as_ptr(),
                iov_len: data_buf.size,
            },
            libc::iovec {
                iov_base: dif_buf.as_ptr(),
                iov_len: 8, // Только структура T10Dif
            },
        ];

        let writev_e = opcode::Writev::new(
            types::Fd(self.file.as_raw_fd()),
            iov.as_ptr() as *const _,
            2,
        )
        .offset(offset)
        .build()
        .user_data(user_data);

        unsafe {
            self.ring
                .submission()
                .push(&writev_e)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SQ Full"))?;
        }

        self.ring.submit()?;
        Ok(())
    }

    /// Собирает завершенные операции (блокирует до появления хотя бы одной)
    pub fn wait_completions(&mut self) -> Vec<u64> {
        let mut completed_ids = Vec::new();
        self.ring.submit_and_wait(1).unwrap();

        let mut cq = self.ring.completion();
        while let Some(cqe) = cq.next() {
            if cqe.result() >= 0 {
                completed_ids.push(cqe.user_data());
            } else {
                eprintln!(
                    "Ошибка IO: {}",
                    std::io::Error::from_raw_os_error(-cqe.result())
                );
            }
        }
        completed_ids
    }

    pub unsafe fn submit_combined_write(
        &mut self,
        buffer: &DmaBuffer,
        offset: u64,
        user_data: u64,
    ) -> std::io::Result<()> {
        // Пишем сразу 4608 байт
        let write_e = opcode::Write::new(
            types::Fd(self.file.as_raw_fd()),
            buffer.as_ptr() as *const _,
            4608,
        )
        .offset(offset)
        .build()
        .user_data(user_data);

        unsafe {
            self.ring
                .submission()
                .push(&write_e)
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "SQ Full"))?;
        }

        self.ring.submit()?;
        Ok(())
    }
}
