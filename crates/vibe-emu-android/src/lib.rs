#![allow(non_snake_case)]

#[cfg(test)]
mod tests;

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use jni::{
    EnvUnowned,
    objects::{JByteArray, JClass, JIntArray, JShortArray, JString},
    sys::{JNI_FALSE, JNI_TRUE, jboolean, jint, jlong},
};
use vibe_emu_core::{
    audio_queue::AudioConsumer, cartridge::Cartridge, gameboy::GameBoy, hardware::Model,
    serial::NullLinkPort,
};
use vibe_emu_mobile::{MobileAdapter, MobileLinkPort};

const FB_WIDTH: usize = 160;
const FB_HEIGHT: usize = 144;
const FB_PIXELS: usize = FB_WIDTH * FB_HEIGHT;

const NEUTRAL_DMG_PALETTE: [u32; 4] = [0x00E0F8D0, 0x0088C070, 0x00346856, 0x00081820];
const DEFAULT_DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

#[repr(i32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum EmulationMode {
    Auto = 0,
    ForceDmg = 1,
    ForceCgb = 2,
    ForceMgb = 3,
    ForceSgb = 4,
    ForceSgb2 = 5,
    ForceAgb0 = 6,
    ForceAgb = 7,
    CgbSgb = 8,
    CgbInitialBorder = 9,
    AutoPreferCgb = 10,
    AutoPreferSgb = 11,
}

impl EmulationMode {
    fn from_jint(value: jint) -> Self {
        match value {
            1 => Self::ForceDmg,
            2 => Self::ForceCgb,
            3 => Self::ForceMgb,
            4 => Self::ForceSgb,
            5 => Self::ForceSgb2,
            6 => Self::ForceAgb0,
            7 => Self::ForceAgb,
            8 => Self::CgbSgb,
            9 => Self::CgbInitialBorder,
            10 => Self::AutoPreferCgb,
            11 => Self::AutoPreferSgb,
            _ => Self::Auto,
        }
    }

    fn model(self, rom: &[u8]) -> Model {
        let cgb = rom_prefers_cgb(rom);
        let sgb = rom.get(0x146) == Some(&3) && rom.get(0x14b) == Some(&0x33);
        match self {
            Self::ForceDmg => Model::from_cgb_flag(false),
            Self::ForceCgb | Self::CgbSgb | Self::CgbInitialBorder => Model::from_cgb_flag(true),
            Self::ForceMgb => Model::Mgb,
            Self::ForceSgb => Model::Sgb,
            Self::ForceSgb2 => Model::Sgb2,
            Self::ForceAgb0 => Model::Agb0,
            Self::ForceAgb => Model::Agb,
            Self::AutoPreferSgb if sgb => Model::Sgb,
            Self::AutoPreferCgb if sgb && !cgb => Model::Sgb,
            _ => Model::from_cgb_flag(cgb),
        }
    }
}

struct EmulatorHandle {
    gb: GameBoy,
    frame: Vec<u32>,
    argb: Vec<i32>,
    mobile: Option<Arc<Mutex<MobileAdapter>>>,
    emulation_mode: EmulationMode,
    dmg_neutral_palette: bool,
    bootroms: [Option<Vec<u8>>; 7],
    show_border: bool,
    audio: AudioConsumer,
}

impl EmulatorHandle {
    fn new(emulation_mode: EmulationMode) -> Self {
        let mut gb = GameBoy::new(emulation_mode.model(&[]));
        let audio = gb.mmu.apu.enable_output(44_100);
        Self {
            gb,
            frame: vec![0; FB_PIXELS],
            argb: vec![0; FB_PIXELS],
            mobile: None,
            emulation_mode,
            dmg_neutral_palette: false,
            bootroms: std::array::from_fn(|_| None),
            show_border: true,
            audio,
        }
    }

    fn attach_serial_and_palette(&mut self, cgb: bool) {
        if let Some(adapter) = &self.mobile {
            self.gb
                .mmu
                .serial
                .connect(Box::new(MobileLinkPort::new(adapter.clone())));
        } else {
            self.gb
                .mmu
                .serial
                .connect(Box::new(NullLinkPort::default()));
        }

        if !cgb {
            self.gb
                .mmu
                .ppu
                .set_dmg_palette(if self.dmg_neutral_palette {
                    NEUTRAL_DMG_PALETTE
                } else {
                    DEFAULT_DMG_PALETTE
                });
        }
    }

