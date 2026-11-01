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
