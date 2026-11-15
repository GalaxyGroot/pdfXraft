use std::cell::Cell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

#[test]
fn preferences_scroll_preserves_disabled_animation() {
    for animated in [false, true] {
        let scroll = Rc::new(Cell::new(false));
        let requested = scroll.clone();
        let mut h = Harness::builder().with_size(egui::vec2(600.0, 400.0)).with_step_dt(1.0 / 60.0).build_ui(move |ui| {
            super::preferences_scroll(ui, 200.0, |ui| {
                ui.add_space(1000.0);
                let target = ui.label("Scroll target");
                if requested.replace(false) {
                    ui.scroll_to_rect(target.rect, Some(egui::Align::BOTTOM));
                }
                ui.add_space(300.0);
            });
        });
        h.ctx.all_styles_mut(|style| {
            style.scroll_animation = if animated { egui::style::ScrollAnimation::default() } else { egui::style::ScrollAnimation::none() };
        });
        h.run_steps(4);
        let before = h.get_by_label("Scroll target").rect().top();
        scroll.set(true);
        h.run_steps(4);
        let early = h.get_by_label("Scroll target").rect().top();
        h.run_steps(20);
        let settled = h.get_by_label("Scroll target").rect().top();
        assert!((before - settled).abs() > 50.0, "the target must scroll: {before} -> {settled}");
        assert_eq!((early - settled).abs() > 1.0, animated, "animated: {animated}, early: {early}, settled: {settled}");
    }
}

#[test]
fn shortcut_groups_are_spread_over_the_columns_to_near_equal_height() {
    use super::assign_columns;
    // The sizes of the groups the shortcuts window really lists: File, Edit, View, Pages,
    // Navigation, Selection and tools, Help, Other, then four of the extra keys.
    let heights = [9.0, 11.0, 10.0, 5.0, 6.0, 6.0, 3.0, 6.0, 8.0, 9.0, 6.0, 11.0];
    for columns in 1..=3 {
        let assigned = assign_columns(&heights, columns);
        assert_eq!(assigned.len(), columns);
        let mut all: Vec<usize> = assigned.iter().flatten().copied().collect();
        all.sort_unstable();
        assert_eq!(all, (0..heights.len()).collect::<Vec<_>>(), "every group is in exactly one column");
        assert!(assigned.iter().all(|c| c.windows(2).all(|w| w[0] < w[1])), "each column keeps the original order");
        let sums: Vec<f32> = assigned.iter().map(|c| c.iter().map(|&g| heights[g]).sum()).collect();
        let (tallest, shortest) = (sums.iter().cloned().fold(0.0, f32::max), sums.iter().cloned().fold(f32::MAX, f32::min));
        assert!(tallest - shortest <= 4.0, "{columns} columns: {sums:?}");
    }
    // The contiguous split that left one column with 26 rows and another with 15 would not pass.
    let sums: Vec<f32> = assign_columns(&heights, 3).iter().map(|c| c.iter().map(|&g| heights[g]).sum()).collect();
    assert!(sums.iter().all(|s| *s <= 33.0), "{sums:?}");
    // Few groups, or none: no column is invented and nothing panics.
    assert_eq!(assign_columns(&[5.0], 3).len(), 1);
    assert_eq!(assign_columns(&[], 3), vec![Vec::<usize>::new()]);
}