    fn enable_mobile_adapter(&mut self, config_path: PathBuf) -> bool {
        let adapter = match MobileAdapter::new_std(config_path) {
            Ok(adapter) => adapter,
            Err(_) => return false,
        };

        let adapter = Arc::new(Mutex::new(adapter));
        {
            let mut locked = match adapter.lock() {
                Ok(locked) => locked,
                Err(_) => return false,
            };
            if locked.start().is_err() {
                return false;
            }
        }

        self.gb
            .mmu
            .serial
            .connect(Box::new(MobileLinkPort::new(adapter.clone())));
        self.mobile = Some(adapter);
        true
    }

    fn disable_mobile_adapter(&mut self) {
        if let Some(adapter) = self.mobile.take()
            && let Ok(mut locked) = adapter.lock()
        {
            let _ = locked.stop();
        }

        self.gb
            .mmu
            .serial
            .connect(Box::new(NullLinkPort::default()));
    }

    fn load_cart(&mut self, cart: Cartridge) -> bool {
        self.save();
        let model = self.emulation_mode.model(&cart.rom);
        let boot = self.bootroms[bootrom_index(model)].clone();
        self.gb = if boot.is_some() {
            GameBoy::new_power_on(model)
        } else {
            GameBoy::new(model)
        };
        if let Some(boot) = boot {
            self.gb.mmu.load_boot_rom(boot);
        }
        if self.emulation_mode == EmulationMode::CgbSgb {
            self.gb.enable_sgb_extensions();
        }
        // The facade applies MGB/AGB header-dependent boot registers.
        self.gb.load_cart(cart);
        if self.emulation_mode == EmulationMode::CgbInitialBorder {
            self.gb
                .borrow_sgb_border(self.bootroms[bootrom_index(Model::Sgb)].as_deref(), 600);
        }
        self.audio = self.gb.mmu.apu.enable_output(44_100);
        self.attach_serial_and_palette(model.is_cgb());
        self.copy_frame();
        true
    }

    fn load_rom(&mut self, rom: Vec<u8>) -> bool {
        self.load_cart(Cartridge::from_bytes(rom))
    }

    fn load_rom_from_file(&mut self, path: PathBuf) -> bool {
        // Flush before reading when reloading the same save path.
        self.save();
        match Cartridge::from_file(&path) {
            Ok(cart) => self.load_cart(cart),
            Err(_) => false,
        }
    }

    fn reset(&mut self) {
        if self.gb.mmu.boot_rom.is_some() {
            self.gb.reset_power_on();
            self.gb.cpu.pc = 0x0000;
        } else {
            self.gb.reset();
        }

        self.audio = self.gb.mmu.apu.enable_output(44_100);
        self.attach_serial_and_palette(self.gb.model.is_cgb());
        self.copy_frame();
    }

    fn set_dmg_neutral_palette(&mut self, enabled: bool) {
        self.dmg_neutral_palette = enabled;

        if !self.gb.model.is_cgb() {
            self.gb.mmu.ppu.set_dmg_palette(if enabled {
                NEUTRAL_DMG_PALETTE
            } else {
                DEFAULT_DMG_PALETTE
            });
        }
    }

    fn set_boot_rom(&mut self, mode: usize, mut data: Vec<u8>) -> bool {
        let Some(slot) = self.bootroms.get_mut(mode) else {
            return false;
        };
        let color = matches!(mode, 1 | 5 | 6);
        if !(if color {
            matches!(data.len(), 0x800 | 0x900)
        } else {
            data.len() == 0x100
        }) {
            return false;
        }
        if color && data.len() == 0x800 {
            data.splice(0x100..0x100, [0; 0x100]);
        }
        *slot = Some(data);
        true
    }

    fn clear_boot_rom(&mut self, mode: usize) {
        if let Some(slot) = self.bootroms.get_mut(mode) {
            *slot = None;
        }
    }

    fn set_player_input(&mut self, player: usize, state: u8) {
        let mmu = &mut self.gb.mmu;
        mmu.input
            .update_player_state(player, state, &mut mmu.if_reg);
    }

    fn dimensions(&self) -> (usize, usize) {
        if self.show_border {
            self.gb.mmu.ppu.display_dimensions()
        } else {
            (FB_WIDTH, FB_HEIGHT)
        }
    }

    fn copy_frame(&mut self) {
        let fb = self.gb.mmu.ppu.display_framebuffer();
        self.frame.clear();
        if !self.show_border && self.gb.mmu.ppu.display_dimensions() == (256, 224) {
            for row in fb.as_chunks::<256>().0.iter().skip(40).take(144) {
                self.frame.extend_from_slice(&row[48..208]);
            }
        } else {
            self.frame.extend_from_slice(fb);
        }
        self.argb.resize(self.frame.len(), 0);
    }

