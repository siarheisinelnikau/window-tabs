use std::collections::HashMap;
use window_tabs::model::*;

#[derive(Default)]
struct Synthetic {
    originals: HashMap<Identity, Original>,
    visible: HashMap<Identity, bool>,
    restored: Vec<Identity>,
    journal: Vec<Original>,
    fail_checkpoint: bool,
    fail_hide: Option<Identity>,
    fail_restore: Option<Identity>,
    effects: usize,
}
fn info(id: u64) -> WindowInfo {
    WindowInfo {
        identity: Identity {
            handle: id,
            pid: id as u32,
            started: 100 + id,
            class: "Editor".into(),
        },
        title: format!("Document {id}"),
        executable: "C:\\Fixture\\editor.exe".into(),
        bounds: bounds(),
    }
}
fn bounds() -> Rect {
    Rect {
        left: 100,
        top: 100,
        right: 900,
        bottom: 700,
    }
}
fn original(id: u64) -> Original {
    Original {
        info: info(id),
        placement: Placement {
            flags: 2,
            show: if id == 2 { 3 } else { 1 },
            min: [-1, -1],
            max: [-1, -1],
            normal: bounds(),
        },
        visible: true,
    }
}
impl Synthetic {
    fn setup() -> Self {
        Self {
            originals: (1..=4)
                .map(|id| (info(id).identity, original(id)))
                .collect(),
            ..Default::default()
        }
    }
}
impl WindowAdapter for Synthetic {
    fn capture(&mut self, id: &Identity) -> Result<Original> {
        self.originals.get(id).cloned().ok_or("Gone".into())
    }
    fn alive(&self, id: &Identity) -> bool {
        self.originals.contains_key(id)
    }
    fn present(&mut self, id: &Identity, _: Rect) -> Result<()> {
        self.effects += 1;
        self.visible.insert(id.clone(), true);
        Ok(())
    }
    fn hide(&mut self, id: &Identity) -> Result<()> {
        self.effects += 1;
        if self.fail_hide.as_ref() == Some(id) {
            return Err("Blocked hide".into());
        }
        self.visible.insert(id.clone(), false);
        Ok(())
    }
    fn restore(&mut self, o: &Original) -> Result<()> {
        self.effects += 1;
        if self.fail_restore.as_ref() == Some(&o.info.identity) {
            return Err("Blocked restore".into());
        }
        self.visible.insert(o.info.identity.clone(), o.visible);
        self.restored.push(o.info.identity.clone());
        assert_eq!(
            o.placement.show,
            original(o.info.identity.handle).placement.show
        );
        Ok(())
    }
    fn focus(&mut self, _: &Identity) {}
    fn checkpoint(&mut self, originals: &[Original]) -> Result<()> {
        if self.fail_checkpoint {
            return Err("Disk full".into());
        }
        self.journal = originals.to_vec();
        Ok(())
    }
}
fn create(engine: &mut Groups<Synthetic>) -> GroupId {
    engine
        .create("Work", &[info(1).identity, info(2).identity], bounds())
        .unwrap()
}

