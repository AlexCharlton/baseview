use winapi::shared::guiddef::GUID;
use winapi::shared::minwindef::{ATOM, FALSE, LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::ntdef::PCWSTR;
use winapi::shared::windef::{HCURSOR, HWND, POINT, RECT};
use winapi::shared::winerror::{OLE_E_WRONGCOMPOBJ, RPC_E_CHANGED_MODE, S_OK};
use winapi::um::combaseapi::CoCreateGuid;
use winapi::um::libloaderapi::GetModuleHandleA;
use winapi::um::winuser::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetDpiForWindow, GetMessageW, GetWindowLongPtrW, LoadCursorW, LoadIconA, MapWindowPoints,
    PostMessageW, RegisterClassW, ReleaseCapture, SetCapture, SetCursor,
    SetProcessDpiAwarenessContext, SetTimer, SetWindowLongPtrW, SetWindowPos, TranslateMessage,
    UnregisterClassW, CS_OWNDC, GET_XBUTTON_WPARAM, GWLP_USERDATA, IDC_ARROW, IDC_CROSS, IDC_HAND,
    IDC_HELP, IDC_IBEAM, IDC_NO, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
    IDC_WAIT, MAKEINTRESOURCEA, MSG, SWP_NOMOVE, SWP_NOZORDER, WHEEL_DELTA, WM_CHAR, WM_CLOSE,
    WM_CREATE, WM_DPICHANGED, WM_INPUTLANGCHANGE, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_NCDESTROY, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SHOWWINDOW, WM_SIZE, WM_SYSCHAR,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WM_USER, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW,
    WS_CAPTION, WS_CHILD, WS_CLIPSIBLINGS, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUPWINDOW,
    WS_SIZEBOX, WS_VISIBLE, XBUTTON1, XBUTTON2,
};
use winapi::um::{ole2, oleidl::LPDROPTARGET};

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::{c_void, OsStr};
use std::marker::PhantomData;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;
use std::rc::Rc;

use raw_window_handle::{
    HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle, Win32WindowHandle,
    WindowsDisplayHandle,
};

const BV_WINDOW_MUST_CLOSE: UINT = WM_USER + 1;

use crate::{
    Data, Event, MouseButton, MouseCursor, MouseEvent, PhyPoint, PhySize, ScrollDelta, Size,
    WindowEvent, WindowHandler, WindowInfo, WindowOpenOptions, WindowScalePolicy,
};

use super::drop_handler::DropHandler;
use super::keyboard::KeyboardState;

#[cfg(feature = "opengl")]
use crate::{gl::GlContext, window::RawWindowHandleWrapper};

unsafe fn generate_guid() -> String {
    let mut guid: GUID = std::mem::zeroed();
    CoCreateGuid(&mut guid);
    format!(
        "{:0X}-{:0X}-{:0X}-{:0X}{:0X}-{:0X}{:0X}{:0X}{:0X}{:0X}{:0X}\0",
        guid.Data1,
        guid.Data2,
        guid.Data3,
        guid.Data4[0],
        guid.Data4[1],
        guid.Data4[2],
        guid.Data4[3],
        guid.Data4[4],
        guid.Data4[5],
        guid.Data4[6],
        guid.Data4[7]
    )
}

const WIN_FRAME_TIMER: usize = 4242;

pub struct WindowHandle {
    hwnd: Option<HWND>,
    is_open: Rc<Cell<bool>>,

    // Ensure handle is !Send
    _phantom: PhantomData<*mut ()>,
}