    fn set_input(&mut self, state: u8) {
        let mmu = &mut self.gb.mmu;
        mmu.input.update_state(state, &mut mmu.if_reg);
    }

    fn run_frame(&mut self) -> bool {
        while !self.gb.mmu.ppu.frame_ready() {
            let before = self.gb.cpu.cycles;
            self.gb.cpu.run_for_dots(&mut self.gb.mmu, 4096);
            if self.gb.cpu.faulted || self.gb.cpu.cycles == before {
                return false;
            }
        }

        if let Some(adapter) = &self.mobile
            && let Ok(mut locked) = adapter.lock()
        {
            let _ = locked.poll(17);
        }

        self.copy_frame();
        self.gb.mmu.ppu.clear_frame_flag();
        true
    }

    fn save(&mut self) {
        self.gb.mmu.save_cart_ram();
    }
}

// JNI boot IDs preserve the original DMG=0 and CGB=1 values.
fn bootrom_index(model: Model) -> usize {
    match model {
        Model::Dmg(_) => 0,
        Model::Cgb(_) => 1,
        Model::Mgb => 2,
        Model::Sgb => 3,
        Model::Sgb2 => 4,
        Model::Agb0 => 5,
        Model::Agb => 6,
    }
}

fn rom_prefers_cgb(rom: &[u8]) -> bool {
    if rom.len() <= 0x143 {
        return false;
    }
    matches!(rom[0x143], 0x80 | 0xC0)
}

unsafe fn handle_from_jlong<'a>(handle: jlong) -> Option<&'a mut EmulatorHandle> {
    if handle == 0 {
        return None;
    }
    unsafe { Some(&mut *(handle as *mut EmulatorHandle)) }
}

fn bool_to_jboolean(value: bool) -> jboolean {
    if value { JNI_TRUE } else { JNI_FALSE }
}

fn protect_bool<F: FnOnce() -> bool>(f: F) -> jboolean {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => bool_to_jboolean(value),
        Err(_) => JNI_FALSE,
    }
}

