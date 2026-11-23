//! Working with annotations: the tools panel, the annotation cards, drawing, selecting, and editing them on the image. See `crate::annotate` for what they are.

mod canvas;
mod history;
mod menu;
mod panel;
mod selection_box;
mod style;
#[cfg(test)]
mod tests;
mod tools;
mod typing;

pub(crate) use canvas::canvas_ui;
use history::History;
use menu::menu_ui;
pub(crate) use panel::{panel_ui, select_button};
use selection_box::*;
use style::Style;
use tools::{Icon, Tool, paint_icon};
use typing::text_ui;

use std::ops::RangeInclusive;

use egui::{Color32, CursorIcon, Id, Key, Pos2, Stroke, Ui, pos2};
use tiny_skia::{Point, Transform};

use crate::appstate::ImageGeometry;
use crate::image_editing::{ImageOperation, ImgOpItem, canvas_rect};
use crate::ui::{
    BLACK, BUTTON, CARD, OVER_IMAGE_EDGE, Opacity, PANEL, RED, TEXT, TRANSPARENT, WHITE, YELLOW,
    accent_color, fade, opacity, selection_colors, state_color,
};

use crate::annotate::*;

/// The annotations drawn onto the image. Text being typed is included, since the image is
/// where you see it.
pub fn visible(stack: &[ImgOpItem]) -> Vec<Annotation> {
    stack
        .iter()
        .filter(|op| op.active)
        .find_map(|op| match &op.operation {
            ImageOperation::Annotate(annotations) => Some(annotations),
            _ => None,
        })
        .map(|annotations| annotations.iter().filter(|a| !a.hidden).cloned().collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
enum Drag {
    Draw {
        index: usize,
        from: [f32; 2],
    },
    /// What is moved, the shown ones of what is selected, with where each started
    Move {
        from: [f32; 2],
        starts: Vec<(usize, [i32; 2])>,
    },
    Select {
        from: [f32; 2],
        before: Vec<usize>,
    },
    /// Resizing the selection by the handle at `side`. `shape` is its box, lines
    /// not counted. `before` holds the selected annotations as they were.
    Resize {
        side: [i8; 2],
        from: [f32; 2],
        shape: [f32; 4],
        before: Vec<(usize, Annotation)>,
    },
    /// Turning the selection around `pivot`, which stays put. `from` is the mouse's
    /// starting angle, `base` the angle shown at the start.
    Rotate {
        pivot: [f32; 2],
        from: f32,
        base: f32,
        before: Vec<(usize, Annotation)>,
    },
    /// A button above the box was pressed, it acts when released over it
    Button(Action),
}

/// What the mouse and the keys do with annotations
#[derive(Debug, Clone, Default)]
pub struct AnnotateUi {
    /// The tool a drag over the image uses. `None` moves the image instead.
    pub tool: Option<Tool>,
    /// Whether the tools panel is unfolded. Annotating needs it open.
    open: bool,
    style: Style,
    /// The card under the mouse in the panel.
    pointed_card: Option<usize>,
    /// A dropdown or colour picker was open at the start of the frame, so Esc closes it.
    popup_open: bool,
    /// Indices of the selected annotations.
    pub selected: Vec<usize>,
    drag: Option<Drag>,
    /// The text annotation being typed into.
    typing: Option<usize>,
    /// The text box takes keyboard input.
    focus_text: bool,
    /// Where a click on the text being typed put the caret, a drag selects from there
    text_drag: Option<usize>,
    /// The mouse is over the text being typed
    over_text: bool,
    /// The image's size as loaded.
    original: (u32, u32),
    /// The annotations as they appear in the edited image.
    pub layer: AnnotationLayer,
    /// Set when the mouse and keys belong to annotations this frame.
    pub grab: bool,
    /// The corner of the box around what is selected that shows the button to turn
    rotate_corner: Option<[i8; 2]>,
    /// What of the box the mouse is over, since when, for its tooltip
    hovered: Option<(Target, f64)>,
    /// Where on the screen the menu of a right-click on what is selected is open
    menu: Option<Pos2>,
    /// What is drawn over the image this frame: the box around what is selected, its handles and
    /// buttons, the thin box of what the mouse points at, the box that selects. The central
    /// panel paints it right after the image, in its own layer, so it is below windows such as
    /// the file manager and the menus.
    pub overlay: Vec<egui::Shape>,
    history: History,
}

impl AnnotateUi {
    /// Runs before the panel and canvas. Sets the image size and the line widths scaled to it.
    pub fn prepare(&mut self, original: (u32, u32)) {
        self.original = original;
        self.style.fit(original);
    }

    /// Clears the annotations, selection, any drag or typing in progress and the undo history,
    /// for a new image.
    pub fn new_image(&mut self) {
        self.forget_drawn();
        self.history.clear();
    }

    /// Drops the current tool and clears what `new_image` clears, but keeps the undo history:
    /// switched on again, earlier changes can still be undone.
    pub fn stop(&mut self) {
        self.tool = None;
        self.forget_drawn();
    }

    /// Clears the annotations, selection and any drag or typing in progress.
    fn forget_drawn(&mut self) {
        self.layer.reset();
        self.selected.clear();
        self.drag = None;
        self.typing = None;
    }

    /// Selects every annotation that is shown, hidden ones are ignored.
    fn select_all(&mut self, annotations: &[Annotation]) {
        self.selected = (0..annotations.len())
            .filter(|&i| !annotations[i].hidden)
            .collect();
    }

    /// What is selected and shown. What the arrow keys, dragging, and the menu act on. A hidden
    /// annotation can still be selected on its card to change or delete it.
    fn shown(&self, annotations: &[Annotation]) -> Vec<usize> {
        self.selected
            .iter()
            .copied()
            .filter(|i| annotations.get(*i).is_some_and(|a| !a.hidden))
            .collect()
    }

    /// Hides or shows annotation `i`. Hiding also deselects it, since nothing on the image
    /// would show it's selected.
    fn toggle_hidden(&mut self, a: &mut Annotation, i: usize) {
        a.hidden = !a.hidden;
        if a.hidden {
            self.selected.retain(|s| *s != i);
        }
    }

    fn start_typing(&mut self, index: usize) {
        self.typing = Some(index);
        self.text_drag = None;
        self.focus_text = true;
        self.selected = vec![index];
        self.drag = None;
    }

    fn finish_typing(&mut self, annotations: &mut Vec<Annotation>) {
        if let Some(i) = self.typing.take()
            && let Some(a) = annotations.get_mut(i)
        {
            if a.is_empty() {
                self.remove(annotations, vec![i]);
            } else {
                a.reanchor(self.original);
            }
        }
    }

    fn remove(&mut self, annotations: &mut Vec<Annotation>, mut indices: Vec<usize>) {
        indices.sort_unstable();
        indices.dedup();
        let mut gone: Vec<u32> = indices
            .iter()
            .filter_map(|i| match annotations.get(*i)?.shape {
                Shape::Counter(number) => Some(number),
                _ => None,
            })
            .collect();
        for i in indices.into_iter().rev() {
            if i < annotations.len() {
                annotations.remove(i);
            }
        }
        renumber(annotations, &mut gone);
        self.selected.clear();
        self.typing = None;
        self.drag = None;
    }
}
