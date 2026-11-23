use super::canvas::snap_angle;
use super::*;
use crate::annotate::tests::{arrow_at, item};

#[test]
fn hidden_ones_are_not_drawn() {
    let mut hidden = arrow_at([1, 1], [50, 50]);
    hidden.hidden = true;
    let shown = arrow_at([5, 5], [60, 60]);
    let stack = vec![item(ImageOperation::Annotate(vec![hidden, shown.clone()]))];
    assert_eq!(visible(&stack), vec![shown]);
}

/// Ctrl+A selects what is shown, as a box over all of it would
#[test]
fn select_all_leaves_out_hidden_ones() {
    let mut hidden = arrow_at([1, 1], [50, 50]);
    hidden.hidden = true;
    let list = vec![
        arrow_at([5, 5], [60, 60]),
        hidden,
        arrow_at([9, 9], [70, 70]),
    ];
    let mut state = AnnotateUi {
        selected: vec![1],
        ..Default::default()
    };
    state.select_all(&list);
    assert_eq!(state.selected, vec![0, 2]);
    state.select_all(&[]);
    assert!(state.selected.is_empty(), "nothing to select");
}

/// Shift keeps a line or an arrow to steps of 15°, as long as it was dragged
#[test]
fn shift_turns_lines_to_steps_of_15_degrees() {
    let near = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01;
    let from = [10., 20.];
    // a little off straight to the right, down, left and up
    for (to, snapped) in [
        ([110., 27.], [10. + 100.245, 20.]),
        ([13., 120.], [10., 20. + 100.045]),
        ([-90., 15.], [10. - 100.125, 20.]),
        ([7., -80.], [10., 20. - 100.045]),
    ] {
        let got = snap_angle(from, to);
        assert!(
            near(got, snapped),
            "{to:?} goes to {got:?}, not {snapped:?}"
        );
    }
    // to the nearest step, as long as before: 16.7° to 15°, 28.8° to 30°, 39.8° to 45°
    for ((dx, dy), degrees) in [((100., 30.), 15f32), ((100., 55.), 30.), ((60., 50.), 45.)] {
        let length = f32::hypot(dx, dy);
        let (sin, cos) = degrees.to_radians().sin_cos();
        let snapped = [10. + length * cos, 20. + length * sin];
        let got = snap_angle(from, [10. + dx, 20. + dy]);
        assert!(
            near(got, snapped),
            "{dx},{dy} goes to {got:?}, not {snapped:?}"
        );
    }
    // not dragged, nothing to turn
    assert!(near(snap_angle(from, from), from));
}

#[test]
fn new_ones_look_like_the_one_changed_last() {
    let mut style = Style::default();
    style.fit((800, 600));
    let new = |style: &Style, tool| style.new_annotation(tool).unwrap();
    let mut text = new(&style, Tool::Text);
    text.font.weight = 700;
    text.outline = 3;
    text.color = [0, 0, 255, 255];
    style.remember(&text);
    let frame = new(&style, Tool::Frame);
    assert_eq!((frame.outline, frame.color), (3, [0, 0, 255, 255]));
    assert_eq!(frame.font, Font::default(), "only texts have a font");
    assert_eq!(new(&style, Tool::Text).font.weight, 700);
    let redaction = new(&style, Tool::Redact);
    assert_eq!(redaction.outline, 0, "redactions have no outline");
    assert_eq!(redaction.color, [0, 0, 0, 255]);
    assert!(
        style.new_annotation(Tool::Select).is_none(),
        "selecting draws nothing"
    );
}

#[test]
fn new_stamps_are_as_large_as_the_last() {
    let mut style = Style::default();
    style.fit((1200, 900));
    let stamp = |style: &Style| style.new_annotation(Tool::Stamp(Stamp::Eye)).unwrap();
    assert_eq!(
        stamp(&style).width,
        7 * 8,
        "eight times the width of a line"
    );
    assert_eq!(stamp(&style).shape, Shape::Stamp(Stamp::Eye));
    let mut larger = stamp(&style);
    larger.width = 300;
    style.remember(&larger);
    assert_eq!(stamp(&style).width, 300);
    assert_eq!(
        style.new_annotation(Tool::Frame).unwrap().width,
        7,
        "lines keep their width"
    );
}