impl WindowHandle {
    pub fn close(&mut self) {
        if let Some(hwnd) = self.hwnd.take() {
            unsafe {
                PostMessageW(hwnd, BV_WINDOW_MUST_CLOSE, 0, 0);
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open.get()
    }
}

unsafe impl HasRawWindowHandle for WindowHandle {
    fn raw_window_handle(&self) -> RawWindowHandle {
        if let Some(hwnd) = self.hwnd {
            let mut handle = Win32WindowHandle::empty();
            handle.hwnd = hwnd as *mut c_void;

            RawWindowHandle::Win32(handle)
        } else {
            RawWindowHandle::Win32(Win32WindowHandle::empty())
        }
    }
}

struct ParentHandle {
    _hwnd: HWND,
    is_open: Rc<Cell<bool>>,
}

impl ParentHandle {
    pub fn new(hwnd: HWND) -> (Self, WindowHandle) {
        let is_open = Rc::new(Cell::new(true));

        let handle = WindowHandle {
            hwnd: Some(hwnd),
            is_open: Rc::clone(&is_open),
            _phantom: PhantomData::default(),
        };

        (Self { _hwnd: hwnd, is_open }, handle)
    }
}

impl Drop for ParentHandle {
    fn drop(&mut self) {
        self.is_open.set(false);
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CREATE {
        PostMessageW(hwnd, WM_SHOWWINDOW, 0, 0);
        return 0;
    }

    let window_state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if !window_state_ptr.is_null() {
        let result = wnd_proc_inner(hwnd, msg, wparam, lparam, &*window_state_ptr);

        // If any of the above event handlers caused tasks to be pushed to the deferred tasks list,
        // then we'll try to handle them now
        loop {
            // NOTE: This is written like this instead of using a `while let` loop to avoid exending
            //       the borrow of `window_state.deferred_tasks` into the call of
            //       `window_state.handle_deferred_task()` since that may also generate additional
            //       messages.
            let task = match (*window_state_ptr).deferred_tasks.borrow_mut().pop_front() {
                Some(task) => task,
                None => break,
            };

            (*window_state_ptr).handle_deferred_task(task);
        }

        // NOTE: This is not handled in `wnd_proc_inner` because of the deferred task loop above
        if msg == WM_NCDESTROY {
            unregister_wnd_class((*window_state_ptr).window_class);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(window_state_ptr));
        }

        // The actual custom window proc has been moved to another function so we can always handle
        // the deferred tasks regardless of whether the custom window proc returns early or not
        if let Some(result) = result {
            return result;
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Our custom `wnd_proc` handler. If the result contains a value, then this is returned after
/// handling any deferred tasks. otherwise the default window procedure is invoked.
unsafe fn wnd_proc_inner(
    hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM, window_state: &WindowState,
) -> Option<LRESULT> {
    match msg {
        WM_MOUSEMOVE => {
            let mut window = window_state.create_window();
            let mut window = crate::Window::new(&mut window);
            winapi::um::winuser::SetFocus(hwnd);

            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            let physical_pos = PhyPoint { x, y };
            let logical_pos = physical_pos.to_logical(&window_state.window_info.borrow());
            let event = Event::Mouse(MouseEvent::CursorMoved {
                position: logical_pos,
                modifiers: window_state
                    .keyboard_state
                    .borrow()
                    .get_modifiers_from_mouse_wparam(wparam),
            });

            window_state.handler.borrow_mut().as_mut().unwrap().on_event(&mut window, event);

            Some(0)
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let mut window = window_state.create_window();
            let mut window = crate::Window::new(&mut window);

            let value = (wparam >> 16) as i16;
            let value = value as i32;
            let value = value as f32 / WHEEL_DELTA as f32;

            let event = Event::Mouse(MouseEvent::WheelScrolled {
                delta: if msg == WM_MOUSEWHEEL {
                    ScrollDelta::Lines { x: 0.0, y: value }
                } else {
                    ScrollDelta::Lines { x: value, y: 0.0 }
                },
                modifiers: window_state
                    .keyboard_state
                    .borrow()
                    .get_modifiers_from_mouse_wparam(wparam),
            });

            window_state.handler.borrow_mut().as_mut().unwrap().on_event(&mut window, event);

            Some(0)
        }
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_RBUTTONDOWN
        | WM_RBUTTONUP | WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let mut window = window_state.create_window();
            let mut window = crate::Window::new(&mut window);

            let mut mouse_button_counter = window_state.mouse_button_counter.get();

            let button = match msg {
                WM_LBUTTONDOWN | WM_LBUTTONUP => Some(MouseButton::Left),
                WM_MBUTTONDOWN | WM_MBUTTONUP => Some(MouseButton::Middle),
                WM_RBUTTONDOWN | WM_RBUTTONUP => Some(MouseButton::Right),
                WM_XBUTTONDOWN | WM_XBUTTONUP => match GET_XBUTTON_WPARAM(wparam) {
                    XBUTTON1 => Some(MouseButton::Back),
                    XBUTTON2 => Some(MouseButton::Forward),
                    _ => None,
                },
                _ => None,
            };

            if let Some(button) = button {
                let event = match msg {
                    WM_LBUTTONDOWN | WM_MBUTTONDOWN | WM_RBUTTONDOWN | WM_XBUTTONDOWN => {
                        // Capture the mouse cursor on button down
                        mouse_button_counter = mouse_button_counter.saturating_add(1);
                        SetCapture(hwnd);
                        MouseEvent::ButtonPressed {
                            button,
                            modifiers: window_state
                                .keyboard_state
                                .borrow()
                                .get_modifiers_from_mouse_wparam(wparam),
                        }
                    }
                    WM_LBUTTONUP | WM_MBUTTONUP | WM_RBUTTONUP | WM_XBUTTONUP => {
                        // Release the mouse cursor capture when all buttons are released
                        mouse_button_counter = mouse_button_counter.saturating_sub(1);
                        if mouse_button_counter == 0 {
                            ReleaseCapture();
                        }

                        MouseEvent::ButtonReleased {
                            button,
                            modifiers: window_state
                                .keyboard_state
                                .borrow()
                                .get_modifiers_from_mouse_wparam(wparam),
                        }
                    }
                    _ => {
                        unreachable!()
                    }
                };

                window_state.mouse_button_counter.set(mouse_button_counter);

                window_state
                    .handler
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .on_event(&mut window, Event::Mouse(event));
            }

            None
        }
        WM_TIMER => {
            let mut window = window_state.create_window();
            let mut window = crate::Window::new(&mut window);

            if wparam == WIN_FRAME_TIMER {
                if let Ok(mut h) = window_state.handler.try_borrow_mut() {
                    h.as_mut().unwrap().on_frame(&mut window);
                } else {
                    //println!("Warning: baseview: Can't process frame");
                }
            }

            Some(0)
        }
        WM_CLOSE => {
            // Make sure to release the borrow before the DefWindowProc call
            {
                let mut window = window_state.create_window();
                let mut window = crate::Window::new(&mut window);

                window_state
                    .handler
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .on_event(&mut window, Event::Window(WindowEvent::WillClose));
            }

            // DestroyWindow(hwnd);
            // Some(0)
            Some(DefWindowProcW(hwnd, msg, wparam, lparam))
        }
        WM_CHAR | WM_SYSCHAR | WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP
        | WM_INPUTLANGCHANGE => {
            let mut window = window_state.create_window();
            let mut window = crate::Window::new(&mut window);

            let opt_event =
                window_state.keyboard_state.borrow_mut().process_message(hwnd, msg, wparam, lparam);

            if let Some(event) = opt_event {
                window_state
                    .handler
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .on_event(&mut window, Event::Keyboard(event));
            }

            if msg != WM_SYSKEYDOWN {
                Some(0)
            } else {
                None
            }
        }
        WM_SIZE => {
            let mut window = window_state.create_window();
            let mut window = crate::Window::new(&mut window);

            let width = (lparam & 0xFFFF) as u16 as u32;
            let height = ((lparam >> 16) & 0xFFFF) as u16 as u32;

            let new_window_info = {
                let mut window_info = window_state.window_info.borrow_mut();
                let new_window_info =
                    WindowInfo::from_physical_size(PhySize { width, height }, window_info.scale());

                // Only send the event if anything changed
                if window_info.physical_size() == new_window_info.physical_size() {
                    return None;
                }

                *window_info = new_window_info;

                new_window_info
            };

            window_state
                .handler
                .borrow_mut()
                .as_mut()
                .unwrap()
                .on_event(&mut window, Event::Window(WindowEvent::Resized(new_window_info)));

            None
        }
        WM_DPICHANGED => {
            // To avoid weirdness with the realtime borrow checker.
            let new_rect = {
                if let WindowScalePolicy::SystemScaleFactor = window_state.scale_policy {
                    let dpi = (wparam & 0xFFFF) as u16 as u32;
                    let scale_factor = dpi as f64 / 96.0;

                    let mut window_info = window_state.window_info.borrow_mut();
                    *window_info =
                        WindowInfo::from_logical_size(window_info.logical_size(), scale_factor);

                    Some((
                        RECT {
                            left: 0,
                            top: 0,
                            // todo: check if usize fits into i32
                            right: window_info.physical_size().width as i32,
                            bottom: window_info.physical_size().height as i32,
                        },
                        window_state.dw_style,
                    ))
                } else {
                    None
                }
            };
            if let Some((mut new_rect, dw_style)) = new_rect {
                // Convert this desired "client rectangle" size to the actual "window rectangle"
                // size (Because of course you have to do that).
                AdjustWindowRectEx(&mut new_rect, dw_style, 0, 0);

                // Windows makes us resize the window manually. This will trigger another `WM_SIZE` event,
                // which we can then send the user the new scale factor.
                SetWindowPos(
                    hwnd,
                    hwnd,
                    new_rect.left,
                    new_rect.top,
                    new_rect.right - new_rect.left,
                    new_rect.bottom - new_rect.top,
                    SWP_NOZORDER | SWP_NOMOVE,
                );
            }

            None
        }
        WM_SETCURSOR => {
            let cursor = *window_state.cursor.borrow();
            if cursor != LoadCursorW(null_mut(), IDC_ARROW) {
                SetCursor(cursor);
                Some(0)
            } else {
                None
            }
        }
        // NOTE: `WM_NCDESTROY` is handled in the outer function because this deallocates the window
        //        state
        BV_WINDOW_MUST_CLOSE => {
            DestroyWindow(hwnd);
            Some(0)
        }
        _ => None,
    }
}

unsafe fn register_wnd_class() -> ATOM {
    // We generate a unique name for the new window class to prevent name collisions
    let class_name_str = format!("Baseview-{}", generate_guid());
    let mut class_name: Vec<u16> = OsStr::new(&class_name_str).encode_wide().collect();
    class_name.push(0);
    let icon = LoadIconA(GetModuleHandleA(null_mut()), MAKEINTRESOURCEA(1));

    let wnd_class = WNDCLASSW {
        style: CS_OWNDC,
        lpfnWndProc: Some(wnd_proc),
        hInstance: null_mut(),
        lpszClassName: class_name.as_ptr(),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hIcon: icon,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: null_mut(),
        lpszMenuName: null_mut(),
    };

    RegisterClassW(&wnd_class)
}

unsafe fn unregister_wnd_class(wnd_class: ATOM) {
    UnregisterClassW(wnd_class as _, null_mut());
}

/// All data associated with the window. This uses internal mutability so the outer struct doesn't
/// need to be mutably borrowed. Mutably borrowing the entire `WindowState` can be problematic
/// because of the Windows message loops' reentrant nature. Care still needs to be taken to prevent
/// `handler` from indirectly triggering other events that would also need to be handled using
/// `handler`.
struct WindowState {
    /// The HWND belonging to this window. The window's actual state is stored in the `WindowState`
    /// struct associated with this HWND through `unsafe { GetWindowLongPtrW(self.hwnd,
    /// GWLP_USERDATA) } as *const WindowState`.
    pub hwnd: HWND,
    window_class: ATOM,
    window_info: RefCell<WindowInfo>,
    parent_handle: Option<ParentHandle>,
    drop_handler: DropHandler,
    keyboard_state: RefCell<KeyboardState>,
    mouse_button_counter: Cell<usize>,
    // Initialized late so the `Window` can hold a reference to this `WindowState`
    handler: Rc<RefCell<Option<Box<dyn WindowHandler>>>>,
    scale_policy: WindowScalePolicy,
    dw_style: u32,
    cursor: RefCell<HCURSOR>,

    /// Tasks that should be executed at the end of `wnd_proc`. This is needed to avoid mutably
    /// borrowing the fields from `WindowState` more than once. For instance, when the window
    /// handler requests a resize in response to a keyboard event, the window state will already be
    /// borrowed in `wnd_proc`. So the `resize()` function below cannot also mutably borrow that
    /// window state at the same time.
    pub deferred_tasks: RefCell<VecDeque<WindowTask>>,

    #[cfg(feature = "opengl")]
    pub gl_context: Option<GlContext>,
}

impl WindowState {
    fn create_window(&self) -> Window {
        Window { state: self }
    }

    /// Handle a deferred task as described in [`Self::deferred_tasks
    pub(self) fn handle_deferred_task(&self, task: WindowTask) {
        match task {
            WindowTask::Resize(size) => {
                let window_info = {
                    let mut window_info = self.window_info.borrow_mut();
                    let scaling = window_info.scale();
                    *window_info = WindowInfo::from_logical_size(size, scaling);

                    *window_info
                };

                // If the window is a standalone window then the size needs to include the window
                // decorations
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: window_info.physical_size().width as i32,
                    bottom: window_info.physical_size().height as i32,
                };
                unsafe {
                    AdjustWindowRectEx(&mut rect, self.dw_style, 0, 0);
                    SetWindowPos(
                        self.hwnd,
                        self.hwnd,
                        0,
                        0,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        SWP_NOZORDER | SWP_NOMOVE,
                    )
                };
            }
            WindowTask::Drag(data) => {
                super::drag::start_drag(data);
            }
        }
    }
}

/// Tasks that must be deferred until the end of [`wnd_proc()`] to avoid reentrant `WindowState`
/// borrows. See the docstring on [`WindowState::deferred_tasks`] for more information.
#[derive(Debug, Clone)]
enum WindowTask {
    /// Resize the window to the given size. The size is in logical pixels. DPI scaling is applied
    /// automatically.
    Resize(Size),
    /// Start a drag event
    Drag(Data),
}

pub struct Window<'a> {
    state: &'a WindowState,
}

impl Window<'_> {
    pub fn open_parented<P, H, B>(parent: &P, options: WindowOpenOptions, build: B) -> WindowHandle
    where
        P: HasRawWindowHandle,
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        let parent = match parent.raw_window_handle() {
            RawWindowHandle::Win32(h) => h.hwnd as HWND,
            h => panic!("unsupported parent handle {:?}", h),
        };

        let (window_handle, _) = Self::open(true, parent, options, build);

        window_handle
    }

