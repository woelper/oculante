// use crate::image_loader::*;
// use cmd_lib::run_cmd;

// use crate::{
//     image_editing::{process_pixels, ImageOperation, ScaleFilter},
//     shortcuts::{keypresses_as_markdown, ShortcutExt, Shortcuts},
// };

// use super::*;
// use std::{fs::File, io::Write, path::PathBuf, time::Instant};

// #[test]
// fn ci_load() {
//     open_image(
//         &PathBuf::from("tests/frstvisuals-lmV1g1UbdhQ-unsplash.jpg"),
//         None,
//     )
//     .unwrap();
// }

// #[test]
// /// This test needs a window system and user verification (it should show an image)
// fn net() {
//     std::env::set_var("RUST_LOG", "info");
//     let _ = env_logger::try_init();

//     info!("Spawn thread...");
//     std::thread::spawn(|| {
//         std::process::Command::new("cargo")
//             .args(["run", "--", "-l", "11111"])
//             .output()
//             .unwrap();
//     });

//     info!("Ran the app, sleeping");

//     std::thread::sleep(std::time::Duration::from_secs(5));
//     info!("Running nc");

//     run_cmd! (
//         nc localhost 11111 < tests/test.jpg;
//     )
//     .unwrap();
// }

// #[test]
// #[allow(unreachable_code)]
// fn bench_load_large() {
//     #[cfg(debug_assertions)]
//     panic!("This test needs release mode to pass.");
//     std::env::set_var("RUST_LOG", "info");
//     let _ = env_logger::try_init();
//     let iters = 5;

//     info!("Benching JPEG with {iters} iterations...");
//     let mut total = 0;

//     for _i in 0..iters {
//         let start = Instant::now();
//         open_image(
//             &PathBuf::from("tests/mohsen-karimi-f_2B1vBMaQQ-unsplash.jpg"),
//             None,
//         )
//         .unwrap();
//         let elapsed = start.elapsed();
//         let d = elapsed.as_millis();
//         total += d;
//         info!("Loaded jpg image in {}", d);
//     }
//     info!("{} ms mean", total / iters);
//     let mut f = File::create("benches/load_large_jpg.bench").unwrap();
//     f.write_fmt(format_args!("{}ms", total / iters)).unwrap();

//     info!("Benching PNG with {iters} iterations...");
//     let mut total = 0;

//     for _i in 0..iters {
//         let start = Instant::now();

//         open_image(&PathBuf::from("tests/large.png"), None).unwrap();
//         let elapsed = start.elapsed();
//         let d = elapsed.as_millis();
//         total += d;
//         info!("Loaded png image in {}", d);
//     }
//     info!("{} ms mean", total / iters);
//     let mut f = File::create("benches/load_large_png.bench").unwrap();
//     f.write_fmt(format_args!("{}ms", total / iters)).unwrap();
// }

// #[test]
// fn ci_bench_process_pxl() {
//     std::env::set_var("RUST_LOG", "info");
//     let _ = env_logger::try_init();
//     let iters = 5;
//     info!("Benching this with {iters} iterations...");
//     let mut total = 0;

//     let ops = vec![
//         ImageOperation::Brightness(10),
//         ImageOperation::Contrast(10),
//         ImageOperation::Exposure(20),
//         ImageOperation::Equalize((10, 100)),
//         ImageOperation::Posterize(4),
//         ImageOperation::Desaturate(20),
//         ImageOperation::HSV((20, 0, 0)),
//         // ImageOperation::Noise {amt: 50, mono: false},
//     ];

//     for _i in 0..iters {
//         let f = open_image(
//             &PathBuf::from("tests/mohsen-karimi-f_2B1vBMaQQ-unsplash.jpg"),
//             None,
//         )
//         .unwrap();
//         let mut buffer = f.recv().unwrap().buffer;
//         let start = Instant::now();
//         process_pixels(&mut buffer, &ops);
//         let elapsed = start.elapsed();
//         let d = elapsed.as_millis();
//         total += d;
//         info!("Processed image in {} s", elapsed.as_secs_f32());
//     }
//     info!("{} ms mean", total / iters);
//     info!("295");
// }

// #[test]
// fn bench_process_bright() {
//     std::env::set_var("RUST_LOG", "info");
//     let _ = env_logger::try_init();
//     let iters = 5;
//     info!("Benching this with {iters} iterations...");
//     let mut total = 0;

//     let ops = vec![ImageOperation::Brightness(10)];

//     for _i in 0..iters {
//         let f = open_image(
//             &PathBuf::from("tests/mohsen-karimi-f_2B1vBMaQQ-unsplash.jpg"),
//             None,
//         )
//         .unwrap();
//         let mut buffer = f.recv().unwrap().buffer;
//         let start = Instant::now();
//         process_pixels(&mut buffer, &ops);
//         let elapsed = start.elapsed();
//         let d = elapsed.as_millis();
//         total += d;
//         info!("Processed image in {} s", elapsed.as_secs_f32());
//     }
//     info!("{} ms mean", total / iters);
//     info!("24 simd");
// }

// #[test]
// fn dump_shortcuts() {
//     use std::io::prelude::*;
//     let mut shortcuts_file = File::create("shortcuts.txt").unwrap();

//     let shortcuts = Shortcuts::default_keys();
//     let mut ordered_shortcuts = shortcuts.iter().collect::<Vec<_>>();
//     ordered_shortcuts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

