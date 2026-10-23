use crate::Message;
use iced::widget::svg;
use iced::{Color, Element};
#[derive(Clone, Copy)]
pub enum Icon {
    Logo,
    Search,
    Folder,
    Cube,
    Grid,
    List,
    Filter,
    Settings,
    Bell,
    Left,
    Right,
    Down,
    Plus,
    Heart,
    Star,
    Clock,
    Edit,
    Tag,
    Collection,
    Person,
    Landscape,
    Building,
    Plant,
    Food,
    Moon,
    Video,
    Unsorted,
    Trash,
    Duplicate,
    Raw,
    Maximize,
    Minimize,
    Close,
    Image,
    Database,
    Copy,
    Download,
    More,
    Refresh,
    Network,
    Package,
    Text,
    Sparkles,
    Pause,
    External,
    Pin,
    Camera,
    Save,
    Globe,
    Play,
}
pub fn icon<'a>(kind: Icon, size: f32, color: Color) -> Element<'a, Message> {
    if matches!(kind, Icon::Logo) {
        return svg(svg::Handle::from_memory(
            include_bytes!("../../../assets/branding/moriq.svg").as_slice(),
        ))
        .width(size)
        .height(size)
        .into();
    }
    let body = match kind {
        Icon::Logo => "",
        Icon::Search => r#"<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 4.5 4.5"/>"#,
        Icon::Folder => {
            r#"<path d="M3 6a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/><path d="M3 9h18"/>"#
        }
        Icon::Cube | Icon::Package => {
            r#"<path d="m12 2 9 5v10l-9 5-9-5V7Zm0 10L3 7m9 5 9-5m-9 5v10M7.5 4.5l9 5"/>"#
        }
        Icon::Grid => {
            r#"<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>"#
        }
        Icon::List => r#"<path d="M8 5h13M8 12h13M8 19h13M3 5h.1M3 12h.1M3 19h.1"/>"#,
        Icon::Filter => r#"<path d="M4 4h16v4l-6 6v6h-4v-6L4 8Z"/>"#,
        Icon::Settings => {
            r#"<path d="m10 2-.6 3-2 1-2.9-.9-2 3.5L4.7 11v2L2.5 15.4l2 3.5 2.9-.9 2 1 .6 3h4l.6-3 2-1 2.9.9 2-3.5-2.2-2.4v-2l2.2-2.4-2-3.5-2.9.9-2-1L14 2Z"/><circle cx="12" cy="12" r="3"/>"#
        }
        Icon::Bell => r#"<path d="M5 17h14l-2-3V9a5 5 0 0 0-10 0v5Zm5 3a2 2 0 0 0 4 0M12 2v2"/>"#,
        Icon::Left => r#"<path d="m14 5-7 7 7 7"/>"#,
        Icon::Right => r#"<path d="m9 5 7 7-7 7"/>"#,
        Icon::Down => r#"<path d="m6 9 6 6 6-6"/>"#,
        Icon::Plus => r#"<path d="M12 4v16M4 12h16"/>"#,
        Icon::Download => r#"<path d="M12 3v12m-5-5 5 5 5-5M4 17v4h16v-4"/>"#,
        Icon::Heart => {
            r#"<path d="M20.6 4.8a5.5 5.5 0 0 0-7.8 0l-.8.8-.8-.8a5.5 5.5 0 0 0-7.8 7.8L12 21l8.6-8.4a5.5 5.5 0 0 0 0-7.8Z"/>"#
        }
        Icon::Star => {
            r#"<path d="m12 2 3.1 6.3 7 .9-5 4.9 1.2 7-6.3-3.3-6.3 3.3 1.2-7-5-4.9 7-.9Z"/>"#
        }
        Icon::Clock => r#"<circle cx="12" cy="12" r="9"/><path d="M12 6v6l4 2"/>"#,
        Icon::Edit => r#"<path d="m15 4 5 5M4 15l12-12a2.2 2.2 0 0 1 5 5L9 20l-6 1Z"/>"#,
        Icon::Tag => r#"<path d="M3 3h8l10 10-8 8L3 11Z"/><circle cx="7.5" cy="7.5" r="1"/>"#,
        Icon::Collection => r#"<path d="m12 2 10 6-10 6L2 8Zm-9 11 9 5 9-5M3 18l9 5 9-5"/>"#,
        Icon::Person => r#"<circle cx="12" cy="7" r="4"/><path d="M4 22v-3a8 8 0 0 1 16 0v3Z"/>"#,
        Icon::Landscape | Icon::Image => {
            r#"<rect x="3" y="3" width="18" height="18" rx="3"/><circle cx="8" cy="8" r="1.5"/><path d="m3 18 6-6 4 4 3-3 5 5"/>"#
        }
        Icon::Building => {
            r#"<path d="M3 21V9h6V3h8v10h4v8ZM9 9v12M13 7h1m-1 4h1m-1 4h1M5 13h1m-1 4h1m11 0h1"/>"#
        }
        Icon::Plant => {
            r#"<path d="M12 22V8M12 15C1 16 1 5 5 5c6 0 7 6 7 10Zm0-3c0-8 8-10 8-6 0 6-4 7-8 6"/>"#
        }
        Icon::Food => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M7 7v5m3-5v5M7 10h3m-1.5 2v6m7-11v11m0-11c-4 0-4 6 0 6"/>"#
        }
        Icon::Moon => r#"<path d="M20 15A9 9 0 0 1 9 3a9 9 0 1 0 11 12Z"/>"#,
        Icon::Video => {
            r#"<rect x="3" y="5" width="14" height="14" rx="2"/><path d="m17 10 5-3v10l-5-3ZM8 9l5 3-5 3Z"/>"#
        }
        Icon::Unsorted => r#"<path d="M3 5h7l2 3h9v12H3Z"/>"#,
        Icon::Trash => r#"<path d="M3 6h18M9 3h6m-9 3 1 15h10l1-15M10 10v7m4-7v7"/>"#,
        Icon::Duplicate => {
            r#"<rect x="7" y="7" width="14" height="14" rx="2"/><path d="M17 7V3H3v14h4"/>"#
        }
        Icon::Raw => r#"<path d="M5 2h10l4 4v16H5Zm10 0v5h4M8 12h8m-8 4h8"/>"#,
        Icon::Maximize => r#"<rect x="5" y="5" width="14" height="14" rx="1"/>"#,
        Icon::Minimize => r#"<path d="M5 12h14"/>"#,
        Icon::Close => r#"<path d="m6 6 12 12M6 18 18 6"/>"#,
        Icon::Database => {
            r#"<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 4 18 4 18 0V5M3 12c0 4 18 4 18 0"/>"#
        }
        Icon::Copy => {
            r#"<rect x="8" y="8" width="13" height="13" rx="2"/><path d="M16 8V3H3v13h5"/>"#
        }
        Icon::More => {
            r#"<circle cx="4" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="20" cy="12" r="1"/>"#
        }
        Icon::Refresh => r#"<path d="M3 10a9 9 0 1 1 2 8M3 3v7h7"/>"#,
        Icon::Network => {
            r#"<circle cx="12" cy="4" r="2"/><circle cx="4" cy="19" r="2"/><circle cx="20" cy="19" r="2"/><circle cx="12" cy="13" r="2"/><path d="M12 6v5m-2 4-4 3m8-3 4 3"/>"#
        }
        Icon::Text => r#"<path d="M5 2h10l4 4v16H5ZM15 2v5h4M8 11h8m-8 4h8m-8 3h5"/>"#,

        Icon::Sparkles => r#"<path d="m12 2 3 7 7 3-7 3-3 7-3-7-7-3 7-3Z"/>"#,
        Icon::Pause => r#"<path d="M8 5v14M16 5v14"/>"#,
        Icon::External => r#"<path d="M14 3h7v7m0-7L10 14M9 3H3v18h18v-6"/>"#,
        Icon::Pin => {
            r#"<path d="M19 9c0 6-7 13-7 13S5 15 5 9a7 7 0 0 1 14 0Z"/><circle cx="12" cy="9" r="2"/>"#
        }
        Icon::Camera => r#"<path d="M3 7h5l2-3h4l2 3h5v14H3Z"/><circle cx="12" cy="14" r="4"/>"#,
        Icon::Save => r#"<path d="M4 3h14l3 3v15H3V3Zm3 0v6h10V3M7 21v-8h10v8"/>"#,
        Icon::Globe => {
            r#"<circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18"/>"#
        }
        Icon::Play => r#"<path d="m8 4 12 8-12 8Z"/>"#,
    };
    let markup = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#53617f" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"##
    );
    svg(svg::Handle::from_memory(markup.into_bytes()))
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}
