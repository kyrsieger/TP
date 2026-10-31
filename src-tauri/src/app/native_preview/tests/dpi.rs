use super::*;

fn work() -> Rect {
    Rect {
        x: -2000.,
        y: 0.,
        w: 4000.,
        h: 2000.,
    }
}

fn image_point(g: &Geometry, pointer: (f64, f64)) -> (f64, f64) {
    (
        (pointer.0 - g.x) / (g.dpi * g.scale),
        (pointer.1 - g.y) / (g.dpi * g.scale),
    )
}

fn same_point(actual: (f64, f64), expected: (f64, f64)) {
    assert!((actual.0 - expected.0).abs() < 0.000001);
    assert!((actual.1 - expected.1).abs() < 0.000001);
}

#[test]
fn dpi_roundtrip_preserves_image_anchor_zoom_and_toolbar_size() {
    let mut geometry = Geometry::new(400, 200, 1., work());
    geometry.move_to(-900., 250.);
    let original = geometry.rect();
    let pointer = (-780., 330.);
    let original_point = image_point(&geometry, pointer);
    let original_control = controls(original.w, original.h, 1., 1., 0, 1)[0].rect;
    for dpi in [1.25, 2., 1.] {
        assert!(geometry.update_dpi(dpi, pointer));
        assert_eq!(geometry.scale, 1.);
        assert_eq!(geometry.target, 1.);
        assert_eq!(geometry.rect().w, 400. * dpi);
        same_point(image_point(&geometry, pointer), original_point);
        let rect = geometry.rect();
        let control = controls(rect.w, rect.h, dpi, geometry.scale, 0, 1)[0].rect;
        assert_eq!(control.w / dpi, original_control.w);
        assert_eq!(control.h / dpi, original_control.h);
    }
    assert_eq!(geometry.rect(), original);
}

#[test]
fn dpi_change_rebases_an_in_flight_zoom_without_losing_its_target() {
    let mut geometry = Geometry::new(400, 200, 1., work());
    let pointer = (geometry.x + 100., geometry.y + 80.);
    geometry.zoom(2., pointer);
    let original_point = image_point(&geometry, pointer);
    geometry.update_dpi(2., pointer);
    assert_eq!(geometry.target, 2.);
    geometry.last = Instant::now() - std::time::Duration::from_millis(20);
    assert!(geometry.tick());
    assert!(geometry.scale > 1. && geometry.scale < 2.);
    same_point(image_point(&geometry, pointer), original_point);
}

#[test]
fn continuous_drag_preserves_the_grabbed_image_point_across_dpi_changes() {
    let mut geometry = Geometry::new(400, 200, 1., work());
    geometry.move_to(-300., 100.);
    let mut drag = PointerDrag {
        last: (-100., 180.),
    };
    let grabbed = image_point(&geometry, drag.last);
    drag.move_to(&mut geometry, (20., 190.));
    geometry.update_dpi(2., drag.last);
    drag.move_to(&mut geometry, (50., 210.));
    same_point(image_point(&geometry, drag.last), grabbed);
    geometry.update_dpi(1., drag.last);
    drag.move_to(&mut geometry, (-30., 200.));
    same_point(image_point(&geometry, drag.last), grabbed);
    assert_eq!(geometry.scale, 1.);
}

#[test]
fn new_page_fits_using_the_current_preview_dpi() {
    let mut first = Geometry::new(400, 200, 1., work());
    first.update_dpi(2., (0., 500.));
    let next = Geometry::new(300, 150, first.dpi, first.work);
    assert_eq!(next.scale, 1.);
    assert_eq!(next.rect().w, 600.);
    assert_eq!(next.rect().h, 300.);
    assert_eq!(next.rect().x + next.rect().w / 2., 0.);
}
