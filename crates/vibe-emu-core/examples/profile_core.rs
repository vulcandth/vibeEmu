//! Deterministic, core-only workload for wall-clock benchmarks and sampling profilers.
use std::{env, error::Error, fs, time::Instant};
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 || args.len() > 5 {
        return Err("usage: profile_core ROM [frames=1800] [warmup=300] [audio|silent]".into());
    }
    let frames: u64 = args.get(2).map_or(Ok(1800), |s| s.parse())?;
    let warmup: u64 = args.get(3).map_or(Ok(300), |s| s.parse())?;
    if frames == 0 {
        return Err("frames must be positive".into());
    }
    let audio = match args.get(4).map(String::as_str).unwrap_or("audio") {
        "audio" => true,
        "silent" => false,
        _ => return Err("output must be audio or silent".into()),
    };
    let rom = fs::read(&args[1])?;
    let model = if rom.get(0x143).is_some_and(|flag| flag & 0x80 != 0) {
        Model::Cgb(Default::default())
    } else {
        Model::Dmg(Default::default())
    };
    let mut gb = GameBoy::new(model);
    // Loading bytes avoids save files and wall-clock RTC synchronization.
    gb.mmu.load_cart(Cartridge::from_bytes(rom));
    let consumer = audio.then(|| gb.mmu.apu.enable_output(48_000));
    let mut video_hash = 0xcbf29ce484222325u64;
    let mut audio_hash = video_hash;
    let mut samples = 0u64;
    let mut elapsed = std::time::Duration::ZERO;
    let mut dots = 0;
    for frame in 0..warmup + frames {
        let start_dots = gb.cpu.cycles;
        let start = Instant::now();
        // Fixed dot budgets also work with the LCD disabled. Carry instruction
        // overshoot forward so every run executes the same amount of hardware time.
        let target = (frame + 1) * 70_224;
        while gb.cpu.cycles < target {
            gb.cpu
                .run_for_dots(&mut gb.mmu, (target - gb.cpu.cycles).min(4096) as u16);
        }
        if frame >= warmup {
            elapsed += start.elapsed();
            dots += gb.cpu.cycles - start_dots;
            for &pixel in gb.mmu.ppu.framebuffer() {
                video_hash = (video_hash ^ u64::from(pixel)).wrapping_mul(0x100000001b3);
            }
        }
        if let Some(consumer) = &consumer {
            while let Some((left, right)) = consumer.pop_stereo() {
                if frame >= warmup {
                    let packed = u32::from(left as u16) | (u32::from(right as u16) << 16);
                    audio_hash = (audio_hash ^ u64::from(packed)).wrapping_mul(0x100000001b3);
                    samples += 1;
                }
            }
        }
    }
    let seconds = elapsed.as_secs_f64();
    println!(
        "model={model:?} frames={frames} warmup={warmup} audio={audio} seconds={seconds:.6} fps={:.2} realtime={:.2}x dots={dots} video={video_hash:016x} audio_hash={audio_hash:016x} samples={samples} pc={:04x}",
        frames as f64 / seconds,
        dots as f64 / 4_194_304.0 / seconds,
        gb.cpu.pc,
    );
    Ok(())
}