/// In a small box the handles left nothing in between: what was in it could only be resized
#[test]
fn a_small_box_can_be_grabbed_inside() {
    let view = egui::Rect::from_min_size(egui::pos2(0., 0.), egui::vec2(800., 600.));
    let small = egui::Rect::from_min_size(egui::pos2(100., 100.), egui::vec2(16., 16.));
    let handles = Handles::new(small, None, view);
    assert_eq!(handles.target(small.center()), None, "the middle is free");
    // just outside a corner it still resizes
    assert_eq!(
        handles.target(egui::pos2(97., 97.)),
        Some(Target::Handle([-1, -1]))
    );
    // in a large box a handle is grabbed from as far as ever
    let large = egui::Rect::from_min_size(egui::pos2(100., 100.), egui::vec2(200., 150.));
    let handles = Handles::new(large, None, view);
    assert_eq!(
        handles.target(egui::pos2(107., 107.)),
        Some(Target::Handle([-1, -1]))
    );
    assert_eq!(handles.target(large.center()), None);
}

/// "Reset rotation" turned an arrow flipped top to bottom around, it was taken as turned by 180
#[test]
fn reset_rotation_turns_back_what_was_turned_only() {
    for ops in [
        vec![],
        vec![item(ImageOperation::Rotate(90))],
        vec![item(ImageOperation::Flip(false))],
    ] {
        let mapping = Mapping::new(&ops, (300, 300));
        let mut arrow = arrow_at([100, 100], [60, 0]);
        arrow.stand_upright(&mapping);
        let middle = middle_in(&arrow, &mapping).unwrap();
        // flipped top to bottom: not turned, it stays as it is
        let flipped_only = flipped(&arrow, &mapping, middle, 1);
        let mut list = vec![flipped_only.clone()];
        menu::reset_rotation(&mut list, &[0], &mapping);
        assert_eq!(list[0], flipped_only, "{ops:?}: a flip is not undone");
        // turned and flipped: upright again, still flipped, where it was
        let turned = flipped(&rotated(&arrow, &mapping, middle, 40), &mapping, middle, 0);
        let mut list = vec![turned.clone()];
        menu::reset_rotation(&mut list, &[0], &mapping);
        let angle = shown_angle(&list[0], &mapping);
        assert!(angle.abs() < 0.5, "{ops:?}: upright, {angle}");
        assert_eq!(list[0].flip, turned.flip, "{ops:?}: still flipped");
        let was = middle_in(&turned, &mapping).unwrap();
        let now = middle_in(&list[0], &mapping).unwrap();
        assert!(
            (was[0] - now[0]).abs() <= 1. && (was[1] - now[1]).abs() <= 1.,
            "{ops:?}"
        );
    }
}

#[test]
fn changes_are_undone_and_redone() {
    let mut history = History::default();
    let mut list = vec![];
    history.record(&list, false);
    // drawn and changed while busy, one step once it is done
    list.push(arrow_at([1, 1], [20, 0]));
    history.record(&list, true);
    list[0].width = 9;
    history.record(&list, false);
    list[0].hidden = true;
    history.record(&list, false);
    // nothing changed, no step
    history.record(&list, false);
    assert!(history.step(&mut list, true));
    assert!(!list[0].hidden && list[0].width == 9);
    assert!(history.step(&mut list, true));
    assert!(list.is_empty());
    assert!(!history.step(&mut list, true), "nothing more to undo");
    assert!(history.step(&mut list, false));
    assert_eq!(list.len(), 1);
    // the step back is not a change to record
    history.record(&list, false);
    assert!(history.step(&mut list, false));
    assert!(list[0].hidden);
    // a new change after an undo: nothing to redo
    assert!(history.step(&mut list, true));
    list.clear();
    history.record(&list, false);
    assert!(!history.step(&mut list, false));
    assert!(history.step(&mut list, true));
    assert_eq!(list.len(), 1);
    // a new image starts over
    history.clear();
    assert!(!history.step(&mut list, true));
}

/// The oldest steps go, the newest stay
#[test]
fn a_long_history_keeps_its_newest_steps() {
    let mut history = History::default();
    let mut list = vec![arrow_at([1, 1], [20, 0])];
    history.record(&list, false);
    for width in 1..=250 {
        list[0].width = width;
        history.record(&list, false);
    }
    let mut steps = 0;
    while history.step(&mut list, true) {
        steps += 1;
    }
    assert_eq!(steps, 200);
    assert_eq!(list[0].width, 50, "back to as it was 200 steps ago");
}

