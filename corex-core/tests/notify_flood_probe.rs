use notify_debouncer_full::{new_debouncer, notify::RecursiveMode, DebounceEventResult};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn probe_after_flood() {
    let watch = PathBuf::from(r"C:\Users\iwell\Documents\Vue2\front\master\app");
    let file = watch.join("manifest.json");
    let (tx, rx) = mpsc::channel::<DebounceEventResult>();
    let mut debouncer = new_debouncer(Duration::from_millis(50), None, move |res| {
        let _ = tx.send(res);
    }).unwrap();
    debouncer.watch(&watch, RecursiveMode::Recursive).unwrap();

    // flood: create/delete many temp files
    for i in 0..2000 {
        let p = watch.join(format!("_flood_{i}.tmp"));
        std::fs::write(&p, b"x").unwrap();
        let _ = std::fs::remove_file(&p);
    }
    std::thread::sleep(Duration::from_millis(500));
    // drain
    let mut errors = 0;
    let mut events = 0;
    while let Ok(res) = rx.try_recv() {
        match res {
            Ok(ev) => events += ev.len(),
            Err(e) => {
                errors += 1;
                eprintln!("flood error batch: {e:?}");
            }
        }
    }
    eprintln!("after flood: events={events} error_batches={errors}");

    // now edit manifest
    let original = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("{original} ")).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    std::fs::write(&file, original).unwrap();
    std::thread::sleep(Duration::from_millis(200));

    let mut got_manifest = false;
    while let Ok(res) = rx.try_recv() {
        match res {
            Ok(evs) => {
                for e in evs {
                    eprintln!("post kind={:?} paths={:?}", e.kind, e.paths);
                    if e.paths.iter().any(|p| p.ends_with("manifest.json")) {
                        got_manifest = true;
                    }
                }
            }
            Err(e) => eprintln!("post error: {e:?}"),
        }
    }
    assert!(got_manifest, "manifest change should still notify after flood");
}