fn protect_void<F: FnOnce()>(f: F) {
    let _ = catch_unwind(AssertUnwindSafe(f));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_create(
    _env: EnvUnowned,
    _class: JClass,
    emulationMode: jint,
) -> jlong {
    match catch_unwind(AssertUnwindSafe(|| {
        EmulatorHandle::new(EmulationMode::from_jint(emulationMode))
    })) {
        Ok(handle) => Box::into_raw(Box::new(handle)) as jlong,
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_setDmgNeutralPalette(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    enabled: jboolean,
) {
    protect_void(|| unsafe {
        if let Some(handle) = handle_from_jlong(handle) {
            handle.set_dmg_neutral_palette(enabled);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_setBootRom(
    env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    mode: jint,
    data: JByteArray,
) -> jboolean {
    protect_bool(|| unsafe {
        // The JVM supplies an attached environment for this native call.
        let mut guard = jni::AttachGuard::from_unowned(env.as_raw());
        let env = guard.borrow_env_mut();
        let Some(handle) = handle_from_jlong(handle) else {
            return false;
        };

        let bytes = match env.convert_byte_array(data) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        handle.set_boot_rom(mode as usize, bytes)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_clearBootRom(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    mode: jint,
) {
    protect_void(|| unsafe {
        let Some(handle) = handle_from_jlong(handle) else {
            return;
        };
        handle.clear_boot_rom(mode as usize);
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_destroy(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
) {
    protect_void(|| unsafe {
        if handle == 0 {
            return;
        }
        let ptr = handle as *mut EmulatorHandle;
        if !ptr.is_null() {
            let mut boxed = Box::from_raw(ptr);
            boxed.disable_mobile_adapter();
            boxed.save();
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_enableMobileAdapter(
    env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    configPath: JString,
) -> jboolean {
    protect_bool(|| unsafe {
        // The JVM supplies an attached environment for this native call.
        let mut guard = jni::AttachGuard::from_unowned(env.as_raw());
        let env = guard.borrow_env_mut();
        let Some(handle) = handle_from_jlong(handle) else {
            return false;
        };

        let Ok(path_str) = configPath.mutf8_chars(env) else {
            return false;
        };

        handle.enable_mobile_adapter(PathBuf::from(path_str.to_str().into_owned()))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_disableMobileAdapter(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
) {
    protect_void(|| unsafe {
        if let Some(handle) = handle_from_jlong(handle) {
            handle.disable_mobile_adapter();
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_loadRom(
    env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    rom: JByteArray,
) -> jboolean {
    protect_bool(|| unsafe {
        // The JVM supplies an attached environment for this native call.
        let mut guard = jni::AttachGuard::from_unowned(env.as_raw());
        let env = guard.borrow_env_mut();
        let bytes = match env.convert_byte_array(rom) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        if let Some(handle) = handle_from_jlong(handle) {
            handle.load_rom(bytes)
        } else {
            false
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_loadRomFile(
    env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    path: JString,
) -> jboolean {
    protect_bool(|| unsafe {
        // The JVM supplies an attached environment for this native call.
        let mut guard = jni::AttachGuard::from_unowned(env.as_raw());
        let env = guard.borrow_env_mut();
        let Some(handle) = handle_from_jlong(handle) else {
            return false;
        };

        let Ok(path_str) = path.mutf8_chars(env) else {
            return false;
        };

        handle.load_rom_from_file(PathBuf::from(path_str.to_str().into_owned()))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_runFrame(
    env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    buffer: JIntArray,
) -> jint {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        // The JVM supplies an attached environment for this native call.
        let mut guard = jni::AttachGuard::from_unowned(env.as_raw());
        let env = guard.borrow_env_mut();
        let Some(handle) = handle_from_jlong(handle) else {
            return 0;
        };

        if !handle.run_frame() {
            return 0;
        }

        let len = match buffer.len(env) {
            Ok(len) => len,
            Err(_) => return 0,
        };
        if len < handle.frame.len() {
            return 0;
        }

        for (dst, src) in handle.argb.iter_mut().zip(handle.frame.iter().copied()) {
            *dst = (0xFF00_0000u32 | src) as i32;
        }

        if buffer.set_region(env, 0, &handle.argb).is_err() {
            return 0;
        }
        let (width, height) = handle.dimensions();
        ((width << 16) | height) as jint
    }))
    .unwrap_or_default()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_drainAudio(
    env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    out: JShortArray,
) -> jint {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        // The JVM supplies an attached environment for this native call.
        let mut guard = jni::AttachGuard::from_unowned(env.as_raw());
        let env = guard.borrow_env_mut();
        let Some(handle) = handle_from_jlong(handle) else {
            return 0;
        };

        let max_len = match out.len(env) {
            Ok(len) => len,
            Err(_) => return 0,
        };
        if max_len < 2 {
            return 0;
        }

        let mut samples = Vec::with_capacity(max_len);
        while samples.len() + 1 < max_len {
            if let Some((left, right)) = handle.audio.pop_stereo() {
                samples.push(left);
                samples.push(right);
            } else {
                break;
            }
        }

        if samples.is_empty() {
            return 0;
        }

        let _ = out.set_region(env, 0, &samples);
        (samples.len() / 2) as jint
    }))
    .unwrap_or_default()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_setInput(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    state: jint,
) {
    protect_void(|| unsafe {
        if let Some(handle) = handle_from_jlong(handle) {
            handle.set_input(state as u8);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_saveRam(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
) {
    protect_void(|| unsafe {
        if let Some(handle) = handle_from_jlong(handle) {
            handle.gb.mmu.save_cart_ram();
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_reset(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
) {
    protect_void(|| unsafe {
        if let Some(handle) = handle_from_jlong(handle) {
            handle.reset();
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_setPlayerInput(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    player: jint,
    state: jint,
) {
    protect_void(|| unsafe {
        if let Some(handle) = handle_from_jlong(handle) {
            handle.set_player_input(player as usize, state as u8);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_setShowBorder(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    show: jboolean,
) {
    protect_void(|| unsafe {
        if let Some(handle) = handle_from_jlong(handle) {
            handle.show_border = show;
            handle.copy_frame();
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_clockHz(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
) -> jint {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        handle_from_jlong(handle).map_or(4_194_304, |handle| handle.gb.model.clock_hz() as jint)
    }))
    .unwrap_or(4_194_304)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_sgbHost(
    _env: EnvUnowned,
    _class: JClass,
    handle: jlong,
) -> jboolean {
    protect_bool(|| unsafe {
        handle_from_jlong(handle).is_some_and(|handle| {
            handle
                .gb
                .mmu
                .ppu
                .sgb
                .as_ref()
                .is_some_and(|sgb| sgb.is_command_host())
        })
    })
}