/// Switched off and on, what was done before can be undone; only a new image starts over
#[test]
fn undo_lasts_while_annotations_are_switched_off() {
    let mut state = AnnotateUi::default();
    let mut list = vec![];
    state.history.record(&list, false);
    list.push(arrow_at([1, 1], [20, 0]));
    state.history.record(&list, false);
    state.stop();
    // removed and added again: a new, empty list
    let mut list = vec![];
    state.history.record(&list, false);
    assert!(
        state.history.step(&mut list, true),
        "the empty list is a step"
    );
    assert_eq!(list.len(), 1, "the arrow is back");
    state.new_image();
    assert!(
        !state.history.step(&mut list, true),
        "a new image starts over"
    );
}

#[test]
fn counters_count_on_from_the_highest() {
    let counter = |n| Annotation::new(Shape::Counter(n), [229, 72, 74, 255], 40, [0, 0]);
    assert_eq!(next_counter(&[]), 1);
    assert_eq!(next_counter(&[arrow_at([0, 0], [5, 5])]), 1);
    let list = [counter(1), counter(5), arrow_at([0, 0], [5, 5]), counter(2)];
    assert_eq!(
        next_counter(&list),
        6,
        "one removed in between is not counted again"
    );
    assert_eq!(counter(3).label(), "Counter 3");
    assert_eq!(
        panel::names(&[counter(1), counter(2)]),
        ["Counter 1", "Counter 2"]
    );
}

#[test]
fn counters_after_a_removed_one_count_on_without_a_gap() {
    let counter = |n| Annotation::new(Shape::Counter(n), [0; 4], 40, [0, 0]);
    let numbers = |list: &[Annotation]| -> Vec<u32> {
        list.iter()
            .filter_map(|a| match a.shape {
                Shape::Counter(n) => Some(n),
                _ => None,
            })
            .collect()
    };
    let mut state = AnnotateUi::default();
    let mut list = vec![
        counter(1),
        counter(2),
        arrow_at([0, 0], [5, 5]),
        counter(3),
        counter(4),
        counter(5),
    ];
    state.remove(&mut list, vec![3]);
    assert_eq!(numbers(&list), [1, 2, 3, 4]);
    // two at once
    state.remove(&mut list, vec![0, 3]);
    assert_eq!(numbers(&list), [1, 2]);
    // the next one counts on
    assert_eq!(next_counter(&list), 3);
    // one of two with the same number: the other still has it
    let mut list = vec![counter(1), counter(2), counter(2), counter(3)];
    state.remove(&mut list, vec![1]);
    assert_eq!(numbers(&list), [1, 2, 3]);
    // removing something else changes no number
    let mut list = vec![counter(1), arrow_at([0, 0], [5, 5]), counter(2)];
    state.remove(&mut list, vec![1]);
    assert_eq!(numbers(&list), [1, 2]);
}

#[test]
fn counters_look_as_they_are_set() {
    let mut style = Style::default();
    style.fit((600, 600));
    let next = style.new_annotation(Tool::Counter).unwrap();
    let arrow = style.new_annotation(Tool::Arrow).unwrap();
    assert_eq!(
        next.color, arrow.color,
        "the color of the other annotations"
    );
    assert_eq!(next.number_color, white());
    // a changed one: the next ones look like it, the others take its color
    let mut changed = next.clone();
    changed.color = [0, 90, 200, 255];
    changed.number_color = [255, 220, 0, 255];
    style.remember(&changed);
    let next = style.new_annotation(Tool::Counter).unwrap();
    assert_eq!(next.number_color, [255, 220, 0, 255]);
    assert_eq!(
        style.new_annotation(Tool::Arrow).unwrap().color,
        [0, 90, 200, 255]
    );
}

#[test]
fn every_annotation_can_cast_a_shadow_off_at_first() {
    let mut style = Style::default();
    style.fit((600, 600));
    let tools = Tool::ALL.into_iter().chain([Tool::Stamp(Stamp::Check)]);
    for tool in tools.clone() {
        let a = style.new_annotation(tool).unwrap();
        assert!(!a.casts_shadow(), "{tool:?} has none at first");
    }
    // switched on for one, the next ones have it, but for redactions and spotlights
    let mut arrow = style.new_annotation(Tool::Arrow).unwrap();
    arrow.shadow = 40;
    style.remember(&arrow);
    for tool in tools {
        let a = style.new_annotation(tool).unwrap();
        let none = matches!(tool, Tool::Redact | Tool::Spotlight);
        assert_eq!(a.casts_shadow(), !none, "{tool:?}");
    }
}

