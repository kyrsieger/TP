//! AppKit counterpart: one retained NSImage, native image view and controls.
//! Geometry and view layout are submitted on the main thread, with no WebView capture.
use super::mac_drag::WindowDrag;
use super::*;
use block2::RcBlock;
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    AnyThread, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::*;
use objc2_foundation::{
    NSData, NSNotification, NSNotificationCenter, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSTimer,
};
use std::{cell::RefCell, ptr::NonNull};

define_class!(
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    struct ImageWindow;
    impl ImageWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { true }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { true }
    }
);
struct Surface {
    app: Callbacks,
    picture: Picture,
    geometry: Geometry,
    window: Retained<ImageWindow>,
    root: Retained<NSView>,
    image: Retained<NSImageView>,
    message: Retained<NSTextField>,
    backdrop: Retained<NSBox>,
    buttons: Vec<Retained<NSButton>>,
    timer: Retained<NSTimer>,
    monitor: Retained<AnyObject>,
    movement_observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    visible: Rect,
    hovered: bool,
    drag: WindowDrag,
    needs_layout: bool,
    pending_index: usize,
}
impl Drop for Surface {
    fn drop(&mut self) {
        self.timer.invalidate();
        unsafe {
            NSEvent::removeMonitor(&self.monitor);
            let center = NSNotificationCenter::defaultCenter();
            for observer in &self.movement_observers {
                center.removeObserver((**observer).as_ref());
            }
        }
        self.window.orderOut(None);
    }
}
thread_local! { static SURFACE:RefCell<Option<Surface>>=const{RefCell::new(None)}; }
fn from_native(r: NSRect) -> Rect {
    Rect {
        x: r.origin.x,
        y: -r.origin.y - r.size.height,
        w: r.size.width,
        h: r.size.height,
    }
}
fn to_native(r: Rect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, -r.y - r.h), NSSize::new(r.w, r.h))
}
fn point() -> (f64, f64) {
    let p = NSEvent::mouseLocation();
    (p.x, -p.y)
}
impl Surface {
    fn sync_native_frame(&mut self) -> bool {
        let mut dirty = self.drag.sync_frame(
            &mut self.geometry,
            &mut self.visible,
            from_native(self.window.frame()),
        );
        if let Some(screen) = self.window.screen() {
            let work = from_native(screen.visibleFrame());
            if self.geometry.work != work {
                self.geometry.work = work;
                dirty = true;
            }
        }
        dirty
    }
    fn navigate(&mut self, direction: isize) {
        step(
            &self.app,
            &self.picture.id,
            self.pending_index,
            self.picture.count,
            direction,
        );
        self.pending_index = (self.pending_index as isize + direction)
            .rem_euclid(self.picture.count as isize) as usize;
    }
    fn draw(&mut self) {
        // Window Server owns the frame until the drag ends. Re-clipping it mid-drag
        // would fight its position updates; retain the latest native frame meanwhile.
        if self.drag.active() {
            return;
        }
        let rect = self.geometry.rect();
        let visible = rect.intersect(self.geometry.work);
        if visible.w < 1. || visible.h < 1. {
            return;
        }
        // Group native layout changes without introducing another implicit animation.
        // NSImageView uses the retained image; no decode or screenshot per frame.
        NSAnimationContext::beginGrouping();
        NSAnimationContext::currentContext().setDuration(0.);
        NSAnimationContext::currentContext().setAllowsImplicitAnimation(false);
        self.window.setFrame_display(to_native(visible), false);
        self.image.setFrame(NSRect::new(
            NSPoint::new(
                rect.x - visible.x,
                visible.h - (rect.y - visible.y) - rect.h,
            ),
            NSSize::new(rect.w, rect.h),
        ));
        self.message.setFrame(NSRect::new(
            NSPoint::new(0., (visible.h - 20.) / 2.),
            NSSize::new(visible.w, 20.),
        ));
        let layout = controls(
            visible.w,
            visible.h,
            1.,
            self.geometry.scale,
            self.picture.index,
            self.picture.count,
        );
        let mtm = MainThreadMarker::new().expect("native preview is main-thread only");
        if let Some(last) = layout.last() {
            let left = last.rect.x;
            let first = &layout[0].rect;
            self.backdrop.setFrame(NSRect::new(
                NSPoint::new(left, visible.h - first.y - first.h),
                NSSize::new(first.x + first.w - left, first.h),
            ));
            self.backdrop.setHidden(!self.hovered);
        }
        while self.buttons.len() > layout.len() {
            if let Some(button) = self.buttons.pop() {
                button.removeFromSuperview();
            }
        }
        while self.buttons.len() < layout.len() {
            let button = unsafe {
                NSButton::buttonWithTitle_target_action(&NSString::from_str(""), None, None, mtm)
            };
            button.setBordered(false);
            button.setContentTintColor(Some(&NSColor::whiteColor()));
            self.root.addSubview(&button);
            self.buttons.push(button);
        }
        for (button, control) in self.buttons.iter().zip(layout) {
            button.setTitle(&NSString::from_str(&control.text));
            let r = control.rect;
            button.setFrame(NSRect::new(
                NSPoint::new(r.x, visible.h - r.y - r.h),
                NSSize::new(r.w, r.h),
            ));
            button.setHidden(!self.hovered);
        }
        self.root.displayIfNeeded();
        NSAnimationContext::endGrouping();
        self.visible = visible;
    }
    fn action(&mut self, action: Action) {
        let anchor = (
            self.visible.x + self.visible.w / 2.,
            self.visible.y + self.visible.h / 2.,
        );
        match action {
            Action::Bigger => self.geometry.zoom(1.25, anchor),
            Action::Smaller => self.geometry.zoom(1. / 1.25, anchor),
            Action::Fit => self.geometry.fit(),
            Action::Fill => self.geometry.fill(),
            Action::Original => self.geometry.zoom(1. / self.geometry.target, anchor),
            Action::Previous => self.navigate(-1),
            Action::Next => self.navigate(1),
            Action::Close => {}
        }
    }
}
pub(super) fn refresh_language() {
    SURFACE.with(|surface| {
        if let Some(surface) = surface.borrow_mut().as_mut() {
            surface
                .window
                .setTitle(&NSString::from_str(if surface.picture.video {
                    crate::i18n::tr("TP 视频首帧预览")
                } else {
                    crate::i18n::tr("TP 大图预览")
                }));
            surface
                .message
                .setStringValue(&NSString::from_str(crate::i18n::tr(
                    surface.picture.unavailable.unwrap_or(""),
                )));
            surface.draw();
        }
    });
}