#[test]
fn switching_reordering_detaching_and_closing_restore_originals() {
    let mut engine = Groups::new(Synthetic::setup());
    let group = create(&mut engine);
    assert!(engine.adapter.visible[&info(1).identity]);
    assert!(!engine.adapter.visible[&info(2).identity]);
    engine.activate(group, 1, true).unwrap();
    engine.reorder(group, 1, 0).unwrap();
    assert_eq!(
        engine.get(group).unwrap().selected().unwrap().info.identity,
        info(2).identity
    );
    engine.detach(group, 0).unwrap();
    assert_eq!(engine.adapter.restored, vec![info(2).identity]);
    engine.close(group).unwrap();
    assert!(engine.groups.is_empty());
    assert!(engine.adapter.journal.is_empty());
    assert!(engine.adapter.visible.values().all(|v| *v));
}
#[test]
fn journal_failure_prevents_every_desktop_effect() {
    let mut adapter = Synthetic::setup();
    adapter.fail_checkpoint = true;
    let mut engine = Groups::new(adapter);
    assert!(
        engine
            .create("Work", &[info(1).identity, info(2).identity], bounds())
            .is_err()
    );
    assert_eq!(engine.adapter.effects, 0);
    assert!(engine.groups.is_empty());
}
#[test]
fn partial_creation_rolls_back_and_failed_restore_remains_recoverable() {
    let mut adapter = Synthetic::setup();
    adapter.fail_hide = Some(info(2).identity);
    adapter.fail_restore = Some(info(1).identity);
    let mut engine = Groups::new(adapter);
    assert!(
        engine
            .create("Work", &[info(1).identity, info(2).identity], bounds())
            .is_err()
    );
    assert_eq!(engine.adapter.journal.len(), 1);
    assert_eq!(engine.adapter.journal[0].info.identity, info(1).identity);
    engine.adapter.fail_restore = None;
    engine.close_all().unwrap();
    assert!(engine.adapter.journal.is_empty());
}
#[test]
fn exited_active_process_reveals_next_tab_and_reused_handle_is_not_touched() {
    let mut engine = Groups::new(Synthetic::setup());
    let group = create(&mut engine);
    engine.adapter.originals.remove(&info(1).identity);
    let mut recycled = original(1);
    recycled.info.identity.started = 999;
    engine
        .adapter
        .originals
        .insert(recycled.info.identity.clone(), recycled);
    engine.prune().unwrap();
    assert_eq!(engine.get(group).unwrap().members.len(), 1);
    assert!(engine.adapter.visible[&info(2).identity]);
    engine.close_all().unwrap();
    assert!(!engine.adapter.restored.contains(&info(1).identity));
}
#[test]
fn duplicate_and_already_grouped_windows_are_rejected_without_side_effects() {
    let mut engine = Groups::new(Synthetic::setup());
    assert!(
        engine
            .create("Work", &[info(1).identity, info(1).identity], bounds())
            .is_err()
    );
    assert_eq!(engine.adapter.effects, 0);
    let group = create(&mut engine);
    assert!(engine.add(group, &[info(1).identity]).is_err());
    engine.add(group, &[info(3).identity]).unwrap();
    assert_eq!(engine.adapter.journal.len(), 3);
    assert!(!engine.adapter.visible[&info(3).identity]);
}
#[test]
fn templates_never_guess_ambiguous_windows() {
    let mut engine = Groups::new(Synthetic::setup());
    let group = create(&mut engine);
    let template = Template::from_group(engine.get(group).unwrap());
    assert_eq!(template.resolve(&[info(1), info(2)]).unwrap().len(), 2);
    assert!(template.resolve(&[info(1)]).is_err());
    let mut duplicate = info(3);
    duplicate.title = info(1).title;
    assert!(template.resolve(&[info(1), info(2), duplicate]).is_err());
}
#[test]
fn negative_monitor_coordinates_and_reserved_strip_stay_in_work_area() {
    let work = Rect {
        left: -1920,
        top: -200,
        right: 0,
        bottom: 880,
    };
    let rect = Rect {
        left: -2100,
        top: -300,
        right: 200,
        bottom: 1200,
    }
    .fit(work, 40);
    assert_eq!(
        rect,
        Rect {
            left: -1920,
            top: -160,
            right: 0,
            bottom: 880
        }
    );
}
#[test]
fn invalid_names_and_indices_are_errors() {
    assert!(valid_name("  ").is_err());
    assert!(valid_name("bad\nname").is_err());
    let mut engine = Groups::new(Synthetic::setup());
    let group = create(&mut engine);
    assert!(engine.reorder(group, 0, 9).is_err());
    assert!(engine.detach(group, 9).is_err());
    engine.rename(group, "  Research  ").unwrap();
    assert_eq!(engine.get(group).unwrap().name, "Research");
}
