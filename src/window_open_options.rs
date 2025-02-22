use crate::Size;

/// The dpi scaling policy of the window
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowScalePolicy {
    /// Use the system's dpi scale factor
    SystemScaleFactor,
    /// Use the given dpi scale factor (e.g. `1.0` = 96 dpi)
    ScaleFactor(f64),
}

/// The options for opening a new window
pub struct WindowOpenOptions {
    pub title: String,

    /// The logical size of the window.
    ///
    /// These dimensions will be scaled by the scaling policy specified in `scale`. Mouse
    /// position will be passed back as logical coordinates.
    pub size: Size,

    /// The dpi scaling policy
    pub scale: WindowScalePolicy,

    /// Callback that determines if the drop target is valid
    pub drop_target_valid: Option<Box<dyn Fn() -> bool + Send + Sync>>,

    /// Should this window be resizable?
    pub resizable: bool,

    /// If provided, then an OpenGL context will be created for this window. You'll be able to
    /// access this context through [crate::Window::gl_context].
    #[cfg(feature = "opengl")]
    pub gl_config: Option<crate::gl::GlConfig>,
}