//     for (k, v) in ordered_shortcuts {
//         writeln!(shortcuts_file, "{} = {:?}\n", keypresses_as_markdown(&v), k).unwrap();
//     }
// }

// #[test]
// /// Generate / update flathub meta
// fn flathub() {
//     use chrono::offset::Utc;
//     use chrono::DateTime;
//     use std::time::SystemTime;
//     use xmltree::Element;
//     use xmltree::{EmitterConfig, XMLNode};

//     let kokai_result = std::process::Command::new("kokai")
//         .args(&["release", "--ref", "HEAD"])
//         .output()
//         .unwrap()
//         .stdout;
//     let release_notes = String::from_utf8_lossy(&kokai_result).to_string();
//     let release_notes: String = release_notes
//         .lines()
//         .into_iter()
//         .filter(|l| !l.contains("# HEAD"))
//         .map(|l| format!("{l}\n"))
//         .collect();

//     let metafile = "res/flathub/io.github.woelper.Oculante.metainfo.xml";
//     let s = std::fs::read_to_string(metafile).unwrap();
//     let mut doc = Element::parse(s.as_bytes()).unwrap();
//     let releases = doc.get_mut_child("releases").unwrap();
//     // check if this version is already present, we don't want duplicates
//     for c in &releases.children {
//         if c.as_element().unwrap().attributes.get("version").unwrap() == env!("CARGO_PKG_VERSION") {
//             panic!("This release already exists!");
//         }
//     }
//     let mut new_release = Element::new("release");

//     new_release
//         .attributes
//         .insert("version".into(), env!("CARGO_PKG_VERSION").into());

//     let datetime: DateTime<Utc> = SystemTime::now().into();
//     let date = format!("{}", datetime.format("%Y-%m-%d"));
//     new_release.attributes.insert("date".into(), date);
//     let mut url = Element::new("url");
//     url.attributes.insert("type".into(), "details".into());
//     url.children.insert(
//         0,
//         XMLNode::Text(format!(
//             "https://github.com/woelper/oculante/releases/tag/{}",
//             env!("CARGO_PKG_VERSION")
//         )),
//     );
//     let mut description = Element::new("description");
//     let mut lines = release_notes.lines().into_iter();
//     loop {
//         if let Some(l) = lines.next() {
//             if l.starts_with("###") {
//                 let mut paragraph = Element::new("p");
//                 paragraph.children.insert(
//                     0,
//                     XMLNode::Text(
//                         l.replace("### ", "")
//                             .replace(":sparkles:", "")
//                             .replace(":beetle:", "")
//                             .replace(":green_apple:", "")
//                             .trim()
//                             .into(),
//                     ),
//                 );
//                 description.children.insert(0, XMLNode::Element(paragraph));

//                 let mut list = Element::new("ul");

//                 // skip next empty line
//                 _ = lines.next();
//                 while let Some(commit) = lines.next() {
//                     dbg!(commit);

//                     if commit.starts_with("*") {
//                         let mut item = Element::new("li");
//                         item.children
//                             .insert(0, XMLNode::Text(commit.replace("* ", "")));
//                         list.children.insert(0, XMLNode::Element(item));
//                     } else {
//                         break;
//                     }
//                 }
//                 description.children.insert(1, XMLNode::Element(list));
//             }
//         } else {
//             break;
//         }
//     }

//     new_release
//         .children
//         .insert(0, XMLNode::Element(description));
//     new_release.children.insert(0, XMLNode::Element(url));
//     releases.children.insert(0, XMLNode::Element(new_release));
//     let config = EmitterConfig::new()
//         .autopad_comments(true)
//         .perform_indent(true);

//     // doc.write_with_config(File::create("result.xml").unwrap(), config)
//     //     .unwrap();
//     doc.write_with_config(File::create(metafile).unwrap(), config)
//         .unwrap();
// }

// #[test]
// fn ci_bench_process_all() {
//     std::env::set_var("RUST_LOG", "info");
//     let _ = env_logger::try_init();
//     let iters = 5;
//     info!("Multi-ops: benching this with {iters} iterations...");
//     let mut total = 0;

//     // let blur = ImageOperation::Blur(10);

//     for _i in 0..iters {
//         let ops = vec![
//             ImageOperation::Brightness(10),
//             ImageOperation::ChromaticAberration(5),
//             // ImageOperation::Blur(5),
//             ImageOperation::Desaturate(100),
//             ImageOperation::Resize {
//                 dimensions: (300, 200),
//                 aspect: true,
//                 filter: ScaleFilter::Hamming,
//             },
//             // ImageOperation::
//         ];
//         let f = open_image(
//             &PathBuf::from("tests/mohsen-karimi-f_2B1vBMaQQ-unsplash.jpg"),
//             None,
//         )
//         .unwrap();
//         let mut buffer = f.recv().unwrap().buffer;
//         let start = Instant::now();
//         process_pixels(&mut buffer, &ops);

//         for op in ops {
//             info!("IMG {:?}", op);
//             op.process_image(&mut buffer).unwrap();
//         }

//         let elapsed = start.elapsed();
//         let d = elapsed.as_millis();
//         total += d;
//         info!("Processed image in {} s", elapsed.as_secs_f32());
//         info!("simd 1.467s");
//     }
//     info!("{} ms mean", total / iters);
// }
