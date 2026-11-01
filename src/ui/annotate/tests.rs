use super::*;
use crate::annotate::tests::{arrow_at, item};

#[test]
fn hidden_ones_are_not_drawn() {
    let mut hidden = arrow_at([1, 1], [50, 50]);
    hidden.hidden = true;
    let shown = arrow_at([5, 5], [60, 60]);
    let stack = vec![item(ImageOperation::Annotate(vec![hidden, shown.clone()]))];
    assert_eq!(visible(&stack, &AnnotateUi::default()), vec![shown]);
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
        assert!((was[0] - now[0]).abs() <= 1. && (was[1] - now[1]).abs() <= 1., "{ops:?}");
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
    assert!(state.history.step(&mut list, true), "the empty list is a step");
    assert_eq!(list.len(), 1, "the arrow is back");
    state.new_image();
    assert!(!state.history.step(&mut list, true), "a new image starts over");
}

#[test]
fn counters_count_on_from_the_highest() {
    let counter = |n| Annotation::new(Shape::Counter(n), [229, 72, 74, 255], 40, [0, 0]);
    assert_eq!(next_counter(&[]), 1);
    assert_eq!(next_counter(&[arrow_at([0, 0], [5, 5])]), 1);
    let list = [counter(1), counter(5), arrow_at([0, 0], [5, 5]), counter(2)];
    assert_eq!(next_counter(&list), 6, "one removed in between is not counted again");
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
    assert_eq!(next.color, arrow.color, "the color of the other annotations");
    assert_eq!(next.number_color, white());
    // a changed one: the next ones look like it, the others take its color
    let mut changed = next.clone();
    changed.color = [0, 90, 200, 255];
    changed.number_color = [255, 220, 0, 255];
    style.remember(&changed);
    let next = style.new_annotation(Tool::Counter).unwrap();
    assert_eq!(next.number_color, [255, 220, 0, 255]);
    assert_eq!(style.new_annotation(Tool::Arrow).unwrap().color, [0, 90, 200, 255]);
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
