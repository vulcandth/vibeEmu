use super::*;
use vibe_emu_core::state_store::{Slot, StateStore};

impl EmulatorHandle {
    pub(crate) fn state_operation(
        &mut self,
        root: &std::path::Path,
        operation: i32,
        slot: i32,
        path: &std::path::Path,
    ) -> serde_json::Value {
        let result = (|| -> std::io::Result<_> {
            let slot = match slot {
                1..=10 => Slot::Number(slot as u8),
                11 => Slot::Quick,
                12 => Slot::Recovery,
                _ => return Err(std::io::Error::other("Invalid state slot")),
            };
            let store = StateStore::new(root, &self.gb)?;
            match operation {
                0 => {}
                1 => store.save(&self.gb, slot)?,
                2 => store.load(&mut self.gb, slot)?,
                3 => store.import(&mut self.gb, path)?,
                4 => store.export(&self.gb, path)?,
                _ => return Err(std::io::Error::other("Invalid state operation")),
            }
            if matches!(operation, 2 | 3) {
                self.audio = self.gb.mmu.apu.enable_output(44_100);
                for player in 0..4 {
                    self.gb.mmu.input.set_player_state(player, 0xff);
                }
                self.copy_frame();
            }
            let rows: Vec<_> = Slot::ALL.into_iter().map(|s| {
                let (description, available) = match store.metadata(s) {
                    Ok(Some(m)) => (format!("{:?} · cycle {} · Unix {}", m.model, m.cycles, m.created_unix), true),
                    Ok(None) => ("Empty".into(), false),
                    Err(e) => (format!("Unavailable: {e}"), false),
                };
                serde_json::json!({"label": s.label(), "description": description, "available": available})
            }).collect();
            Ok(rows)
        })();
        match result {
            Ok(rows) => serde_json::json!({"ok": true, "message": if matches!(operation, 2 | 3) {
                "State loaded, including cartridge SRAM. Recovery contains the pre-load state."
            } else { "Ready. Save states include all cartridge SRAM." }, "rows": rows}),
            Err(e) => serde_json::json!({"ok": false, "message": e.to_string()}),
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_vibeemua_NativeBridge_stateOperation(
    env: EnvUnowned,
    _class: JClass,
    handle: jlong,
    root: JString,
    operation: jint,
    slot: jint,
    path: JString,
) -> jni::sys::jstring {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        let mut guard = jni::AttachGuard::from_unowned(env.as_raw());
        let env = guard.borrow_env_mut();
        let Some(handle) = handle_from_jlong(handle) else {
            return std::ptr::null_mut();
        };
        let Ok(root) = root
            .mutf8_chars(env)
            .map(|s| PathBuf::from(s.to_str().into_owned()))
        else {
            return std::ptr::null_mut();
        };
        let Ok(path) = path
            .mutf8_chars(env)
            .map(|s| PathBuf::from(s.to_str().into_owned()))
        else {
            return std::ptr::null_mut();
        };
        let reply = handle
            .state_operation(&root, operation, slot, &path)
            .to_string();
        env.new_string(reply)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }))
    .unwrap_or(std::ptr::null_mut())
}
