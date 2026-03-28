#![allow(non_snake_case)]

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use jni::{
    JNIEnv,
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
}

impl EmulationMode {
    fn from_jint(value: jint) -> Self {
        match value {
            1 => Self::ForceDmg,
            2 => Self::ForceCgb,
            _ => Self::Auto,
        }
    }
}

struct EmulatorHandle {
    gb: GameBoy,
    frame: Vec<u32>,
    argb: Vec<i32>,
    mobile: Option<Arc<Mutex<MobileAdapter>>>,
    emulation_mode: EmulationMode,
    current_cgb: bool,
    dmg_neutral_palette: bool,
    bootrom_dmg: Option<Vec<u8>>,
    bootrom_cgb: Option<Vec<u8>>,
    audio: AudioConsumer,
}

impl EmulatorHandle {
    fn new(emulation_mode: EmulationMode) -> Self {
        let initial_cgb = matches!(emulation_mode, EmulationMode::ForceCgb);
        let mut gb = GameBoy::new(Model::from_cgb_flag(initial_cgb));
        let audio = gb.mmu.apu.enable_output(44_100);
        Self {
            gb,
            frame: vec![0; FB_PIXELS],
            argb: vec![0; FB_PIXELS],
            mobile: None,
            emulation_mode,
            current_cgb: initial_cgb,
            dmg_neutral_palette: false,
            bootrom_dmg: None,
            bootrom_cgb: None,
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

    fn recreate_core(&mut self, cgb: bool, power_on: bool) {
        self.current_cgb = cgb;
        let model = Model::from_cgb_flag(cgb);
        self.gb = if power_on {
            GameBoy::new_power_on(model)
        } else {
            GameBoy::new(model)
        };
        self.audio = self.gb.mmu.apu.enable_output(44_100);
        self.attach_serial_and_palette(cgb);
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

    fn load_rom(&mut self, rom: Vec<u8>) -> bool {
        let cgb = match self.emulation_mode {
            EmulationMode::ForceDmg => false,
            EmulationMode::ForceCgb => true,
            EmulationMode::Auto => rom_prefers_cgb(&rom),
        };

        self.recreate_core(cgb, self.boot_rom_configured(cgb));
        self.gb.mmu.load_cart(Cartridge::from_bytes(rom));
        self.load_selected_boot_rom();
        true
    }

    fn load_rom_from_file(&mut self, path: PathBuf) -> bool {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        let cgb = match self.emulation_mode {
            EmulationMode::ForceDmg => false,
            EmulationMode::ForceCgb => true,
            EmulationMode::Auto => rom_prefers_cgb(&bytes),
        };

        self.recreate_core(cgb, self.boot_rom_configured(cgb));

        let cart = match Cartridge::from_file(&path) {
            Ok(cart) => cart,
            Err(_) => return false,
        };
        self.gb.mmu.load_cart(cart);
        self.load_selected_boot_rom();
        true
    }

    fn reset(&mut self) {
        if self.boot_rom_configured(self.current_cgb) {
            self.gb.reset_power_on();
            self.gb.cpu.pc = 0x0000;
        } else {
            self.gb.reset();
        }

        self.audio = self.gb.mmu.apu.enable_output(44_100);
        self.attach_serial_and_palette(self.current_cgb);
    }

    fn set_dmg_neutral_palette(&mut self, enabled: bool) {
        self.dmg_neutral_palette = enabled;

        if !self.current_cgb {
            self.gb.mmu.ppu.set_dmg_palette(if enabled {
                NEUTRAL_DMG_PALETTE
            } else {
                DEFAULT_DMG_PALETTE
            });
        }
    }

    fn set_boot_rom(&mut self, cgb: bool, data: Vec<u8>) {
        if cgb {
            self.bootrom_cgb = Some(data);
        } else {
            self.bootrom_dmg = Some(data);
        }
    }

    fn clear_boot_rom(&mut self, cgb: bool) {
        if cgb {
            self.bootrom_cgb = None;
        } else {
            self.bootrom_dmg = None;
        }
    }

    fn set_input(&mut self, state: u8) {
        let mmu = &mut self.gb.mmu;
        mmu.input.update_state(state, &mut mmu.if_reg);
    }

    fn run_frame(&mut self) -> bool {
        while !self.gb.mmu.ppu.frame_ready() {
            self.gb.cpu.step(&mut self.gb.mmu);
        }

        if let Some(adapter) = &self.mobile
            && let Ok(mut locked) = adapter.lock()
        {
            let _ = locked.poll(17);
        }

        let fb = self.gb.mmu.ppu.framebuffer();
        if fb.len() != self.frame.len() {
            return false;
        }
        self.frame.copy_from_slice(fb);
        self.gb.mmu.ppu.clear_frame_flag();
        true
    }

    fn save(&mut self) {
        let _ = self.gb.mmu.save_cart_ram();
    }

    fn boot_rom_configured(&self, cgb: bool) -> bool {
        if cgb {
            self.bootrom_cgb.is_some()
        } else {
            self.bootrom_dmg.is_some()
        }
    }

    fn selected_boot_rom(&self) -> Option<&Vec<u8>> {
        if self.current_cgb {
            self.bootrom_cgb.as_ref()
        } else {
            self.bootrom_dmg.as_ref()
        }
    }

    fn load_selected_boot_rom(&mut self) {
        if let Some(data) = self.selected_boot_rom() {
            self.gb.mmu.load_boot_rom(data.clone());
            self.gb.cpu.pc = 0x0000;
        }
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
    _env: JNIEnv,
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
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    enabled: jboolean,
) {
    protect_void(|| unsafe {
        if let Some(handle) = handle_from_jlong(handle) {
            handle.set_dmg_neutral_palette(enabled != 0);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_setBootRom(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    mode: jint,
    data: JByteArray,
) {
    protect_void(|| unsafe {
        let Some(handle) = handle_from_jlong(handle) else {
            return;
        };

        let bytes = match env.convert_byte_array(data) {
            Ok(bytes) => bytes,
            Err(_) => return,
        };

        handle.set_boot_rom(mode != 0, bytes);
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_clearBootRom(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    mode: jint,
) {
    protect_void(|| unsafe {
        let Some(handle) = handle_from_jlong(handle) else {
            return;
        };
        handle.clear_boot_rom(mode != 0);
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_destroy(
    _env: JNIEnv,
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
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    configPath: JString,
) -> jboolean {
    protect_bool(|| unsafe {
        let Some(handle) = handle_from_jlong(handle) else {
            return false;
        };

        let Ok(path_str) = env.get_string(&configPath) else {
            return false;
        };

        handle.enable_mobile_adapter(PathBuf::from(path_str.to_string_lossy().into_owned()))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_disableMobileAdapter(
    _env: JNIEnv,
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
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    rom: JByteArray,
) -> jboolean {
    protect_bool(|| unsafe {
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
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    path: JString,
) -> jboolean {
    protect_bool(|| unsafe {
        let Some(handle) = handle_from_jlong(handle) else {
            return false;
        };

        let Ok(path_str) = env.get_string(&path) else {
            return false;
        };

        handle.load_rom_from_file(PathBuf::from(path_str.to_string_lossy().into_owned()))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_runFrame(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    buffer: JIntArray,
) -> jboolean {
    protect_bool(|| unsafe {
        let Some(handle) = handle_from_jlong(handle) else {
            return false;
        };

        if !handle.run_frame() {
            return false;
        }

        let len = match env.get_array_length(&buffer) {
            Ok(len) => len as usize,
            Err(_) => return false,
        };
        if len < FB_PIXELS {
            return false;
        }

        for (dst, src) in handle.argb.iter_mut().zip(handle.frame.iter().copied()) {
            *dst = (0xFF00_0000u32 | src) as i32;
        }

        env.set_int_array_region(&buffer, 0, &handle.argb).is_ok()
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_drainAudio(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    out: JShortArray,
) -> jint {
    match catch_unwind(AssertUnwindSafe(|| unsafe {
        let Some(handle) = handle_from_jlong(handle) else {
            return 0;
        };

        let max_len = match env.get_array_length(&out) {
            Ok(len) => len as usize,
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

        let _ = env.set_short_array_region(&out, 0, &samples);
        (samples.len() / 2) as jint
    })) {
        Ok(value) => value,
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_setInput(
    _env: JNIEnv,
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
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    protect_void(|| unsafe {
        if let Some(handle) = handle_from_jlong(handle) {
            let _ = handle.gb.mmu.save_cart_ram();
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_reset(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    protect_void(|| unsafe {
        if let Some(handle) = handle_from_jlong(handle) {
            handle.reset();
        }
    });
}