/// One frame of the canvas with these keys pressed. Returns which of them are still in the
/// input after it, as the shortcuts of the app read it.
fn canvas_frame(
    ctx: &egui::Context,
    list: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    keys: &[(Key, egui::Modifiers)],
) -> Vec<Key> {
    let mapping = Mapping::new(&[], (200, 200));
    let geo = ImageGeometry {
        scale: 1.,
        offset: nalgebra::Vector2::new(0., 0.),
        dimensions: (200, 200),
    };
    let mut input = egui::RawInput::default();
    // held from the first press, let go of before the next frame
    let held = keys.first().map_or(NONE, |(_, m)| *m);
    input.events.push(egui::Event::ModifiersChanged(held));
    for (key, modifiers) in keys {
        input.events.push(egui::Event::Key {
            key: *key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: *modifiers,
        });
    }
    let mut left = vec![];
    let mut output = ctx.run_ui(input, |ui| {
        canvas_ui(ui.ctx(), list, state, &mapping, &geo, true, false);
        left = keys
            .iter()
            .map(|(key, _)| *key)
            .filter(|key| ui.input(|i| i.key_pressed(*key)))
            .collect();
    });
    output.textures_delta.clear();
    left
}

const NONE: egui::Modifiers = egui::Modifiers::NONE;

/// The arrow keys leave a hidden one alone, even when it's selected on its card: nothing on the
/// image would show it moving. Delete removes it, since the card shows it's selected.
#[test]
fn hidden_ones_are_not_moved_by_the_keys() {
    let ctx = egui::Context::default();
    let mut hidden = arrow_at([10, 10], [50, 0]);
    hidden.hidden = true;
    let shown = arrow_at([10, 50], [50, 0]);
    let mut list = vec![hidden.clone(), shown.clone()];
    let mut state = AnnotateUi {
        tool: Some(Tool::Select),
        selected: vec![0],
        ..Default::default()
    };
    // with only a hidden annotation selected the arrows move nothing and don't switch images
    let left = canvas_frame(&ctx, &mut list, &mut state, &[(Key::ArrowRight, NONE)]);
    assert_eq!(list[0], hidden, "not moved");
    assert!(left.is_empty(), "not to the next image");
    // with a shown one selected as well, only that one moves
    state.selected = vec![0, 1];
    let left = canvas_frame(&ctx, &mut list, &mut state, &[(Key::ArrowRight, NONE)]);
    assert!(left.is_empty(), "the arrow moves what is selected only");
    assert_eq!(list[0], hidden);
    assert_eq!(list[1].start((200, 200)), [11, 50]);
    // Delete removes both
    canvas_frame(&ctx, &mut list, &mut state, &[(Key::Backspace, NONE)]);
    assert!(list.is_empty());
}

/// Hiding a selected one deselects it, so Delete can't remove it unseen
#[test]
fn hiding_deselects() {
    let ctx = egui::Context::default();
    let mut list = vec![arrow_at([10, 10], [50, 0]), arrow_at([10, 50], [50, 0])];
    let mut state = AnnotateUi {
        tool: Some(Tool::Select),
        selected: vec![0, 1],
        ..Default::default()
    };
    state.toggle_hidden(&mut list[0], 0);
    assert!(list[0].hidden);
    assert_eq!(state.selected, vec![1]);
    canvas_frame(&ctx, &mut list, &mut state, &[(Key::Delete, NONE)]);
    assert_eq!(list.len(), 1);
    assert!(list[0].hidden, "the hidden one stays");
    // shown again it stays deselected, until it's picked
    state.toggle_hidden(&mut list[0], 0);
    assert!(!list[0].hidden && state.selected.is_empty());
}

