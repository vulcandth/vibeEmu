use crate::gameboy::GameBoy;
use egui::RichText;

/// Run-state shared with `main.rs`
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum RunState {
    Running,
    Paused,
}

pub struct Debugger {
    breakpoints: std::collections::HashSet<u16>,
    next_step: bool,
}

impl Default for Debugger {
    fn default() -> Self {
        Self::new()
    }
}

impl Debugger {
    pub fn new() -> Self {
        Self {
            breakpoints: Default::default(),
            next_step: false,
        }
    }

    /// Returns the desired new `RunState`
    pub fn ui(&mut self, gb: &mut GameBoy, ctx: &egui::Context, state: RunState) -> RunState {
        egui::Window::new("Debugger").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("\u{25B6} Run").clicked() {
                    return RunState::Running;
                }
                if ui.button("\u{23F8} Pause").clicked() {
                    return RunState::Paused;
                }
                if ui.button("\u{23ED} Step").clicked() {
                    self.next_step = true;
                    return RunState::Paused;
                }
                state
            });

            ui.separator();

            // --- Registers -------------------------------------------------
            let cpu = &gb.cpu;
            ui.label(RichText::new("Registers").heading());
            egui_extras::TableBuilder::new(ui)
                .column(egui_extras::Column::auto())
                .column(egui_extras::Column::auto())
                .body(|mut body| {
                    body.row(18.0, |mut r| {
                        r.col(|c| {
                            c.label("PC");
                        });
                        r.col(|c| {
                            c.monospace(format!("{:04X}", cpu.pc));
                        });
                    });
                    body.row(18.0, |mut r| {
                        r.col(|c| {
                            c.label("SP");
                        });
                        r.col(|c| {
                            c.monospace(format!("{:04X}", cpu.sp));
                        });
                    });
                    body.row(18.0, |mut r| {
                        r.col(|c| {
                            c.label("AF");
                        });
                        r.col(|c| {
                            c.monospace(format!("{:04X}", ((cpu.a as u16) << 8) | cpu.f as u16));
                        });
                    });
                    body.row(18.0, |mut r| {
                        r.col(|c| {
                            c.label("BC");
                        });
                        r.col(|c| {
                            c.monospace(format!("{:04X}", cpu.get_bc()));
                        });
                    });
                    body.row(18.0, |mut r| {
                        r.col(|c| {
                            c.label("DE");
                        });
                        r.col(|c| {
                            c.monospace(format!("{:04X}", cpu.get_de()));
                        });
                    });
                    body.row(18.0, |mut r| {
                        r.col(|c| {
                            c.label("HL");
                        });
                        r.col(|c| {
                            c.monospace(format!("{:04X}", cpu.get_hl()));
                        });
                    });
                });

            ui.separator();

            // --- Breakpoints ----------------------------------------------
            ui.label(RichText::new("Breakpoints").heading());
            let mut delete: Option<u16> = None;
            for bp in &self.breakpoints {
                ui.horizontal(|ui| {
                    if ui.small_button("\u{2716}").clicked() {
                        delete = Some(*bp);
                    }
                    ui.monospace(format!("0x{:04X}", bp));
                });
            }
            if let Some(addr) = delete {
                self.breakpoints.remove(&addr);
            }

            #[allow(static_mut_refs)]
            ui.horizontal(|ui| {
                static mut NEW_BP: String = String::new();
                let text = unsafe { &mut NEW_BP };
                if ui.text_edit_singleline(text).lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    if let Ok(v) = u16::from_str_radix(text.trim_start_matches("0x"), 16) {
                        self.breakpoints.insert(v);
                        text.clear();
                    }
                }
            });
        });

        state
    }

    /// true = CPU should stop after executing `step`
    pub fn consume_step(&mut self) -> bool {
        let s = self.next_step;
        self.next_step = false;
        s
    }

    /// true = PC matches a user breakpoint
    pub fn breakpoint_hit(&self, pc: u16) -> bool {
        self.breakpoints.contains(&pc)
    }
}