pub(super) fn hide() {
    SURFACE.with(|s| {
        s.borrow_mut().take();
    });
}
fn close() {
    let app = SURFACE.with(|s| s.borrow().as_ref().map(|s| s.app.clone()));
    hide();
    if let Some(app) = app {
        closed(&app);
    }
}
fn tick() {
    SURFACE.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        let Some(s) = slot.as_mut() else {
            return;
        };
        let mut dirty = std::mem::take(&mut s.needs_layout);
        dirty |= s.sync_native_frame();
        dirty |= s
            .drag
            .finish_if_released(NSEvent::pressedMouseButtons() & 1 != 0);
        if s.drag.active() {
            return;
        }
        let p = point();
        let hovered = s.visible.contains(p.0, p.1)
            && NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(
                NSPoint::new(p.0, -p.1),
                0,
                MainThreadMarker::new().unwrap(),
            ) == s.window.windowNumber();
        dirty |= hovered != s.hovered;
        s.hovered = hovered;
        dirty |= s.geometry.tick();
        if dirty {
            s.draw();
        }
    });
}
fn native_window_moved(number: isize) {
    // AppKit can notify synchronously inside our own setFrame. The mutable borrow
    // skips those notifications; draw records its submitted frame before returning.
    SURFACE.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        if let Some(s) = slot.as_mut().filter(|s| s.window.windowNumber() == number) {
            s.needs_layout |= s.sync_native_frame();
        }
    });
}
fn input(event: NonNull<NSEvent>) -> *mut NSEvent {
    let e = unsafe { event.as_ref() };
    let mut consume = false;
    let mut close_requested = false;
    let mut drag = None;
    SURFACE.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        let Some(s) = slot.as_mut() else {
            return;
        };
        if e.windowNumber() != s.window.windowNumber() {
            return;
        }
        s.needs_layout |= s.sync_native_frame();
        s.needs_layout |= s
            .drag
            .finish_if_released(NSEvent::pressedMouseButtons() & 1 != 0);
        match e.r#type() {
            NSEventType::ScrollWheel | NSEventType::Magnify => {
                let factor = if e.r#type() == NSEventType::Magnify {
                    (1. + e.magnification()).max(0.01)
                } else {
                    let delta = e.scrollingDeltaY()
                        * if e.isDirectionInvertedFromDevice() {
                            -1.
                        } else {
                            1.
                        }
                        * if e.hasPreciseScrollingDeltas() {
                            1.
                        } else {
                            16.
                        };
                    (delta.clamp(-120., 120.) * 0.002).exp()
                };
                s.geometry.zoom(factor, point());
                consume = true;
            }
            NSEventType::LeftMouseDown => {
                let p = point();
                let x = p.0 - s.visible.x;
                let y = p.1 - s.visible.y;
                let hit = controls(
                    s.visible.w,
                    s.visible.h,
                    1.,
                    s.geometry.scale,
                    s.picture.index,
                    s.picture.count,
                )
                .into_iter()
                .find(|c| s.hovered && c.rect.contains(x, y));
                if let Some(control) = hit {
                    if let Some(action) = control.action {
                        if matches!(action, Action::Close) {
                            close_requested = true;
                        } else {
                            s.action(action);
                            s.draw();
                        }
                    }
                } else {
                    s.drag.begin(&mut s.geometry);
                    drag = Some(s.window.clone());
                }
                consume = true;
            }
            NSEventType::KeyDown => {
                let modifiers = e.modifierFlags();
                let command = modifiers
                    .intersects(NSEventModifierFlags::Command | NSEventModifierFlags::Control);
                let text = e
                    .charactersIgnoringModifiers()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if e.keyCode() == 53 || text == " " {
                    close_requested = true;
                    consume = true;
                    return;
                }
                let action = if e.keyCode() == 123 {
                    Some(Action::Previous)
                } else if e.keyCode() == 124 {
                    Some(Action::Next)
                } else if command {
                    match text.as_str() {
                        "+" | "=" => Some(Action::Bigger),
                        "-" => Some(Action::Smaller),
                        "0" => Some(Action::Fit),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(action) = action {
                    s.action(action);
                    s.draw();
                    consume = true;
                }
            }
            _ => {}
        }
    });
    // This returns immediately: movement notifications update geometry and the timer
    // detects button release even when Window Server omits the mouse-up event.
    if let Some(window) = drag {
        window.performWindowDragWithEvent(e);
    }
    if close_requested {
        close();
    }
    if consume {
        std::ptr::null_mut()
    } else {
        event.as_ptr()
    }
}
pub(super) fn show(app: Callbacks, mut picture: Picture) -> Result<()> {
    let mtm = MainThreadMarker::new().ok_or(crate::i18n::tr("预览需要主线程"))?;
    let work = SURFACE
        .with(|s| s.borrow().as_ref().map(|s| s.geometry.work))
        .or_else(|| NSScreen::mainScreen(mtm).map(|s| from_native(s.visibleFrame())))
        .ok_or(crate::i18n::tr("无法确定当前屏幕"))?;
    let geometry = Geometry::new(picture.image.width(), picture.image.height(), 1., work);
    let native_image =
        NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&picture.encoded))
            .ok_or(crate::i18n::tr("无法读取图片"))?;
    picture.image = image::RgbaImage::new(0, 0);
    picture.encoded = Vec::new();
    let window: Retained<ImageWindow> = unsafe {
        msg_send![ImageWindow::alloc(mtm),initWithContentRect:to_native(geometry.rect()),styleMask:NSWindowStyleMask::Borderless,backing:NSBackingStoreType::Buffered,defer:false]
    };
    unsafe {
        window.setReleasedWhenClosed(false);
    }
    // Match Tao's always-on-top level (5), also used by the pinned main panel.
    // AppKit's NSFloatingWindowLevel is only 3 and would remain behind that panel.
    window.setLevel(5);
    window.setTitle(&NSString::from_str(if picture.video {
        crate::i18n::tr("TP 视频首帧预览")
    } else {
        crate::i18n::tr("TP 大图预览")
    }));
    window.setHasShadow(true);
    window.setOpaque(true);
    window.setBackgroundColor(Some(&NSColor::colorWithWhite_alpha(0.094, 1.)));
    let root = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0., 0.),
            NSSize::new(geometry.rect().w, geometry.rect().h),
        ),
    );
    root.setWantsLayer(true);
    root.setClipsToBounds(true);
    let image = NSImageView::initWithFrame(NSImageView::alloc(mtm), root.bounds());
    image.setImage(Some(&native_image));
    image.setImageScaling(NSImageScaling::ScaleAxesIndependently);
    image.setEditable(false);
    image.setAnimates(false);
    root.addSubview(&image);
    let message = NSTextField::labelWithString(
        &NSString::from_str(crate::i18n::tr(picture.unavailable.unwrap_or(""))),
        mtm,
    );
    message.setAlignment(NSTextAlignment::Center);
    message.setTextColor(Some(&NSColor::whiteColor()));
    message.setHidden(picture.unavailable.is_none());
    root.addSubview(&message);
    let backdrop = NSBox::initWithFrame(NSBox::alloc(mtm), NSRect::default());
    backdrop.setBoxType(NSBoxType::Custom);
    backdrop.setBorderWidth(0.);
    backdrop.setTitlePosition(NSTitlePosition::NoTitle);
    backdrop.setFillColor(&NSColor::colorWithWhite_alpha(0.08, 0.86));
    backdrop.setCornerRadius(6.);
    root.addSubview(&backdrop);
    window.setContentView(Some(&root));
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::ScrollWheel
                | NSEventMask::Magnify
                | NSEventMask::LeftMouseDown
                | NSEventMask::KeyDown,
            &RcBlock::new(input),
        )
    }
    .ok_or(crate::i18n::tr("无法注册预览手势"))?;
    let timer = unsafe {
        NSTimer::scheduledTimerWithTimeInterval_repeats_block(
            0.016,
            true,
            &RcBlock::new(|_: NonNull<NSTimer>| tick()),
        )
    };
    let number = window.windowNumber();
    let movement_observers = unsafe {
        [
            NSWindowDidMoveNotification,
            NSWindowDidChangeScreenNotification,
        ]
        .into_iter()
        .map(|name| {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(name),
                Some(&window),
                None,
                &RcBlock::new(move |_: NonNull<NSNotification>| native_window_moved(number)),
            )
        })
        .collect()
    };
    let mut surface = Surface {
        pending_index: picture.index,
        app,
        picture,
        geometry,
        window,
        root,
        image,
        message,
        backdrop,
        buttons: Vec::new(),
        timer,
        monitor,
        movement_observers,
        visible: Rect::default(),
        hovered: false,
        drag: WindowDrag::default(),
        needs_layout: false,
    };
    surface.draw();
    let window = surface.window.clone();
    SURFACE.with(|s| *s.borrow_mut() = Some(surface));
    window.makeKeyAndOrderFront(None);
    Ok(())
}