/// While a tool is picked the annotations take only their keys: the other shortcuts of the app
/// work. All keys were taken.
#[test]
fn the_annotations_take_only_their_keys() {
    let ctx = egui::Context::default();
    let mut list = vec![arrow_at([10, 50], [50, 0])];
    let mut state = AnnotateUi {
        tool: Some(Tool::Arrow),
        ..Default::default()
    };
    let command = egui::Modifiers::COMMAND;
    // the app's: zoom, fullscreen, info
    for key in [Key::Num1, Key::F, Key::I, Key::Equals] {
        let left = canvas_frame(&ctx, &mut list, &mut state, &[(key, NONE)]);
        assert_eq!(left, vec![key], "{key:?} reaches the app");
    }
    // the arrows don't switch images, even with nothing selected, but Shift still pans
    for key in [
        Key::ArrowLeft,
        Key::ArrowRight,
        Key::ArrowUp,
        Key::ArrowDown,
    ] {
        let left = canvas_frame(&ctx, &mut list, &mut state, &[(key, NONE)]);
        assert!(left.is_empty(), "{key:?} is taken");
        let shift = egui::Modifiers::SHIFT;
        let left = canvas_frame(&ctx, &mut list, &mut state, &[(key, shift)]);
        assert_eq!(left, vec![key], "Shift+{key:?} reaches the app");
    }
    assert_eq!(list[0].start((200, 200)), [10, 50], "nothing moved");
    for key in [Key::O, Key::V] {
        let left = canvas_frame(&ctx, &mut list, &mut state, &[(key, command)]);
        assert_eq!(left, vec![key], "Ctrl+{key:?} reaches the app");
    }
    // theirs: Delete never deletes the file, undo, select all
    for (key, modifiers) in [(Key::Delete, NONE), (Key::Z, command), (Key::A, command)] {
        let left = canvas_frame(&ctx, &mut list, &mut state, &[(key, modifiers)]);
        assert!(left.is_empty(), "{key:?} is taken");
    }
    // Ctrl+A selected the arrow: now the arrows move it, with Shift further
    assert_eq!(state.selected, vec![0]);
    let left = canvas_frame(
        &ctx,
        &mut list,
        &mut state,
        &[(Key::ArrowDown, egui::Modifiers::SHIFT)],
    );
    assert!(left.is_empty());
    assert_eq!(list[0].start((200, 200)), [10, 60]);
    // Esc lets go of it, then of the tool
    assert!(canvas_frame(&ctx, &mut list, &mut state, &[(Key::Escape, NONE)]).is_empty());
    assert!(state.selected.is_empty());
    canvas_frame(&ctx, &mut list, &mut state, &[(Key::Escape, NONE)]);
    assert_eq!(state.tool, Some(Tool::Select));
    // without a tool nothing is taken
    state.tool = None;
    let left = canvas_frame(&ctx, &mut list, &mut state, &[(Key::Delete, NONE)]);
    assert_eq!(left, vec![Key::Delete]);
    assert_eq!(list.len(), 1);
}

/// Runs one frame with these events, the image shown at 100% in the top left
fn events_frame(
    ctx: &egui::Context,
    list: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    mapping: &Mapping,
    events: Vec<egui::Event>,
) {
    let geo = ImageGeometry {
        scale: 1.,
        offset: nalgebra::Vector2::new(0., 0.),
        dimensions: mapping.result,
    };
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(800., 600.),
        )),
        events,
        focused: true,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        canvas_ui(ui.ctx(), list, state, mapping, &geo, false, false);
    });
    output.textures_delta.clear();
}

fn button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}

fn mouse_drag(
    ctx: &egui::Context,
    list: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    mapping: &Mapping,
    from: Pos2,
    to: Pos2,
    modifiers: egui::Modifiers,
) {
    let held = egui::Event::ModifiersChanged(modifiers);
    let frames = [
        vec![held.clone(), egui::Event::PointerMoved(from)],
        vec![held.clone(), button(from, true, modifiers)],
        vec![held.clone(), egui::Event::PointerMoved(from.lerp(to, 0.5))],
        vec![held.clone(), egui::Event::PointerMoved(to)],
        vec![held, button(to, false, modifiers)],
    ];
    for events in frames {
        events_frame(ctx, list, state, mapping, events);
    }
}

