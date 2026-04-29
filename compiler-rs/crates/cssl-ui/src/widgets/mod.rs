//! § Built-in retained-mode widget impls.
//!
//! § ROLE
//!   The `Ui` immediate-mode driver constructs widget records inline (no
//!   `Box<dyn Widget>` allocation). For applications that prefer the
//!   retained-mode tree (animations, custom widgets, slot-fill containers),
//!   this module provides concrete `Widget` impls for each surface
//!   primitive.
//!
//! § INVENTORY
//!   - `Button`     — clickable rectangle with text label.
//!   - `Label`      — read-only text.
//!   - `Checkbox`   — toggle box + label.
//!   - `RadioGroup` — set of mutually-exclusive radios.
//!   - `Slider`     — horizontal value-picker.
//!   - `TextInput`  — single-line text editor.
//!   - `Dropdown`   — popup-menu picker.
//!   - `List`       — vertical list of items.
//!   - `TreeView`   — collapsible hierarchy.
//!   - `TabPanel`   — tabs over a stack.
//!   - `ScrollView` — clipped overflow.
//!   - `Image`      — bitmap reference.
//!   - `ProgressBar` — read-only progress.
//!
//! § PRIME-DIRECTIVE attestation
//!   Pure data structures + pure event handlers. No IO, no surveillance.

pub mod button;
pub mod checkbox;
pub mod dropdown;
pub mod image;
pub mod label;
pub mod list;
pub mod progress;
pub mod radio;
pub mod scroll;
pub mod slider;
pub mod tab_panel;
pub mod text_input;
pub mod tree;

pub use button::Button;
pub use checkbox::Checkbox;
pub use dropdown::Dropdown;
pub use image::Image;
pub use label::Label;
pub use list::List;
pub use progress::ProgressBar;
pub use radio::{Radio, RadioGroup};
pub use scroll::ScrollView;
pub use slider::Slider;
pub use tab_panel::{Tab, TabPanel};
pub use text_input::TextInput;
pub use tree::{TreeNode, TreeView};
