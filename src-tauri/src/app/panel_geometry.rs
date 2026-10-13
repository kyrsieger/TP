use super::{
    common::{err, Result},
    model::Settings,
};
use tauri::{Manager, PhysicalPosition, PhysicalRect, PhysicalSize};

fn panel_position(
    settings: &Settings,
    area: &PhysicalRect<i32, u32>,
    size: PhysicalSize<u32>,
    cursor: Option<PhysicalPosition<f64>>,
) -> PhysicalPosition<i32> {
    let mut x =
        f64::from(area.position.x) + f64::from(area.size.width) - f64::from(size.width) - 18.0;
    let mut y =
        f64::from(area.position.y) + f64::from(area.size.height) - f64::from(size.height) - 18.0;
    if let Some(cursor) = cursor {
        x = (cursor.x + 12.0).min(x);
        y = (cursor.y + 12.0).min(y);
    } else if let (true, Some(saved_x), Some(saved_y)) = (
        settings.remember_position,
        settings.panel_x,
        settings.panel_y,
    ) {
        x = saved_x;
        y = saved_y;
    } else if settings.fixed_position == "left" {
        x = f64::from(area.position.x) + 18.0;
    } else if settings.fixed_position == "center" {
        x = f64::from(area.position.x)
            + ((f64::from(area.size.width) - f64::from(size.width)) / 2.0).round();
    }
    PhysicalPosition::new(x.round() as i32, y.round() as i32)
}

/// Keep geometry operations in one native call before showing the panel.
#[tauri::command]
pub(super) fn place_panel(app: tauri::AppHandle, value: Settings) -> Result<()> {
    value.validate()?;
    let window = app.get_webview_window("main").ok_or("面板窗口不存在")?;
    window
        .set_size(PhysicalSize::new(
            value.panel_width.round() as u32,
            value.panel_height.round() as u32,
        ))
        .map_err(err)?;
    let size = window.outer_size().map_err(err)?;
    let cursor = if value.position_mode == "cursor" {
        Some(window.cursor_position().map_err(err)?)
    } else {
        None
    };
    let monitor = match cursor {
        Some(cursor) => window.monitor_from_point(cursor.x, cursor.y),
        None => window.current_monitor(),
    }
    .map_err(err)?;
    if let Some(monitor) = monitor {
        window
            .set_position(panel_position(&value, monitor.work_area(), size, cursor))
            .map_err(err)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_positions_use_physical_monitor_coordinates() {
        let area = PhysicalRect {
            position: PhysicalPosition::new(-1920, 40),
            size: PhysicalSize::new(1920, 1040),
        };
        let size = PhysicalSize::new(420, 600);
        for (fixed, expected_x) in [("left", -1902), ("right", -438), ("center", -1170)] {
            let settings = Settings {
                fixed_position: fixed.into(),
                ..Settings::default()
            };
            assert_eq!(
                panel_position(&settings, &area, size, None),
                PhysicalPosition::new(expected_x, 462)
            );
        }
    }

    #[test]
    fn cursor_placement_ignores_saved_position_and_stays_inside_bottom_right() {
        let area = PhysicalRect {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(1920, 1080),
        };
        let settings = Settings {
            remember_position: true,
            panel_x: Some(100.0),
            panel_y: Some(200.0),
            ..Settings::default()
        };
        let size = PhysicalSize::new(420, 600);
        assert_eq!(
            panel_position(
                &settings,
                &area,
                size,
                Some(PhysicalPosition::new(50.0, 60.0))
            ),
            PhysicalPosition::new(62, 72)
        );
        assert_eq!(
            panel_position(
                &settings,
                &area,
                size,
                Some(PhysicalPosition::new(1900.0, 1000.0))
            ),
            PhysicalPosition::new(1482, 462)
        );
        assert_eq!(
            panel_position(&settings, &area, size, None),
            PhysicalPosition::new(100, 200)
        );
    }

    #[test]
    fn incomplete_saved_position_falls_back_to_fixed_position() {
        let area = PhysicalRect {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(1920, 1080),
        };
        let settings = Settings {
            remember_position: true,
            fixed_position: "left".into(),
            panel_x: Some(100.0),
            panel_y: None,
            ..Settings::default()
        };
        assert_eq!(
            panel_position(&settings, &area, PhysicalSize::new(420, 600), None),
            PhysicalPosition::new(18, 462)
        );
    }
}