/// Shift gives a square or a circle as shown, even on a stretched image
#[test]
fn shift_squares_as_it_is_shown_on_a_stretched_image() {
    let mapping = crate::annotate::tests::stretched();
    for tool in [Tool::Frame, Tool::Circle, Tool::Spotlight] {
        let ctx = egui::Context::default();
        let mut list = vec![];
        let mut state = AnnotateUi {
            tool: Some(tool),
            ..Default::default()
        };
        state.prepare((200, 200));
        mouse_drag(
            &ctx,
            &mut list,
            &mut state,
            &mapping,
            pos2(40., 40.),
            pos2(140., 70.),
            egui::Modifiers::SHIFT,
        );
        assert_eq!(list.len(), 1, "{tool:?}");
        let shown = selection_bounds(&list, &[0], &mapping).unwrap().shape;
        let (w, h) = (shown[2] - shown[0], shown[3] - shown[1]);
        assert!((w - h).abs() <= 2. && w >= 98., "{tool:?}: {w} by {h}");
    }
    // a stamp is as large as the drag
    let ctx = egui::Context::default();
    let mut list = vec![];
    let mut state = AnnotateUi {
        tool: Some(Tool::Stamp(Stamp::Check)),
        ..Default::default()
    };
    state.prepare((200, 200));
    mouse_drag(
        &ctx,
        &mut list,
        &mut state,
        &mapping,
        pos2(40., 40.),
        pos2(100., 100.),
        egui::Modifiers::NONE,
    );
    let shown = selection_bounds(&list, &[0], &mapping).unwrap().shape;
    let (w, h) = (shown[2] - shown[0], shown[3] - shown[1]);
    assert!((w - 60.).abs() <= 2. && (h - 60.).abs() <= 2., "{w} by {h}");
    assert!(
        (shown[0] - 40.).abs() <= 2. && (shown[1] - 40.).abs() <= 2.,
        "{shown:?}"
    );
}

/// Typing goes by the drawn text: a click on it moves the caret, a click elsewhere ends typing,
/// and long text isn't wrapped
#[test]
fn typing_goes_by_the_text_as_it_is_drawn() {
    let ctx = egui::Context::default();
    let mapping = Mapping::new(&[], (400, 200));
    let mut a = Annotation::new(Shape::Text("Hello".into()), [255; 4], 40, [20, 20]);
    a.rotation = 30;
    let mut list = vec![a];
    let mut state = AnnotateUi {
        tool: Some(Tool::Text),
        ..Default::default()
    };
    state.prepare((400, 200));
    state.start_typing(0);
    let stack = vec![item(ImageOperation::Annotate(list.clone()))];
    assert_eq!(visible(&stack).len(), 1, "drawn while it is typed");
    events_frame(&ctx, &mut list, &mut state, &mapping, vec![]);
    events_frame(
        &ctx,
        &mut list,
        &mut state,
        &mapping,
        vec![egui::Event::Text("!".into())],
    );
    assert_eq!(list[0].shape, Shape::Text("Hello!".into()));
    assert!(
        !state.overlay.is_empty(),
        "the caret and the frame are shown"
    );
    // click on the turned "e", the caret goes before it
    let e = line_positions("Hello!", &list[0]);
    let mut at = Point::from_xy((e[1] + e[2]) / 2. - 1., 25.);
    list[0].transform(&mapping).map_point(&mut at);
    let at = pos2(at.x, at.y);
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![button(at, true, egui::Modifiers::NONE)],
        vec![button(at, false, egui::Modifiers::NONE)],
        vec![],
    ] {
        events_frame(&ctx, &mut list, &mut state, &mapping, events);
    }
    assert_eq!(state.typing, Some(0), "still typing");
    events_frame(
        &ctx,
        &mut list,
        &mut state,
        &mapping,
        vec![egui::Event::Text("X".into())],
    );
    assert_eq!(list[0].shape, Shape::Text("HXello!".into()));
    events_frame(
        &ctx,
        &mut list,
        &mut state,
        &mapping,
        vec![egui::Event::Text(" and a long way on".into())],
    );
    let Shape::Text(text) = &list[0].shape else {
        unreachable!()
    };
    assert!(!text.contains('\n'));
    let away = pos2(390., 190.);
    for events in [
        vec![egui::Event::PointerMoved(away)],
        vec![button(away, true, egui::Modifiers::NONE)],
        vec![button(away, false, egui::Modifiers::NONE)],
    ] {
        events_frame(&ctx, &mut list, &mut state, &mapping, events);
    }
    assert_eq!(state.typing, None);
    assert_eq!(list.len(), 1);
}
