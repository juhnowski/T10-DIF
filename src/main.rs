use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::{Duration, Instant};
use t10_dif_storage::{AsyncDifStorage, DmaBuffer, T10Dif};

#[derive(Parser, Debug)]
#[command(author, version, about = "T10-DIF Hardware Integrity Bench")]
struct Args {
    /// Путь к блочному устройству (напр. /dev/sdb1)
    #[arg(short, long)]
    device: String,

    /// Объем данных для проверки в ГБ
    #[arg(short, long, default_value_t = 1)]
    size_gb: u64,

    /// Глубина очереди (Queue Depth) для io_uring
    #[arg(short, long, default_value_t = 32)]
    qd: u32,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let stride = 8192; // 4K Data + 4K DIF
    let total_blocks = (args.size_gb * 1024 * 1024 * 1024) / 4096;
    let batch_size = args.qd as usize;

    let mut storage = AsyncDifStorage::new(&args.device, args.qd)
        .expect("❌ Ошибка открытия устройства. Проверьте путь и права (sudo).");

    let pb = ProgressBar::new(total_blocks);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} blocks ({msg})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    let mut w_bufs: Vec<DmaBuffer> = (0..batch_size)
        .map(|_| DmaBuffer::new_aligned_pair().unwrap())
        .collect();
    let mut r_bufs: Vec<DmaBuffer> = (0..batch_size)
        .map(|_| DmaBuffer::new_aligned_pair().unwrap())
        .collect();

    let mut offset = 0u64;
    let mut errors = 0u64;
    let start_time = Instant::now();

    println!(
        "🚀 Начинаем проверку {} ({} ГБ, QD={})",
        args.device, args.size_gb, args.qd
    );

    for _ in 0..(total_blocks / batch_size as u64) {
        let lba_base = offset / 4096;

        // 1. Подготовка пачки (Rayon внутри библиотеки)
        w_bufs.iter_mut().enumerate().for_each(|(i, b)| {
            b.data_part_mut().fill(0xAA);
            *b.dif_part_mut() =
                T10Dif::compute(b.data_part_mut(), 0x01, (lba_base + (i * 2) as u64) as u32);
        });

        // 2. Асинхронная ЗАПИСЬ
        for i in 0..batch_size {
            unsafe {
                storage.submit_pair_write(&w_bufs[i], offset + (i as u64 * stride), i as u64)?;
            }
        }
        wait_all(&mut storage, batch_size);

        // 3. Асинхронное ЧТЕНИЕ
        for i in 0..batch_size {
            unsafe {
                storage.submit_pair_read(&mut r_bufs[i], offset + (i as u64 * stride), i as u64)?;
            }
        }
        wait_all(&mut storage, batch_size);

        // 4. ВЕРИФИКАЦИЯ
        for i in 0..batch_size {
            let w = w_bufs[i].dif_part_mut();
            let r = r_bufs[i].dif_part_mut();
            if w.guard_tag != r.guard_tag || w.ref_tag != r.ref_tag {
                errors += 1;
            }
        }

        offset += (batch_size as u64 * stride);
        pb.inc(batch_size as u64);

        let mb_ps = (offset as f64 / 1024.0 / 1024.0) / start_time.elapsed().as_secs_f64();
        pb.set_message(format!("{:.2} MB/s", mb_ps));
    }

    pb.finish_with_message("Завершено");
    println!("\n--- Итоги теста ---");
    println!("⏱ Время: {:?}", start_time.elapsed());
    println!("❌ Найдено повреждений данных (DIF errors): {}", errors);

    if errors == 0 {
        println!("✅ Диск надежен. Silent Data Corruption не обнаружен.");
    } else {
        println!(
            "🚨 ВНИМАНИЕ: Обнаружены ошибки целостности! Использовать этот диск для СХД опасно."
        );
    }

    Ok(())
}

fn wait_all(s: &mut AsyncDifStorage, n: usize) {
    let mut count = 0;
    while count < n {
        count += s.wait_completions().len();
    }
}