    pub fn open_as_if_parented<H, B>(options: WindowOpenOptions, build: B) -> WindowHandle
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        let (window_handle, _) = Self::open(true, null_mut(), options, build);

        window_handle
    }

    pub fn open_blocking<H, B>(options: WindowOpenOptions, build: B)
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        let (_, hwnd) = Self::open(false, null_mut(), options, build);

        unsafe {
            let mut msg: MSG = std::mem::zeroed();

            loop {
                let window_state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
                if window_state_ptr.is_null() {
                    break;
                }
                let status = GetMessageW(&mut msg, null_mut(), 0, 0);

                if status == -1 {
                    break;
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn open<H, B>(
        parented: bool, parent: HWND, mut options: WindowOpenOptions, build: B,
    ) -> (WindowHandle, HWND)
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        unsafe {
            let mut title: Vec<u16> = OsStr::new(&options.title[..]).encode_wide().collect();
            title.push(0);

            let window_class = register_wnd_class();
            // todo: manage error ^

            let scaling = match options.scale {
                WindowScalePolicy::SystemScaleFactor => 1.0,
                WindowScalePolicy::ScaleFactor(scale) => scale,
            };

            let window_info = WindowInfo::from_logical_size(options.size, scaling);

            let mut rect = RECT {
                left: 0,
                top: 0,
                // todo: check if usize fits into i32
                right: window_info.physical_size().width as i32,
                bottom: window_info.physical_size().height as i32,
            };

            let mut flags = if parented {
                WS_CHILD | WS_VISIBLE
            } else {
                WS_POPUPWINDOW | WS_CAPTION | WS_VISIBLE | WS_MINIMIZEBOX | WS_CLIPSIBLINGS
            };

            if !parented {
                if options.resizable {
                    flags |= WS_SIZEBOX | WS_MAXIMIZEBOX;
                }
                AdjustWindowRectEx(&mut rect, flags, FALSE, 0);
            }

            let hwnd = CreateWindowExW(
                0,
                window_class as _,
                title.as_ptr(),
                flags,
                0,
                0,
                rect.right - rect.left,
                rect.bottom - rect.top,
                parent as *mut _,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            // todo: manage error ^

            #[cfg(feature = "opengl")]
            let gl_context: Option<GlContext> = options.gl_config.map(|gl_config| {
                let mut handle = Win32WindowHandle::empty();
                handle.hwnd = hwnd as *mut c_void;
                let handle = RawWindowHandleWrapper { handle: RawWindowHandle::Win32(handle) };

                GlContext::create(&handle, gl_config).expect("Could not create OpenGL context")
            });
            // The Window refers to this `WindowState`, so this `handler` needs to be
            // initialized later
            let handler: Rc<RefCell<Option<Box<dyn WindowHandler>>>> = Rc::new(RefCell::new(None));

            let (parent_handle, window_handle) = ParentHandle::new(hwnd);
            let parent_handle = if parented { Some(parent_handle) } else { None };

            let drop_handler_window_handler = handler.clone();
            let drop_handler = DropHandler::new(
                hwnd,
                Box::new(move |e, p| {
                    let window_state_ptr =
                        GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
                    let mut window = (*window_state_ptr).create_window();
                    let mut window = crate::Window::new(&mut window);
                    if let Some(p) = p {
                        let mut point = p;
                        if let Some(_parent) = &window.window.state.parent_handle {
                            // mutates point
                            MapWindowPoints(
                                null_mut(),
                                window.window.state.hwnd,
                                &mut point as *mut _ as *mut POINT,
                                1,
                            );
                        }
                        let logical_pos =
                            point.to_logical(&window.window.state.window_info.borrow());
                        let event = Event::Mouse(MouseEvent::CursorMoved {
                            position: logical_pos,
                            modifiers: keyboard_types::Modifiers::empty(),
                        });

                        drop_handler_window_handler
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .on_event(&mut window, event);
                    }
                    drop_handler_window_handler
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .on_event(&mut window, e);
                }),
                options.drop_target_valid.take(),
            );

            let window_state = Box::new(WindowState {
                hwnd,
                window_class,
                window_info: RefCell::new(window_info),
                parent_handle,
                drop_handler,
                keyboard_state: RefCell::new(KeyboardState::new()),
                mouse_button_counter: Cell::new(0),
                handler,
                scale_policy: options.scale,
                dw_style: flags,
                cursor: RefCell::new(LoadCursorW(null_mut(), IDC_ARROW)),

                deferred_tasks: RefCell::new(VecDeque::with_capacity(4)),

                #[cfg(feature = "opengl")]
                gl_context,
            });

            let handler = {
                let mut window = window_state.create_window();
                let mut window = crate::Window::new(&mut window);

                build(&mut window)
            };
            *window_state.handler.borrow_mut() = Some(Box::new(handler));

            let ole_init_result = ole2::OleInitialize(null_mut());
            // It is ok if the initialize result is `S_FALSE` because it might happen that
            // multiple windows are created on the same thread.
            if ole_init_result == OLE_E_WRONGCOMPOBJ {
                panic!("OleInitialize failed! Result was: `OLE_E_WRONGCOMPOBJ`");
            } else if ole_init_result == RPC_E_CHANGED_MODE {
                panic!(
                    "OleInitialize failed! Result was: `RPC_E_CHANGED_MODE`. \
                     Make sure other crates are not using multithreaded COM library \
                     on the same thread or disable drag and drop support."
                );
            }
            let handler_interface_ptr =
                &mut (*window_state.drop_handler.data).interface as LPDROPTARGET;
            assert_eq!(ole2::RegisterDragDrop(hwnd, handler_interface_ptr), S_OK);

            // Only works on Windows 10 unfortunately.
            SetProcessDpiAwarenessContext(
                winapi::shared::windef::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
            );

            // Now we can get the actual dpi of the window.
            let new_rect = if let WindowScalePolicy::SystemScaleFactor = options.scale {
                // Only works on Windows 10 unfortunately.
                let dpi = GetDpiForWindow(hwnd);
                let scale_factor = dpi as f64 / 96.0;

                let mut window_info = window_state.window_info.borrow_mut();
                if window_info.scale() != scale_factor {
                    *window_info =
                        WindowInfo::from_logical_size(window_info.logical_size(), scale_factor);

                    Some(RECT {
                        left: 0,
                        top: 0,
                        // todo: check if usize fits into i32
                        right: window_info.physical_size().width as i32,
                        bottom: window_info.physical_size().height as i32,
                    })
                } else {
                    None
                }
            } else {
                None
            };

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(window_state) as *const _ as _);
            SetTimer(hwnd, WIN_FRAME_TIMER, 15, None);

            if let Some(mut new_rect) = new_rect {
                // Convert this desired"client rectangle" size to the actual "window rectangle"
                // size (Because of course you have to do that).
                AdjustWindowRectEx(&mut new_rect, flags, 0, 0);

                // Windows makes us resize the window manually. This will trigger another `WM_SIZE` event,
                // which we can then send the user the new scale factor.
                SetWindowPos(
                    hwnd,
                    hwnd,
                    new_rect.left,
                    new_rect.top,
                    new_rect.right - new_rect.left,
                    new_rect.bottom - new_rect.top,
                    SWP_NOZORDER | SWP_NOMOVE,
                );
            }

            (window_handle, hwnd)
        }
    }

    pub fn close(&mut self) {
        unsafe {
            PostMessageW(self.state.hwnd, BV_WINDOW_MUST_CLOSE, 0, 0);
        }
    }

    pub fn resize(&mut self, size: Size) {
        // To avoid reentrant event handler calls we'll defer the actual resizing until after the
        // event has been handled
        let task = WindowTask::Resize(size);
        self.state.deferred_tasks.borrow_mut().push_back(task);
    }

    pub fn start_drag(&self, data: Data) {
        // To avoid reentrant event handler calls we'll defer the actual resizing until after the
        // event has been handled
        let task = WindowTask::Drag(data);
        self.state.deferred_tasks.borrow_mut().push_back(task);
    }

    pub fn set_mouse_cursor(&mut self, mouse_cursor: MouseCursor) {
        unsafe {
            let cursor = LoadCursorW(null_mut(), cursor_to_windows_cursor(mouse_cursor));
            *self.state.cursor.borrow_mut() = cursor;
            SetCursor(cursor);
        }
    }

    #[cfg(feature = "opengl")]
    pub fn gl_context(&self) -> Option<&GlContext> {
        self.state.gl_context.as_ref()
    }
}

unsafe impl HasRawWindowHandle for Window<'_> {
    fn raw_window_handle(&self) -> RawWindowHandle {
        let mut handle = Win32WindowHandle::empty();
        handle.hwnd = self.state.hwnd as *mut c_void;

        RawWindowHandle::Win32(handle)
    }
}

