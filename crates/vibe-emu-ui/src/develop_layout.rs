//! Workspace layout only: no game settings or emulated machine state.
use eframe::egui;
use egui_dock::{DockState, Node, NodeIndex, Surface, Tree};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io, path::PathBuf, time::Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Panel {
    Disassembly,
    Registers,
    Memory,
    Video,
    Watchpoints,
    Game,
}

impl Panel {
    pub const ALL: [Self; 6] = [
        Self::Disassembly,
        Self::Registers,
        Self::Memory,
        Self::Video,
        Self::Watchpoints,
        Self::Game,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Disassembly => "Disassembly",
            Self::Registers => "Registers & stack",
            Self::Memory => "Memory",
            Self::Video => "Video",
            Self::Watchpoints => "Watchpoints",
            Self::Game => "Game preview",
        }
    }
}

// Persist a small validated schema rather than library internals (node indices,
// transient rectangles, focus and drag state). Invalid files cannot reach indexing
// operations inside the docking library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum LayoutNode {
    Tabs {
        panels: Vec<Panel>,
        active: usize,
    },
    Split {
        vertical: bool,
        fraction: f32,
        first: Box<Self>,
        second: Box<Self>,
    },
}

impl LayoutNode {
    fn capture(tree: &Tree<Panel>, index: NodeIndex) -> Option<Self> {
        match tree.iter().nth(index.0)? {
            Node::Empty => None,
            Node::Leaf(leaf) => Some(Self::Tabs {
                panels: leaf.tabs.clone(),
                active: leaf.active.0,
            }),
            Node::Vertical(split) | Node::Horizontal(split) => Some(Self::Split {
                vertical: matches!(tree[index], Node::Vertical(_)),
                fraction: split.fraction,
                first: Box::new(Self::capture(tree, NodeIndex(index.0 * 2 + 1))?),
                second: Box::new(Self::capture(tree, NodeIndex(index.0 * 2 + 2))?),
            }),
        }
    }

    fn validate(&self, seen: &mut HashSet<Panel>, depth: usize) -> bool {
        if depth > Panel::ALL.len() {
            return false;
        }
        match self {
            Self::Tabs { panels, active } => {
                !panels.is_empty()
                    && *active < panels.len()
                    && panels.iter().all(|p| seen.insert(*p))
            }
            Self::Split {
                fraction,
                first,
                second,
                ..
            } => {
                fraction.is_finite()
                    && (0.0..=1.0).contains(fraction)
                    && first.validate(seen, depth + 1)
                    && second.validate(seen, depth + 1)
            }
        }
    }

    fn install(&self, tree: &mut Tree<Panel>, index: NodeIndex) {
        match self {
            Self::Tabs { panels, active } => {
                tree[index] = Node::leaf_with(panels.clone());
                tree[index].get_leaf_mut().unwrap().active = egui_dock::TabIndex(*active);
            }
            Self::Split {
                vertical,
                fraction,
                first,
                second,
            } => {
                let [a, b] = if *vertical {
                    tree.split_below(index, *fraction, vec![Panel::Game])
                } else {
                    tree.split_right(index, *fraction, vec![Panel::Game])
                };
                first.install(tree, a);
                second.install(tree, b);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Floating {
    root: LayoutNode,
    position: [f32; 2],
    size: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Document {
    version: u32,
    main: Option<LayoutNode>,
    floating: Vec<Floating>,
}

impl Document {
    fn capture(dock: &DockState<Panel>) -> Self {
        let floating = dock
            .iter_surfaces()
            .filter_map(|surface| {
                let Surface::Window(tree, window) = surface else {
                    return None;
                };
                let rect = window.rect();
                Some(Floating {
                    root: LayoutNode::capture(tree, NodeIndex::root())?,
                    position: if rect.is_finite() {
                        [rect.min.x, rect.min.y]
                    } else {
                        [40.0, 80.0]
                    },
                    size: if rect.is_finite() {
                        [rect.width(), rect.height()]
                    } else {
                        [480.0, 360.0]
                    },
                })
            })
            .collect();
        Self {
            version: 1,
            main: LayoutNode::capture(dock.main_surface(), NodeIndex::root()),
            floating,
        }
    }

    fn restore(&self) -> Option<DockState<Panel>> {
        let mut seen = HashSet::new();
        if self.version != 1
            || self.floating.len() > Panel::ALL.len()
            || self
                .main
                .as_ref()
                .is_some_and(|node| !node.validate(&mut seen, 0))
            || !self.floating.iter().all(|window| {
                window.root.validate(&mut seen, 0)
                    && window
                        .position
                        .iter()
                        .all(|v| v.is_finite() && v.abs() <= 100_000.0)
                    && window
                        .size
                        .iter()
                        .all(|v| v.is_finite() && (1.0..=100_000.0).contains(v))
            })
        {
            return None;
        }
        let mut dock = DockState::new(Vec::new());
        if let Some(root) = &self.main {
            dock.push_to_first_leaf(Panel::Disassembly);
            root.install(dock.main_surface_mut(), NodeIndex::root());
        }
        for window in &self.floating {
            let surface = dock.add_window(vec![Panel::Game]);
            window.root.install(&mut dock[surface], NodeIndex::root());
            let state = dock.get_window_state_mut(surface).unwrap();
            state.set_position(eframe::egui::pos2(window.position[0], window.position[1]));
            state.set_size(eframe::egui::vec2(window.size[0], window.size[1]));
        }
        Some(dock)
    }
}

pub struct DevelopLayout {
    pub dock: DockState<Panel>,
    pub compact_panel: Panel,
    pub pending_float: Option<Panel>,
    path: PathBuf,
    saved: Option<Document>,
    last_attempt: Instant,
}

impl DevelopLayout {
    pub fn load(path: PathBuf) -> Self {
        let saved = std::fs::metadata(&path)
            .ok()
            .filter(|m| m.len() <= 64 * 1024)
            .and_then(|_| std::fs::read(&path).ok())
            .and_then(|bytes| serde_json::from_slice::<Document>(&bytes).ok());
        let restored = saved.as_ref().and_then(Document::restore);
        if path.exists() && restored.is_none() {
            log::warn!("Ignoring invalid workspace layout: {}", path.display());
        }
        Self {
            dock: restored.unwrap_or_else(Self::default_dock),
            compact_panel: Panel::Disassembly,
            pending_float: None,
            path,
            saved,
            last_attempt: Instant::now(),
        }
    }

    fn default_dock() -> DockState<Panel> {
        let mut dock = DockState::new(vec![Panel::Disassembly]);
        let tree = dock.main_surface_mut();
        let [top, _] = tree.split_below(NodeIndex::root(), 0.62, vec![Panel::Memory, Panel::Video]);
        let [_, right] = tree.split_right(top, 0.53, vec![Panel::Registers]);
        tree.split_right(right, 0.53, vec![Panel::Game, Panel::Watchpoints]);
        dock
    }

    pub fn reset(&mut self) {
        self.dock = Self::default_dock();
        self.compact_panel = Panel::Disassembly;
    }

    pub fn visible(&self, panel: Panel) -> bool {
        self.dock.find_tab(&panel).is_some()
    }

    pub fn set_visible(&mut self, panel: Panel, visible: bool) {
        match (self.dock.find_tab(&panel), visible) {
            (Some(path), false) => {
                self.dock.remove_tab(path);
            }
            (None, true) => self.dock.push_to_first_leaf(panel),
            _ => {}
        }
        if visible {
            self.focus(panel);
        }
    }

    pub fn focus(&mut self, panel: Panel) {
        self.compact_panel = panel;
        if let Some(path) = self.dock.find_tab(&panel) {
            let _ = self.dock.set_active_tab(path);
            self.dock.set_focused_node_and_surface(path.node_path());
        }
    }

    pub fn floating_panels(&self) -> Vec<Panel> {
        self.dock
            .iter_all_tabs()
            .filter(|(path, _)| !path.surface.is_main())
            .map(|(_, panel)| *panel)
            .collect()
    }

    /// Move an existing panel; never duplicate its debugger state or contents.
    pub fn toggle_floating(&mut self, panel: Panel, rect: egui::Rect) {
        if let Some(path) = self.dock.find_tab(&panel) {
            if path.surface.is_main() {
                self.dock.detach_tab(path, rect);
            } else {
                self.dock.remove_tab(path);
                self.dock.push_to_first_leaf(panel);
                self.focus(panel);
            }
        }
    }

    /// Rate-limit disk access and persist only structural changes, not frame geometry.
    pub fn save(&mut self, force: bool) -> io::Result<()> {
        if !force && self.last_attempt.elapsed().as_secs_f32() < 2.0 {
            return Ok(());
        }
        self.last_attempt = Instant::now();
        let document = Document::capture(&self.dock);
        if self.saved.as_ref() == Some(&document) {
            return Ok(());
        }
        let bytes = serde_json::to_vec_pretty(&document).map_err(io::Error::other)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temporary, bytes)?;
        std::fs::rename(temporary, &self.path)?;
        self.saved = Some(document);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_panel_can_undock_and_dock_back_without_duplicates() {
        let mut layout =
            DevelopLayout::load(std::env::temp_dir().join("vibeemu-undock-test-unused.json"));
        layout.reset();
        for panel in Panel::ALL {
            layout.toggle_floating(
                panel,
                egui::Rect::from_min_size(egui::pos2(20.0, 80.0), egui::vec2(500.0, 300.0)),
            );
            assert_eq!(layout.floating_panels(), vec![panel]);
            assert_eq!(layout.dock.iter_all_tabs().count(), Panel::ALL.len());
            assert!(Document::capture(&layout.dock).restore().is_some());
            layout.toggle_floating(panel, egui::Rect::NOTHING);
            assert!(layout.floating_panels().is_empty());
            assert_eq!(layout.dock.iter_all_tabs().count(), Panel::ALL.len());
        }
    }

    #[test]
    fn persisted_layout_can_be_replaced_and_all_closed_panels_reopened() {
        let path = std::env::temp_dir().join(format!("vibeemu-layout-{}.json", std::process::id()));
        let mut layout = DevelopLayout::load(path.clone());
        layout.reset();
        layout.save(true).unwrap();
        for panel in Panel::ALL {
            layout.set_visible(panel, false);
        }
        layout.save(true).unwrap();
        let mut restored = DevelopLayout::load(path.clone());
        assert_eq!(restored.dock.iter_all_tabs().count(), 0);
        for panel in Panel::ALL {
            restored.set_visible(panel, true);
        }
        restored.save(true).unwrap();
        assert_eq!(
            DevelopLayout::load(path.clone())
                .dock
                .iter_all_tabs()
                .count(),
            6
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn layouts_round_trip_splits_tabs_closed_panels_and_floating_windows() {
        let mut dock = DevelopLayout::default_dock();
        dock.remove_tab(dock.find_tab(&Panel::Video).unwrap());
        let path = dock.find_tab(&Panel::Watchpoints).unwrap();
        dock.remove_tab(path);
        dock.add_window(vec![Panel::Watchpoints]);
        let document = Document::capture(&dock);
        let json = serde_json::to_vec(&document).unwrap();
        let parsed: Document = serde_json::from_slice(&json).unwrap();
        let restored = parsed.restore().unwrap();
        assert!(restored.find_tab(&Panel::Video).is_none());
        assert_eq!(restored.iter_all_tabs().count(), 5);
        assert_eq!(Document::capture(&restored), document);
    }

    #[test]
    fn malformed_layouts_never_reach_the_docking_library() {
        let mut document = Document::capture(&DevelopLayout::default_dock());
        document.version = 99;
        assert!(document.restore().is_none());
        document.version = 1;
        document.main = Some(LayoutNode::Tabs {
            panels: vec![Panel::Memory, Panel::Memory],
            active: 0,
        });
        assert!(document.restore().is_none());
        document.main = Some(LayoutNode::Tabs {
            panels: vec![Panel::Memory],
            active: 2,
        });
        assert!(document.restore().is_none());
        document.main = Some(LayoutNode::Split {
            vertical: true,
            fraction: f32::NAN,
            first: Box::new(LayoutNode::Tabs {
                panels: vec![Panel::Memory],
                active: 0,
            }),
            second: Box::new(LayoutNode::Tabs {
                panels: vec![Panel::Game],
                active: 0,
            }),
        });
        assert!(document.restore().is_none());
    }
}