unsafe impl HasRawDisplayHandle for Window<'_> {
    fn raw_display_handle(&self) -> RawDisplayHandle {
        let handle = WindowsDisplayHandle::empty();
        RawDisplayHandle::Windows(handle)
    }
}

pub fn copy_to_clipboard(_data: &str) {
    todo!()
}

pub fn cursor_to_windows_cursor(mouse_cursor: MouseCursor) -> PCWSTR {
    match mouse_cursor {
        MouseCursor::Default => IDC_ARROW,
        MouseCursor::PointingHand | MouseCursor::Hand => IDC_HAND,
        MouseCursor::Crosshair => IDC_CROSS,
        MouseCursor::Text | MouseCursor::VerticalText => IDC_IBEAM,
        MouseCursor::NotAllowed => IDC_NO,
        MouseCursor::HandGrabbing | MouseCursor::Move | MouseCursor::AllScroll => IDC_SIZEALL,
        MouseCursor::EResize
        | MouseCursor::WResize
        | MouseCursor::EwResize
        | MouseCursor::ColResize => IDC_SIZEWE,
        MouseCursor::NResize
        | MouseCursor::SResize
        | MouseCursor::NsResize
        | MouseCursor::RowResize => IDC_SIZENS,
        MouseCursor::NeResize | MouseCursor::SwResize | MouseCursor::NeswResize => IDC_SIZENESW,
        MouseCursor::NwResize | MouseCursor::SeResize | MouseCursor::NwseResize => IDC_SIZENWSE,
        MouseCursor::Working => IDC_WAIT,
        MouseCursor::Help => IDC_HELP,
        MouseCursor::Hidden => null_mut(),
        _ => IDC_ARROW, // use arrow for the missing cases.
    }
}
