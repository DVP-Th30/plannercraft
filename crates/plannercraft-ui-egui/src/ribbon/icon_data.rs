//! Icônes du ruban (H-UI-77, ADR-0017 D6) : Lucide 1.55.0 (ISC ; dérivées de
//! Feather : MIT), `assets/icons/lucide/` (licences : `LICENSE` du même dossier).
//!
//! Fichier généré par `cargo xtask icons` : ne pas modifier à la main
//! (fraîcheur vérifiée par `h_ui_77_icon_data_fresh`). Coordonnées en
//! 1/64 d'unité de la grille 24 de Lucide ; trait de 2 unités, bouts ronds.

/// Polyligne d'une icône (1/64 d'unité de grille).
#[derive(Debug)]
pub struct IconLine {
    /// Fermée (dernier point relié au premier).
    pub closed: bool,
    /// Points, au moins deux.
    pub points: &'static [(i16, i16)],
}

/// Formes d'une icône : polylignes et points isolés (disques).
#[derive(Debug)]
pub struct IconData {
    /// Polylignes.
    pub lines: &'static [IconLine],
    /// Points isolés.
    pub dots: &'static [(i16, i16)],
}

/// Icône du ruban : un variant par fichier de `assets/icons/lucide/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Icon {
    /// `arrow-down-to-line.svg`.
    ArrowDownToLine,
    /// `arrow-up-down.svg`.
    ArrowUpDown,
    /// `between-vertical-start.svg`.
    BetweenVerticalStart,
    /// `calendar-check.svg`.
    CalendarCheck,
    /// `calendar-cog.svg`.
    CalendarCog,
    /// `calendar-range.svg`.
    CalendarRange,
    /// `chart-column.svg`.
    ChartColumn,
    /// `chart-gantt.svg`.
    ChartGantt,
    /// `chart-no-axes-gantt.svg`.
    ChartNoAxesGantt,
    /// `chevrons-down-up.svg`.
    ChevronsDownUp,
    /// `chevrons-up-down.svg`.
    ChevronsUpDown,
    /// `circle-off.svg`.
    CircleOff,
    /// `circle-x.svg`.
    CircleX,
    /// `clipboard-list.svg`.
    ClipboardList,
    /// `clipboard-paste.svg`.
    ClipboardPaste,
    /// `columns-3.svg`.
    Columns3,
    /// `columns-3-cog.svg`.
    Columns3Cog,
    /// `copy.svg`.
    Copy,
    /// `diamond.svg`.
    Diamond,
    /// `eraser.svg`.
    Eraser,
    /// `file-cog.svg`.
    FileCog,
    /// `file-down.svg`.
    FileDown,
    /// `file-text.svg`.
    FileText,
    /// `filter.svg`.
    Filter,
    /// `indent-decrease.svg`.
    IndentDecrease,
    /// `indent-increase.svg`.
    IndentIncrease,
    /// `info.svg`.
    Info,
    /// `languages.svg`.
    Languages,
    /// `link.svg`.
    Link,
    /// `list-plus.svg`.
    ListPlus,
    /// `list-tree.svg`.
    ListTree,
    /// `locate.svg`.
    Locate,
    /// `maximize.svg`.
    Maximize,
    /// `notebook-pen.svg`.
    NotebookPen,
    /// `paintbrush.svg`.
    Paintbrush,
    /// `panel-left-close.svg`.
    PanelLeftClose,
    /// `pin.svg`.
    Pin,
    /// `printer.svg`.
    Printer,
    /// `redo-2.svg`.
    Redo2,
    /// `refresh-cw.svg`.
    RefreshCw,
    /// `save.svg`.
    Save,
    /// `scissors.svg`.
    Scissors,
    /// `search.svg`.
    Search,
    /// `sun-moon.svg`.
    SunMoon,
    /// `swatch-book.svg`.
    SwatchBook,
    /// `tags.svg`.
    Tags,
    /// `timeline.svg`.
    Timeline,
    /// `triangle-alert.svg`.
    TriangleAlert,
    /// `undo-2.svg`.
    Undo2,
    /// `unlink.svg`.
    Unlink,
    /// `user-plus.svg`.
    UserPlus,
    /// `users.svg`.
    Users,
    /// `zoom-in.svg`.
    ZoomIn,
    /// `zoom-out.svg`.
    ZoomOut,
}

impl Icon {
    /// Toutes les icônes, dans l'ordre des fichiers.
    pub const ALL: [Self; 54] = [
        Self::ArrowDownToLine,
        Self::ArrowUpDown,
        Self::BetweenVerticalStart,
        Self::CalendarCheck,
        Self::CalendarCog,
        Self::CalendarRange,
        Self::ChartColumn,
        Self::ChartGantt,
        Self::ChartNoAxesGantt,
        Self::ChevronsDownUp,
        Self::ChevronsUpDown,
        Self::CircleOff,
        Self::CircleX,
        Self::ClipboardList,
        Self::ClipboardPaste,
        Self::Columns3,
        Self::Columns3Cog,
        Self::Copy,
        Self::Diamond,
        Self::Eraser,
        Self::FileCog,
        Self::FileDown,
        Self::FileText,
        Self::Filter,
        Self::IndentDecrease,
        Self::IndentIncrease,
        Self::Info,
        Self::Languages,
        Self::Link,
        Self::ListPlus,
        Self::ListTree,
        Self::Locate,
        Self::Maximize,
        Self::NotebookPen,
        Self::Paintbrush,
        Self::PanelLeftClose,
        Self::Pin,
        Self::Printer,
        Self::Redo2,
        Self::RefreshCw,
        Self::Save,
        Self::Scissors,
        Self::Search,
        Self::SunMoon,
        Self::SwatchBook,
        Self::Tags,
        Self::Timeline,
        Self::TriangleAlert,
        Self::Undo2,
        Self::Unlink,
        Self::UserPlus,
        Self::Users,
        Self::ZoomIn,
        Self::ZoomOut,
    ];

    /// Nom du fichier Lucide (sans `.svg`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ArrowDownToLine => "arrow-down-to-line",
            Self::ArrowUpDown => "arrow-up-down",
            Self::BetweenVerticalStart => "between-vertical-start",
            Self::CalendarCheck => "calendar-check",
            Self::CalendarCog => "calendar-cog",
            Self::CalendarRange => "calendar-range",
            Self::ChartColumn => "chart-column",
            Self::ChartGantt => "chart-gantt",
            Self::ChartNoAxesGantt => "chart-no-axes-gantt",
            Self::ChevronsDownUp => "chevrons-down-up",
            Self::ChevronsUpDown => "chevrons-up-down",
            Self::CircleOff => "circle-off",
            Self::CircleX => "circle-x",
            Self::ClipboardList => "clipboard-list",
            Self::ClipboardPaste => "clipboard-paste",
            Self::Columns3 => "columns-3",
            Self::Columns3Cog => "columns-3-cog",
            Self::Copy => "copy",
            Self::Diamond => "diamond",
            Self::Eraser => "eraser",
            Self::FileCog => "file-cog",
            Self::FileDown => "file-down",
            Self::FileText => "file-text",
            Self::Filter => "filter",
            Self::IndentDecrease => "indent-decrease",
            Self::IndentIncrease => "indent-increase",
            Self::Info => "info",
            Self::Languages => "languages",
            Self::Link => "link",
            Self::ListPlus => "list-plus",
            Self::ListTree => "list-tree",
            Self::Locate => "locate",
            Self::Maximize => "maximize",
            Self::NotebookPen => "notebook-pen",
            Self::Paintbrush => "paintbrush",
            Self::PanelLeftClose => "panel-left-close",
            Self::Pin => "pin",
            Self::Printer => "printer",
            Self::Redo2 => "redo-2",
            Self::RefreshCw => "refresh-cw",
            Self::Save => "save",
            Self::Scissors => "scissors",
            Self::Search => "search",
            Self::SunMoon => "sun-moon",
            Self::SwatchBook => "swatch-book",
            Self::Tags => "tags",
            Self::Timeline => "timeline",
            Self::TriangleAlert => "triangle-alert",
            Self::Undo2 => "undo-2",
            Self::Unlink => "unlink",
            Self::UserPlus => "user-plus",
            Self::Users => "users",
            Self::ZoomIn => "zoom-in",
            Self::ZoomOut => "zoom-out",
        }
    }

    /// Formes de l'icône.
    #[must_use]
    pub const fn data(self) -> &'static IconData {
        match self {
            Self::ArrowDownToLine => &ARROW_DOWN_TO_LINE,
            Self::ArrowUpDown => &ARROW_UP_DOWN,
            Self::BetweenVerticalStart => &BETWEEN_VERTICAL_START,
            Self::CalendarCheck => &CALENDAR_CHECK,
            Self::CalendarCog => &CALENDAR_COG,
            Self::CalendarRange => &CALENDAR_RANGE,
            Self::ChartColumn => &CHART_COLUMN,
            Self::ChartGantt => &CHART_GANTT,
            Self::ChartNoAxesGantt => &CHART_NO_AXES_GANTT,
            Self::ChevronsDownUp => &CHEVRONS_DOWN_UP,
            Self::ChevronsUpDown => &CHEVRONS_UP_DOWN,
            Self::CircleOff => &CIRCLE_OFF,
            Self::CircleX => &CIRCLE_X,
            Self::ClipboardList => &CLIPBOARD_LIST,
            Self::ClipboardPaste => &CLIPBOARD_PASTE,
            Self::Columns3 => &COLUMNS_3,
            Self::Columns3Cog => &COLUMNS_3_COG,
            Self::Copy => &COPY,
            Self::Diamond => &DIAMOND,
            Self::Eraser => &ERASER,
            Self::FileCog => &FILE_COG,
            Self::FileDown => &FILE_DOWN,
            Self::FileText => &FILE_TEXT,
            Self::Filter => &FILTER,
            Self::IndentDecrease => &INDENT_DECREASE,
            Self::IndentIncrease => &INDENT_INCREASE,
            Self::Info => &INFO,
            Self::Languages => &LANGUAGES,
            Self::Link => &LINK,
            Self::ListPlus => &LIST_PLUS,
            Self::ListTree => &LIST_TREE,
            Self::Locate => &LOCATE,
            Self::Maximize => &MAXIMIZE,
            Self::NotebookPen => &NOTEBOOK_PEN,
            Self::Paintbrush => &PAINTBRUSH,
            Self::PanelLeftClose => &PANEL_LEFT_CLOSE,
            Self::Pin => &PIN,
            Self::Printer => &PRINTER,
            Self::Redo2 => &REDO_2,
            Self::RefreshCw => &REFRESH_CW,
            Self::Save => &SAVE,
            Self::Scissors => &SCISSORS,
            Self::Search => &SEARCH,
            Self::SunMoon => &SUN_MOON,
            Self::SwatchBook => &SWATCH_BOOK,
            Self::Tags => &TAGS,
            Self::Timeline => &TIMELINE,
            Self::TriangleAlert => &TRIANGLE_ALERT,
            Self::Undo2 => &UNDO_2,
            Self::Unlink => &UNLINK,
            Self::UserPlus => &USER_PLUS,
            Self::Users => &USERS,
            Self::ZoomIn => &ZOOM_IN,
            Self::ZoomOut => &ZOOM_OUT,
        }
    }
}

/// `arrow-down-to-line.svg`.
static ARROW_DOWN_TO_LINE: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (768, 1088), (768, 192),
        ] },
        IconLine { closed: false, points: &[
            (384, 704), (768, 1088), (1152, 704),
        ] },
        IconLine { closed: false, points: &[
            (1216, 1344), (320, 1344),
        ] },
    ],
    dots: &[],
};

/// `arrow-up-down.svg`.
static ARROW_UP_DOWN: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (1344, 1024), (1088, 1280), (832, 1024),
        ] },
        IconLine { closed: false, points: &[
            (1088, 1280), (1088, 256),
        ] },
        IconLine { closed: false, points: &[
            (192, 512), (448, 256), (704, 512),
        ] },
        IconLine { closed: false, points: &[
            (448, 256), (448, 1280),
        ] },
    ],
    dots: &[],
};

/// `between-vertical-start.svg`.
static BETWEEN_VERTICAL_START: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (192, 576), (195, 556), (204, 538), (218, 524), (236, 515), (256, 512), (576, 512), (596, 515), (614, 524), (628, 538),
            (637, 556), (640, 576), (640, 1280), (637, 1300), (628, 1318), (614, 1332), (596, 1341), (576, 1344), (256, 1344), (236, 1341),
            (218, 1332), (204, 1318), (195, 1300), (192, 1280),
        ] },
        IconLine { closed: false, points: &[
            (960, 128), (768, 320), (576, 128),
        ] },
        IconLine { closed: true, points: &[
            (896, 576), (899, 556), (908, 538), (922, 524), (940, 515), (960, 512), (1280, 512), (1300, 515), (1318, 524), (1332, 538),
            (1341, 556), (1344, 576), (1344, 1280), (1341, 1300), (1332, 1318), (1318, 1332), (1300, 1341), (1280, 1344), (960, 1344), (940, 1341),
            (922, 1332), (908, 1318), (899, 1300), (896, 1280),
        ] },
    ],
    dots: &[],
};

/// `calendar-check.svg`.
static CALENDAR_CHECK: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (512, 128), (512, 320),
        ] },
        IconLine { closed: false, points: &[
            (1024, 128), (1024, 320),
        ] },
        IconLine { closed: true, points: &[
            (192, 320), (197, 286), (209, 256), (229, 229), (256, 209), (286, 197), (320, 192), (1216, 192), (1250, 197), (1280, 209),
            (1307, 229), (1327, 256), (1339, 286), (1344, 320), (1344, 1216), (1339, 1250), (1327, 1280), (1307, 1307), (1280, 1327), (1250, 1339),
            (1216, 1344), (320, 1344), (286, 1339), (256, 1327), (229, 1307), (209, 1280), (197, 1250), (192, 1216),
        ] },
        IconLine { closed: false, points: &[
            (192, 576), (1344, 576),
        ] },
        IconLine { closed: false, points: &[
            (576, 960), (704, 1088), (960, 832),
        ] },
    ],
    dots: &[],
};

/// `calendar-cog.svg`.
static CALENDAR_COG: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (975, 1079), (916, 1054),
        ] },
        IconLine { closed: false, points: &[
            (975, 1225), (916, 1250),
        ] },
        IconLine { closed: false, points: &[
            (1024, 128), (1024, 320),
        ] },
        IconLine { closed: false, points: &[
            (1054, 916), (1079, 975),
        ] },
        IconLine { closed: false, points: &[
            (1079, 1329), (1054, 1389),
        ] },
        IconLine { closed: false, points: &[
            (1225, 975), (1250, 916),
        ] },
        IconLine { closed: false, points: &[
            (1250, 1389), (1225, 1329),
        ] },
        IconLine { closed: false, points: &[
            (1329, 1079), (1389, 1054),
        ] },
        IconLine { closed: false, points: &[
            (1329, 1225), (1389, 1250),
        ] },
        IconLine { closed: false, points: &[
            (1344, 672), (1344, 320), (1339, 286), (1327, 256), (1307, 229), (1280, 209), (1250, 197), (1216, 192), (320, 192), (286, 197),
            (256, 209), (229, 229), (209, 256), (197, 286), (192, 320), (192, 1216), (197, 1250), (209, 1280), (229, 1307), (256, 1327),
            (286, 1339), (320, 1344), (672, 1344),
        ] },
        IconLine { closed: false, points: &[
            (192, 576), (1344, 576),
        ] },
        IconLine { closed: false, points: &[
            (512, 128), (512, 320),
        ] },
        IconLine { closed: true, points: &[
            (1344, 1152), (1340, 1190), (1329, 1225), (1312, 1259), (1288, 1288), (1259, 1312), (1225, 1329), (1190, 1340), (1152, 1344), (1114, 1340),
            (1079, 1329), (1045, 1312), (1016, 1288), (992, 1259), (975, 1225), (964, 1190), (960, 1152), (964, 1114), (975, 1079), (992, 1045),
            (1016, 1016), (1045, 992), (1079, 975), (1114, 964), (1152, 960), (1190, 964), (1225, 975), (1259, 992), (1288, 1016), (1312, 1045),
            (1329, 1079), (1340, 1114),
        ] },
    ],
    dots: &[],
};

/// `calendar-range.svg`.
static CALENDAR_RANGE: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (192, 320), (197, 286), (209, 256), (229, 229), (256, 209), (286, 197), (320, 192), (1216, 192), (1250, 197), (1280, 209),
            (1307, 229), (1327, 256), (1339, 286), (1344, 320), (1344, 1216), (1339, 1250), (1327, 1280), (1307, 1307), (1280, 1327), (1250, 1339),
            (1216, 1344), (320, 1344), (286, 1339), (256, 1327), (229, 1307), (209, 1280), (197, 1250), (192, 1216),
        ] },
        IconLine { closed: false, points: &[
            (1024, 128), (1024, 320),
        ] },
        IconLine { closed: false, points: &[
            (192, 576), (1344, 576),
        ] },
        IconLine { closed: false, points: &[
            (512, 128), (512, 320),
        ] },
        IconLine { closed: false, points: &[
            (1088, 832), (704, 832),
        ] },
        IconLine { closed: false, points: &[
            (832, 1088), (448, 1088),
        ] },
    ],
    dots: &[
        (448, 832), (1088, 1088),
    ],
};

/// `chart-column.svg`.
static CHART_COLUMN: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (192, 192), (192, 1216), (197, 1250), (209, 1280), (229, 1307), (256, 1327), (286, 1339), (320, 1344), (1344, 1344),
        ] },
        IconLine { closed: false, points: &[
            (1152, 1088), (1152, 576),
        ] },
        IconLine { closed: false, points: &[
            (832, 1088), (832, 320),
        ] },
        IconLine { closed: false, points: &[
            (512, 1088), (512, 896),
        ] },
    ],
    dots: &[],
};

/// `chart-gantt.svg`.
static CHART_GANTT: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (640, 384), (1152, 384),
        ] },
        IconLine { closed: false, points: &[
            (768, 1024), (1152, 1024),
        ] },
        IconLine { closed: false, points: &[
            (192, 192), (192, 1216), (197, 1250), (209, 1280), (229, 1307), (256, 1327), (286, 1339), (320, 1344), (1344, 1344),
        ] },
        IconLine { closed: false, points: &[
            (512, 704), (960, 704),
        ] },
    ],
    dots: &[],
};

/// `chart-no-axes-gantt.svg`.
static CHART_NO_AXES_GANTT: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (384, 320), (1152, 320),
        ] },
        IconLine { closed: false, points: &[
            (256, 768), (896, 768),
        ] },
        IconLine { closed: false, points: &[
            (768, 1216), (1280, 1216),
        ] },
    ],
    dots: &[],
};

/// `chevrons-down-up.svg`.
static CHEVRONS_DOWN_UP: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (448, 1280), (768, 960), (1088, 1280),
        ] },
        IconLine { closed: false, points: &[
            (448, 256), (768, 576), (1088, 256),
        ] },
    ],
    dots: &[],
};

/// `chevrons-up-down.svg`.
static CHEVRONS_UP_DOWN: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (448, 960), (768, 1280), (1088, 960),
        ] },
        IconLine { closed: false, points: &[
            (448, 576), (768, 256), (1088, 576),
        ] },
    ],
    dots: &[],
};

/// `circle-off.svg`.
static CIRCLE_OFF: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (128, 128), (1408, 1408),
        ] },
        IconLine { closed: false, points: &[
            (534, 172), (606, 149), (678, 135), (752, 128), (825, 131), (899, 142), (970, 161), (1039, 189), (1104, 224), (1164, 266),
            (1220, 316), (1270, 372), (1312, 432), (1347, 497), (1374, 566), (1394, 637), (1405, 711), (1407, 784), (1401, 858), (1386, 930),
            (1363, 1002),
        ] },
        IconLine { closed: false, points: &[
            (1221, 1221), (1166, 1270), (1108, 1311), (1046, 1345), (979, 1373), (911, 1393), (840, 1405), (768, 1409), (696, 1405), (625, 1393),
            (557, 1373), (490, 1345), (428, 1311), (370, 1270), (315, 1221), (266, 1166), (225, 1108), (191, 1046), (163, 979), (143, 911),
            (131, 840), (127, 768), (131, 696), (143, 625), (163, 557), (191, 490), (225, 428), (266, 370), (315, 315),
        ] },
    ],
    dots: &[],
};

/// `circle-x.svg`.
static CIRCLE_X: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (1408, 768), (1404, 842), (1392, 912), (1372, 979), (1345, 1046), (1310, 1108), (1268, 1167), (1220, 1220), (1167, 1268), (1108, 1310),
            (1046, 1345), (979, 1372), (912, 1392), (842, 1404), (768, 1408), (694, 1404), (624, 1392), (557, 1372), (490, 1345), (428, 1310),
            (369, 1268), (316, 1220), (268, 1167), (226, 1108), (191, 1046), (164, 979), (144, 912), (132, 842), (128, 768), (132, 694),
            (144, 624), (164, 557), (191, 490), (226, 428), (268, 369), (316, 316), (369, 268), (428, 226), (490, 191), (557, 164),
            (624, 144), (694, 132), (768, 128), (842, 132), (912, 144), (979, 164), (1046, 191), (1108, 226), (1167, 268), (1220, 316),
            (1268, 369), (1310, 428), (1345, 490), (1372, 557), (1392, 624), (1404, 694),
        ] },
        IconLine { closed: false, points: &[
            (960, 576), (576, 960),
        ] },
        IconLine { closed: false, points: &[
            (576, 576), (960, 960),
        ] },
    ],
    dots: &[],
};

/// `clipboard-list.svg`.
static CLIPBOARD_LIST: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (512, 192), (515, 172), (524, 154), (538, 140), (556, 131), (576, 128), (960, 128), (980, 131), (998, 140), (1012, 154),
            (1021, 172), (1024, 192), (1024, 320), (1021, 340), (1012, 358), (998, 372), (980, 381), (960, 384), (576, 384), (556, 381),
            (538, 372), (524, 358), (515, 340), (512, 320),
        ] },
        IconLine { closed: false, points: &[
            (1024, 256), (1152, 256), (1186, 261), (1216, 273), (1243, 293), (1263, 320), (1275, 350), (1280, 384), (1280, 1280), (1275, 1314),
            (1263, 1344), (1243, 1371), (1216, 1391), (1186, 1403), (1152, 1408), (384, 1408), (350, 1403), (320, 1391), (293, 1371), (273, 1344),
            (261, 1314), (256, 1280), (256, 384), (261, 350), (273, 320), (293, 293), (320, 273), (350, 261), (384, 256), (512, 256),
        ] },
        IconLine { closed: false, points: &[
            (768, 704), (1024, 704),
        ] },
        IconLine { closed: false, points: &[
            (768, 1024), (1024, 1024),
        ] },
    ],
    dots: &[
        (512, 704), (512, 1024),
    ],
};

/// `clipboard-paste.svg`.
static CLIPBOARD_PASTE: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (704, 896), (1344, 896),
        ] },
        IconLine { closed: false, points: &[
            (1024, 256), (1152, 256), (1186, 261), (1216, 273), (1243, 293), (1263, 320), (1275, 350), (1280, 384), (1280, 470),
        ] },
        IconLine { closed: false, points: &[
            (1088, 1152), (1344, 896), (1088, 640),
        ] },
        IconLine { closed: false, points: &[
            (512, 256), (384, 256), (350, 261), (320, 273), (293, 293), (273, 320), (261, 350), (256, 384), (256, 1280), (261, 1314),
            (273, 1344), (293, 1371), (320, 1391), (350, 1403), (384, 1408), (1152, 1408), (1180, 1405), (1207, 1396), (1231, 1381), (1251, 1361),
            (1267, 1337),
        ] },
        IconLine { closed: true, points: &[
            (512, 192), (515, 172), (524, 154), (538, 140), (556, 131), (576, 128), (960, 128), (980, 131), (998, 140), (1012, 154),
            (1021, 172), (1024, 192), (1024, 320), (1021, 340), (1012, 358), (998, 372), (980, 381), (960, 384), (576, 384), (556, 381),
            (538, 372), (524, 358), (515, 340), (512, 320),
        ] },
    ],
    dots: &[],
};

/// `columns-3.svg`.
static COLUMNS_3: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (192, 320), (197, 286), (209, 256), (229, 229), (256, 209), (286, 197), (320, 192), (1216, 192), (1250, 197), (1280, 209),
            (1307, 229), (1327, 256), (1339, 286), (1344, 320), (1344, 1216), (1339, 1250), (1327, 1280), (1307, 1307), (1280, 1327), (1250, 1339),
            (1216, 1344), (320, 1344), (286, 1339), (256, 1327), (229, 1307), (209, 1280), (197, 1250), (192, 1216),
        ] },
        IconLine { closed: false, points: &[
            (576, 192), (576, 1344),
        ] },
        IconLine { closed: false, points: &[
            (960, 192), (960, 1344),
        ] },
    ],
    dots: &[],
};

/// `columns-3-cog.svg`.
static COLUMNS_3_COG: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (678, 1344), (320, 1344), (286, 1339), (256, 1327), (229, 1307), (209, 1280), (197, 1250), (192, 1216), (192, 320), (197, 286),
            (209, 256), (229, 229), (256, 209), (286, 197), (320, 192), (1216, 192), (1250, 197), (1280, 209), (1307, 229), (1327, 256),
            (1339, 286), (1344, 320), (1344, 678),
        ] },
        IconLine { closed: false, points: &[
            (916, 1250), (975, 1225),
        ] },
        IconLine { closed: false, points: &[
            (960, 192), (960, 678),
        ] },
        IconLine { closed: false, points: &[
            (975, 1079), (916, 1054),
        ] },
        IconLine { closed: false, points: &[
            (1079, 975), (1054, 916),
        ] },
        IconLine { closed: false, points: &[
            (1079, 1329), (1054, 1389),
        ] },
        IconLine { closed: false, points: &[
            (1225, 975), (1250, 916),
        ] },
        IconLine { closed: false, points: &[
            (1250, 1389), (1225, 1329),
        ] },
        IconLine { closed: false, points: &[
            (1329, 1079), (1388, 1054),
        ] },
        IconLine { closed: false, points: &[
            (1329, 1225), (1388, 1250),
        ] },
        IconLine { closed: false, points: &[
            (576, 192), (576, 1344),
        ] },
        IconLine { closed: true, points: &[
            (1344, 1152), (1340, 1190), (1329, 1225), (1312, 1259), (1288, 1288), (1259, 1312), (1225, 1329), (1190, 1340), (1152, 1344), (1114, 1340),
            (1079, 1329), (1045, 1312), (1016, 1288), (992, 1259), (975, 1225), (964, 1190), (960, 1152), (964, 1114), (975, 1079), (992, 1045),
            (1016, 1016), (1045, 992), (1079, 975), (1114, 964), (1152, 960), (1190, 964), (1225, 975), (1259, 992), (1288, 1016), (1312, 1045),
            (1329, 1079), (1340, 1114),
        ] },
    ],
    dots: &[],
};

/// `copy.svg`.
static COPY: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (512, 640), (517, 606), (529, 576), (549, 549), (576, 529), (606, 517), (640, 512), (1280, 512), (1314, 517), (1344, 529),
            (1371, 549), (1391, 576), (1403, 606), (1408, 640), (1408, 1280), (1403, 1314), (1391, 1344), (1371, 1371), (1344, 1391), (1314, 1403),
            (1280, 1408), (640, 1408), (606, 1403), (576, 1391), (549, 1371), (529, 1344), (517, 1314), (512, 1280),
        ] },
        IconLine { closed: false, points: &[
            (256, 1024), (222, 1019), (192, 1007), (166, 986), (145, 960), (133, 930), (128, 896), (128, 256), (133, 222), (145, 192),
            (166, 166), (192, 145), (222, 133), (256, 128), (896, 128), (930, 133), (960, 145), (986, 166), (1007, 192), (1019, 222),
            (1024, 256),
        ] },
    ],
    dots: &[],
};

/// `diamond.svg`.
static DIAMOND: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (173, 659), (153, 683), (139, 709), (131, 738), (128, 768), (131, 799), (139, 827), (153, 854), (173, 877), (659, 1363),
            (682, 1383), (709, 1397), (737, 1405), (768, 1408), (798, 1405), (827, 1397), (853, 1383), (877, 1363), (1363, 877), (1382, 854),
            (1396, 827), (1405, 799), (1408, 768), (1405, 738), (1396, 709), (1382, 683), (1363, 659), (877, 173), (853, 154), (827, 140),
            (798, 131), (768, 128), (737, 131), (709, 140), (682, 154), (659, 173),
        ] },
    ],
    dots: &[],
};

/// `eraser.svg`.
static ERASER: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (1344, 1344), (512, 1344), (479, 1340), (448, 1327), (421, 1306), (166, 1050), (145, 1023), (132, 993), (128, 960), (132, 927),
            (145, 897), (166, 870), (806, 230), (832, 209), (863, 196), (896, 192), (929, 196), (960, 209), (987, 230), (1370, 614),
            (1391, 641), (1404, 671), (1408, 704), (1404, 737), (1391, 767), (1370, 794), (821, 1344),
        ] },
        IconLine { closed: false, points: &[
            (325, 710), (890, 1275),
        ] },
    ],
    dots: &[],
};

/// `file-cog.svg`.
static FILE_COG: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (960, 512), (940, 509), (922, 500), (908, 486), (899, 468), (896, 448), (896, 128), (926, 131), (955, 140), (982, 154),
            (1005, 173), (1235, 403), (1254, 426), (1268, 453), (1277, 482), (1280, 512),
        ] },
        IconLine { closed: false, points: &[
            (1280, 512), (1280, 1280), (1275, 1314), (1263, 1344), (1243, 1371), (1216, 1391), (1186, 1403), (1152, 1408), (884, 1408),
        ] },
        IconLine { closed: false, points: &[
            (212, 1250), (271, 1225),
        ] },
        IconLine { closed: false, points: &[
            (256, 678), (256, 256), (261, 222), (273, 192), (293, 165), (320, 145), (350, 133), (384, 128), (896, 128),
        ] },
        IconLine { closed: false, points: &[
            (271, 1079), (211, 1054),
        ] },
        IconLine { closed: false, points: &[
            (375, 975), (350, 916),
        ] },
        IconLine { closed: false, points: &[
            (375, 1329), (350, 1389),
        ] },
        IconLine { closed: false, points: &[
            (521, 975), (546, 916),
        ] },
        IconLine { closed: false, points: &[
            (546, 1389), (521, 1329),
        ] },
        IconLine { closed: false, points: &[
            (625, 1079), (684, 1054),
        ] },
        IconLine { closed: false, points: &[
            (625, 1225), (684, 1250),
        ] },
        IconLine { closed: true, points: &[
            (640, 1152), (636, 1190), (625, 1225), (608, 1259), (584, 1288), (555, 1312), (521, 1329), (486, 1340), (448, 1344), (410, 1340),
            (375, 1329), (341, 1312), (312, 1288), (288, 1259), (271, 1225), (260, 1190), (256, 1152), (260, 1114), (271, 1079), (288, 1045),
            (312, 1016), (341, 992), (375, 975), (410, 964), (448, 960), (486, 964), (521, 975), (555, 992), (584, 1016), (608, 1045),
            (625, 1079), (636, 1114),
        ] },
    ],
    dots: &[],
};

/// `file-down.svg`.
static FILE_DOWN: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (384, 1408), (350, 1403), (320, 1391), (293, 1371), (273, 1344), (261, 1314), (256, 1280), (256, 256), (261, 222), (273, 192),
            (293, 165), (320, 145), (350, 133), (384, 128), (896, 128), (926, 131), (955, 140), (982, 154), (1005, 173), (1235, 403),
            (1254, 426), (1268, 453), (1277, 482), (1280, 512), (1280, 1280), (1275, 1314), (1263, 1344), (1243, 1371), (1216, 1391), (1186, 1403),
            (1152, 1408),
        ] },
        IconLine { closed: false, points: &[
            (896, 128), (896, 448), (899, 468), (908, 486), (922, 500), (940, 509), (960, 512), (1280, 512),
        ] },
        IconLine { closed: false, points: &[
            (768, 1152), (768, 768),
        ] },
        IconLine { closed: false, points: &[
            (576, 960), (768, 1152), (960, 960),
        ] },
    ],
    dots: &[],
};

/// `file-text.svg`.
static FILE_TEXT: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (384, 1408), (350, 1403), (320, 1391), (293, 1371), (273, 1344), (261, 1314), (256, 1280), (256, 256), (261, 222), (273, 192),
            (293, 165), (320, 145), (350, 133), (384, 128), (896, 128), (926, 131), (955, 140), (982, 154), (1005, 173), (1235, 403),
            (1254, 426), (1268, 453), (1277, 482), (1280, 512), (1280, 1280), (1275, 1314), (1263, 1344), (1243, 1371), (1216, 1391), (1186, 1403),
            (1152, 1408),
        ] },
        IconLine { closed: false, points: &[
            (896, 128), (896, 448), (899, 468), (908, 486), (922, 500), (940, 509), (960, 512), (1280, 512),
        ] },
        IconLine { closed: false, points: &[
            (640, 576), (512, 576),
        ] },
        IconLine { closed: false, points: &[
            (1024, 832), (512, 832),
        ] },
        IconLine { closed: false, points: &[
            (1024, 1088), (512, 1088),
        ] },
    ],
    dots: &[],
};

/// `filter.svg`.
static FILTER: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (640, 1280), (644, 1303), (657, 1323), (675, 1337), (803, 1401), (824, 1408), (846, 1407), (866, 1398), (882, 1384), (892, 1365),
            (896, 1344), (896, 896), (900, 865), (911, 836), (929, 810), (1391, 299), (1402, 283), (1407, 266), (1407, 248), (1402, 230),
            (1392, 214), (1379, 202), (1362, 195), (1344, 192), (192, 192), (174, 195), (157, 202), (144, 214), (134, 230), (129, 248),
            (129, 266), (134, 283), (145, 299), (607, 810), (625, 836), (636, 865), (640, 896),
        ] },
    ],
    dots: &[],
};

/// `indent-decrease.svg`.
static INDENT_DECREASE: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (1344, 320), (704, 320),
        ] },
        IconLine { closed: false, points: &[
            (1344, 768), (704, 768),
        ] },
        IconLine { closed: false, points: &[
            (1344, 1216), (704, 1216),
        ] },
        IconLine { closed: false, points: &[
            (448, 512), (192, 768), (448, 1024),
        ] },
    ],
    dots: &[],
};

/// `indent-increase.svg`.
static INDENT_INCREASE: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (1344, 320), (704, 320),
        ] },
        IconLine { closed: false, points: &[
            (1344, 768), (704, 768),
        ] },
        IconLine { closed: false, points: &[
            (1344, 1216), (704, 1216),
        ] },
        IconLine { closed: false, points: &[
            (192, 512), (448, 768), (192, 1024),
        ] },
    ],
    dots: &[],
};

/// `info.svg`.
static INFO: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (1408, 768), (1404, 842), (1392, 912), (1372, 979), (1345, 1046), (1310, 1108), (1268, 1167), (1220, 1220), (1167, 1268), (1108, 1310),
            (1046, 1345), (979, 1372), (912, 1392), (842, 1404), (768, 1408), (694, 1404), (624, 1392), (557, 1372), (490, 1345), (428, 1310),
            (369, 1268), (316, 1220), (268, 1167), (226, 1108), (191, 1046), (164, 979), (144, 912), (132, 842), (128, 768), (132, 694),
            (144, 624), (164, 557), (191, 490), (226, 428), (268, 369), (316, 316), (369, 268), (428, 226), (490, 191), (557, 164),
            (624, 144), (694, 132), (768, 128), (842, 132), (912, 144), (979, 164), (1046, 191), (1108, 226), (1167, 268), (1220, 316),
            (1268, 369), (1310, 428), (1345, 490), (1372, 557), (1392, 624), (1404, 694),
        ] },
        IconLine { closed: false, points: &[
            (768, 1024), (768, 768),
        ] },
    ],
    dots: &[
        (768, 512),
    ],
};

/// `languages.svg`.
static LANGUAGES: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (320, 512), (704, 896),
        ] },
        IconLine { closed: false, points: &[
            (256, 896), (640, 512), (768, 320),
        ] },
        IconLine { closed: false, points: &[
            (128, 320), (896, 320),
        ] },
        IconLine { closed: false, points: &[
            (448, 128), (512, 128),
        ] },
        IconLine { closed: false, points: &[
            (1408, 1408), (1088, 768), (768, 1408),
        ] },
        IconLine { closed: false, points: &[
            (896, 1152), (1280, 1152),
        ] },
    ],
    dots: &[],
};

/// `link.svg`.
static LINK: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (640, 832), (670, 867), (704, 896), (743, 921), (784, 940), (828, 953), (873, 960), (919, 959), (964, 953), (1008, 940),
            (1050, 921), (1088, 897), (1123, 867), (1315, 675), (1347, 635), (1372, 593), (1390, 547), (1401, 499), (1404, 449), (1400, 400),
            (1388, 352), (1369, 306), (1344, 265), (1311, 226), (1272, 193), (1230, 167), (1185, 148), (1137, 137), (1087, 132), (1038, 136),
            (989, 147), (943, 165), (901, 190), (862, 222), (752, 332),
        ] },
        IconLine { closed: false, points: &[
            (896, 704), (866, 669), (832, 640), (793, 615), (752, 596), (708, 583), (663, 576), (617, 577), (572, 583), (528, 596),
            (486, 615), (448, 639), (413, 669), (221, 861), (189, 901), (164, 943), (146, 989), (135, 1037), (132, 1087), (136, 1136),
            (148, 1184), (167, 1230), (192, 1271), (225, 1310), (264, 1343), (306, 1369), (351, 1388), (399, 1399), (449, 1404), (498, 1400),
            (547, 1389), (593, 1371), (635, 1346), (674, 1314), (783, 1204),
        ] },
    ],
    dots: &[],
};

/// `list-plus.svg`.
static LIST_PLUS: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (1024, 320), (192, 320),
        ] },
        IconLine { closed: false, points: &[
            (704, 768), (192, 768),
        ] },
        IconLine { closed: false, points: &[
            (1024, 1216), (192, 1216),
        ] },
        IconLine { closed: false, points: &[
            (1152, 576), (1152, 960),
        ] },
        IconLine { closed: false, points: &[
            (1344, 768), (960, 768),
        ] },
    ],
    dots: &[],
};

/// `list-tree.svg`.
static LIST_TREE: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (512, 320), (1344, 320),
        ] },
        IconLine { closed: false, points: &[
            (832, 768), (1344, 768),
        ] },
        IconLine { closed: false, points: &[
            (832, 1216), (1344, 1216),
        ] },
        IconLine { closed: false, points: &[
            (192, 640), (197, 674), (209, 704), (229, 731), (256, 751), (286, 763), (320, 768), (512, 768),
        ] },
        IconLine { closed: false, points: &[
            (192, 320), (192, 1088), (197, 1122), (209, 1152), (229, 1179), (256, 1199), (286, 1211), (320, 1216), (512, 1216),
        ] },
    ],
    dots: &[],
};

/// `locate.svg`.
static LOCATE: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (128, 768), (320, 768),
        ] },
        IconLine { closed: false, points: &[
            (1216, 768), (1408, 768),
        ] },
        IconLine { closed: false, points: &[
            (768, 128), (768, 320),
        ] },
        IconLine { closed: false, points: &[
            (768, 1216), (768, 1408),
        ] },
        IconLine { closed: true, points: &[
            (1216, 768), (1211, 833), (1198, 894), (1175, 954), (1145, 1010), (1107, 1061), (1061, 1107), (1010, 1145), (954, 1175), (894, 1198),
            (833, 1211), (768, 1216), (703, 1211), (642, 1198), (582, 1175), (526, 1145), (475, 1107), (429, 1061), (391, 1010), (361, 954),
            (338, 894), (325, 833), (320, 768), (325, 703), (338, 642), (361, 582), (391, 526), (429, 475), (475, 429), (526, 391),
            (582, 361), (642, 338), (703, 325), (768, 320), (833, 325), (894, 338), (954, 361), (1010, 391), (1061, 429), (1107, 475),
            (1145, 526), (1175, 582), (1198, 642), (1211, 703),
        ] },
    ],
    dots: &[],
};

/// `maximize.svg`.
static MAXIMIZE: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (512, 192), (320, 192), (286, 197), (256, 209), (229, 229), (209, 256), (197, 286), (192, 320), (192, 512),
        ] },
        IconLine { closed: false, points: &[
            (1344, 512), (1344, 320), (1339, 286), (1327, 256), (1307, 229), (1280, 209), (1250, 197), (1216, 192), (1024, 192),
        ] },
        IconLine { closed: false, points: &[
            (192, 1024), (192, 1216), (197, 1250), (209, 1280), (229, 1307), (256, 1327), (286, 1339), (320, 1344), (512, 1344),
        ] },
        IconLine { closed: false, points: &[
            (1024, 1344), (1216, 1344), (1250, 1339), (1280, 1327), (1307, 1307), (1327, 1280), (1339, 1250), (1344, 1216), (1344, 1024),
        ] },
    ],
    dots: &[],
};

/// `notebook-pen.svg`.
static NOTEBOOK_PEN: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (858, 128), (384, 128), (350, 133), (320, 145), (293, 165), (273, 192), (261, 222), (256, 256), (256, 1280), (261, 1314),
            (273, 1344), (293, 1371), (320, 1391), (350, 1403), (384, 1408), (1152, 1408), (1186, 1403), (1216, 1391), (1243, 1371), (1263, 1344),
            (1275, 1314), (1280, 1280), (1280, 806),
        ] },
        IconLine { closed: false, points: &[
            (128, 384), (384, 384),
        ] },
        IconLine { closed: false, points: &[
            (128, 640), (384, 640),
        ] },
        IconLine { closed: false, points: &[
            (128, 896), (384, 896),
        ] },
        IconLine { closed: false, points: &[
            (128, 1152), (384, 1152),
        ] },
        IconLine { closed: true, points: &[
            (1368, 360), (1388, 335), (1400, 309), (1407, 279), (1407, 249), (1400, 219), (1388, 192), (1368, 168), (1344, 148), (1317, 136),
            (1287, 129), (1257, 129), (1227, 136), (1201, 148), (1176, 168), (855, 489), (836, 514), (823, 543), (769, 727), (769, 744),
            (777, 758), (792, 767), (809, 767), (993, 713), (1022, 700), (1047, 681),
        ] },
    ],
    dots: &[],
};

/// `paintbrush.svg`.
static PAINTBRUSH: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (936, 1145), (252, 959),
        ] },
        IconLine { closed: true, points: &[
            (1176, 168), (1201, 148), (1227, 136), (1257, 129), (1287, 129), (1317, 136), (1344, 148), (1368, 168), (1388, 192), (1400, 219),
            (1407, 249), (1407, 279), (1400, 309), (1388, 335), (1368, 360), (1111, 617), (1103, 632), (1103, 648), (1111, 662), (1171, 723),
            (1193, 750), (1208, 781), (1216, 815), (1216, 849), (1208, 883), (1193, 913), (1171, 941), (1111, 1001), (1097, 1010), (1080, 1010),
            (1066, 1001), (535, 470), (526, 456), (526, 440), (535, 425), (595, 365), (623, 343), (653, 328), (687, 320), (721, 320),
            (755, 328), (786, 343), (813, 365), (874, 425), (888, 433), (904, 433), (919, 425),
        ] },
        IconLine { closed: false, points: &[
            (576, 512), (532, 571), (488, 617), (438, 658), (386, 691), (322, 720), (249, 744), (155, 765), (140, 772), (130, 785),
            (129, 802), (135, 817), (604, 1386), (620, 1398), (640, 1405), (660, 1405), (680, 1399), (739, 1353), (808, 1290), (878, 1216),
            (936, 1144), (979, 1082), (1006, 1031), (1020, 991), (1024, 960),
        ] },
    ],
    dots: &[],
};

/// `panel-left-close.svg`.
static PANEL_LEFT_CLOSE: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (192, 320), (197, 286), (209, 256), (229, 229), (256, 209), (286, 197), (320, 192), (1216, 192), (1250, 197), (1280, 209),
            (1307, 229), (1327, 256), (1339, 286), (1344, 320), (1344, 1216), (1339, 1250), (1327, 1280), (1307, 1307), (1280, 1327), (1250, 1339),
            (1216, 1344), (320, 1344), (286, 1339), (256, 1327), (229, 1307), (209, 1280), (197, 1250), (192, 1216),
        ] },
        IconLine { closed: false, points: &[
            (576, 192), (576, 1344),
        ] },
        IconLine { closed: false, points: &[
            (1024, 960), (832, 768), (1024, 576),
        ] },
    ],
    dots: &[],
};

/// `pin.svg`.
static PIN: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (768, 1088), (768, 1408),
        ] },
        IconLine { closed: true, points: &[
            (576, 689), (573, 717), (564, 744), (549, 768), (529, 788), (505, 803), (391, 861), (367, 876), (347, 896), (332, 920),
            (323, 947), (320, 975), (320, 1024), (323, 1044), (332, 1062), (346, 1076), (364, 1085), (384, 1088), (1152, 1088), (1172, 1085),
            (1190, 1076), (1204, 1062), (1213, 1044), (1216, 1024), (1216, 975), (1213, 947), (1204, 920), (1189, 896), (1169, 876), (1145, 861),
            (1031, 803), (1007, 788), (987, 768), (972, 744), (963, 717), (960, 689), (960, 448), (963, 428), (972, 410), (986, 396),
            (1004, 387), (1024, 384), (1058, 379), (1088, 367), (1115, 347), (1135, 320), (1147, 290), (1152, 256), (1147, 222), (1135, 192),
            (1115, 165), (1088, 145), (1058, 133), (1024, 128), (512, 128), (478, 133), (448, 145), (421, 165), (401, 192), (389, 222),
            (384, 256), (389, 290), (401, 320), (421, 347), (448, 367), (478, 379), (512, 384), (532, 387), (550, 396), (564, 410),
            (573, 428), (576, 448),
        ] },
    ],
    dots: &[],
};

/// `printer.svg`.
static PRINTER: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (384, 1152), (256, 1152), (222, 1147), (192, 1135), (165, 1115), (145, 1088), (133, 1058), (128, 1024), (128, 704), (133, 670),
            (145, 640), (165, 613), (192, 593), (222, 581), (256, 576), (1280, 576), (1314, 581), (1344, 593), (1371, 613), (1391, 640),
            (1403, 670), (1408, 704), (1408, 1024), (1403, 1058), (1391, 1088), (1371, 1115), (1344, 1135), (1314, 1147), (1280, 1152), (1152, 1152),
        ] },
        IconLine { closed: false, points: &[
            (384, 576), (384, 192), (387, 172), (396, 154), (410, 140), (428, 131), (448, 128), (1088, 128), (1108, 131), (1126, 140),
            (1140, 154), (1149, 172), (1152, 192), (1152, 576),
        ] },
        IconLine { closed: true, points: &[
            (384, 960), (387, 940), (396, 922), (410, 908), (428, 899), (448, 896), (1088, 896), (1108, 899), (1126, 908), (1140, 922),
            (1149, 940), (1152, 960), (1152, 1344), (1149, 1364), (1140, 1382), (1126, 1396), (1108, 1405), (1088, 1408), (448, 1408), (428, 1405),
            (410, 1396), (396, 1382), (387, 1364), (384, 1344),
        ] },
    ],
    dots: &[],
};

/// `redo-2.svg`.
static REDO_2: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (960, 896), (1280, 576), (960, 256),
        ] },
        IconLine { closed: false, points: &[
            (1280, 576), (608, 576), (552, 580), (499, 593), (448, 614), (401, 643), (359, 679), (323, 721), (294, 768), (273, 819),
            (260, 872), (256, 928), (260, 984), (273, 1037), (294, 1088), (323, 1135), (359, 1177), (401, 1213), (448, 1242), (499, 1263),
            (552, 1276), (608, 1280), (832, 1280),
        ] },
    ],
    dots: &[],
};

/// `refresh-cw.svg`.
static REFRESH_CW: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (192, 768), (196, 697), (209, 629), (229, 564), (258, 500), (294, 441), (337, 386), (386, 337), (441, 294), (500, 258),
            (564, 229), (629, 209), (697, 196), (768, 192), (837, 196), (903, 207), (969, 226), (1032, 252), (1092, 284), (1147, 322),
            (1199, 367), (1344, 512),
        ] },
        IconLine { closed: false, points: &[
            (1344, 192), (1344, 512), (1024, 512),
        ] },
        IconLine { closed: false, points: &[
            (1344, 768), (1340, 839), (1327, 907), (1307, 973), (1278, 1036), (1242, 1095), (1199, 1150), (1150, 1199), (1095, 1242), (1036, 1278),
            (972, 1307), (907, 1327), (839, 1340), (768, 1344), (699, 1340), (633, 1329), (567, 1310), (504, 1284), (444, 1252), (389, 1214),
            (337, 1169), (192, 1024),
        ] },
        IconLine { closed: false, points: &[
            (512, 1024), (192, 1024), (192, 1344),
        ] },
    ],
    dots: &[],
};

/// `save.svg`.
static SAVE: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (973, 192), (1006, 197), (1036, 210), (1062, 230), (1306, 474), (1326, 500), (1339, 530), (1344, 563), (1344, 1216), (1339, 1250),
            (1327, 1280), (1307, 1307), (1280, 1327), (1250, 1339), (1216, 1344), (320, 1344), (286, 1339), (256, 1327), (229, 1307), (209, 1280),
            (197, 1250), (192, 1216), (192, 320), (197, 286), (209, 256), (229, 229), (256, 209), (286, 197), (320, 192),
        ] },
        IconLine { closed: false, points: &[
            (1088, 1344), (1088, 896), (1085, 876), (1076, 858), (1062, 844), (1044, 835), (1024, 832), (512, 832), (492, 835), (474, 844),
            (460, 858), (451, 876), (448, 896), (448, 1344),
        ] },
        IconLine { closed: false, points: &[
            (448, 192), (448, 448), (451, 468), (460, 486), (474, 500), (492, 509), (512, 512), (960, 512),
        ] },
    ],
    dots: &[],
};

/// `scissors.svg`.
static SCISSORS: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (576, 384), (572, 422), (561, 457), (544, 491), (520, 520), (491, 544), (457, 561), (422, 572), (384, 576), (346, 572),
            (311, 561), (277, 544), (248, 520), (224, 491), (207, 457), (196, 422), (192, 384), (196, 346), (207, 311), (224, 277),
            (248, 248), (277, 224), (311, 207), (346, 196), (384, 192), (422, 196), (457, 207), (491, 224), (520, 248), (544, 277),
            (561, 311), (572, 346),
        ] },
        IconLine { closed: false, points: &[
            (520, 520), (768, 768),
        ] },
        IconLine { closed: false, points: &[
            (1280, 256), (520, 1016),
        ] },
        IconLine { closed: true, points: &[
            (576, 1152), (572, 1190), (561, 1225), (544, 1259), (520, 1288), (491, 1312), (457, 1329), (422, 1340), (384, 1344), (346, 1340),
            (311, 1329), (277, 1312), (248, 1288), (224, 1259), (207, 1225), (196, 1190), (192, 1152), (196, 1114), (207, 1079), (224, 1045),
            (248, 1016), (277, 992), (311, 975), (346, 964), (384, 960), (422, 964), (457, 975), (491, 992), (520, 1016), (544, 1045),
            (561, 1079), (572, 1114),
        ] },
        IconLine { closed: false, points: &[
            (947, 947), (1280, 1280),
        ] },
    ],
    dots: &[],
};

/// `search.svg`.
static SEARCH: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (1344, 1344), (1066, 1066),
        ] },
        IconLine { closed: true, points: &[
            (1216, 704), (1211, 772), (1199, 837), (1177, 900), (1147, 960), (1110, 1015), (1066, 1066), (1015, 1110), (960, 1147), (900, 1177),
            (837, 1199), (772, 1211), (704, 1216), (636, 1211), (571, 1199), (508, 1177), (448, 1147), (393, 1110), (342, 1066), (298, 1015),
            (261, 960), (231, 900), (209, 837), (197, 772), (192, 704), (197, 636), (209, 571), (231, 508), (261, 448), (298, 393),
            (342, 342), (393, 298), (448, 261), (508, 231), (571, 209), (636, 197), (704, 192), (772, 197), (837, 209), (900, 231),
            (960, 261), (1015, 298), (1066, 342), (1110, 393), (1147, 448), (1177, 508), (1199, 571), (1211, 636),
        ] },
    ],
    dots: &[],
};

/// `sun-moon.svg`.
static SUN_MOON: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (768, 128), (768, 256),
        ] },
        IconLine { closed: false, points: &[
            (950, 1049), (933, 1102), (910, 1150), (880, 1195), (844, 1235), (802, 1270), (756, 1299), (707, 1321), (655, 1336), (602, 1343),
            (546, 1343), (491, 1335), (440, 1319), (391, 1297), (345, 1267), (304, 1232), (269, 1191), (239, 1145), (217, 1096), (201, 1045),
            (193, 990), (193, 934), (200, 881), (215, 829), (237, 780), (266, 734), (301, 692), (341, 656), (386, 626), (434, 603),
            (487, 586), (504, 586), (517, 592), (529, 605), (537, 624), (538, 645), (533, 666), (519, 708), (513, 751), (513, 794),
            (522, 837), (537, 878), (558, 915), (587, 949), (621, 978), (658, 999), (699, 1014), (742, 1023), (785, 1023), (828, 1017),
            (870, 1003), (891, 998), (912, 999), (931, 1007), (944, 1018), (950, 1032), (950, 1049),
        ] },
        IconLine { closed: false, points: &[
            (1024, 768), (1020, 722), (1009, 680), (990, 640), (964, 603), (933, 572), (896, 546), (856, 527), (814, 516), (768, 512),
        ] },
        IconLine { closed: false, points: &[
            (1216, 320), (1136, 400),
        ] },
        IconLine { closed: false, points: &[
            (1280, 768), (1408, 768),
        ] },
    ],
    dots: &[],
};

/// `swatch-book.svg`.
static SWATCH_BOOK: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (704, 1088), (700, 1134), (689, 1176), (670, 1216), (644, 1253), (613, 1284), (576, 1310), (536, 1329), (494, 1340), (448, 1344),
            (402, 1340), (360, 1329), (320, 1310), (283, 1284), (252, 1253), (226, 1216), (207, 1176), (196, 1134), (192, 1088), (192, 320),
            (197, 286), (209, 256), (229, 229), (256, 209), (286, 197), (320, 192), (576, 192), (610, 197), (640, 209), (667, 229),
            (687, 256), (699, 286), (704, 320),
        ] },
        IconLine { closed: false, points: &[
            (1069, 832), (1216, 832), (1250, 837), (1280, 849), (1307, 869), (1327, 896), (1339, 926), (1344, 960), (1344, 1216), (1339, 1250),
            (1327, 1280), (1307, 1307), (1280, 1327), (1250, 1339), (1216, 1344), (448, 1344),
        ] },
        IconLine { closed: false, points: &[
            (704, 512), (851, 365), (875, 345), (901, 331), (930, 323), (960, 320), (990, 323), (1019, 331), (1046, 346), (1069, 365),
            (1190, 486), (1210, 510), (1225, 536), (1234, 565), (1238, 596), (1235, 627), (1226, 656), (1212, 682), (1192, 706), (634, 1267),
        ] },
    ],
    dots: &[
        (448, 1088),
    ],
};

/// `tags.svg`.
static TAGS: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (843, 128), (876, 132), (907, 145), (934, 166), (1363, 595), (1382, 618), (1397, 645), (1405, 674), (1408, 704), (1405, 734),
            (1397, 763), (1382, 790), (1363, 813), (1069, 1107), (1046, 1126), (1019, 1141), (990, 1149), (960, 1152), (930, 1149), (901, 1141),
            (874, 1126), (851, 1107), (422, 678), (401, 651), (388, 620), (384, 587), (384, 192), (387, 172), (396, 154), (410, 140),
            (428, 131), (448, 128),
        ] },
        IconLine { closed: false, points: &[
            (128, 448), (128, 843), (132, 876), (145, 907), (166, 934), (595, 1363), (625, 1386), (658, 1401), (695, 1408), (732, 1406),
            (767, 1395), (799, 1375),
        ] },
        IconLine { closed: true, points: &[
            (704, 416), (700, 432), (688, 444), (672, 448), (656, 444), (644, 432), (640, 416), (644, 400), (656, 388), (672, 384),
            (688, 388), (700, 400),
        ] },
    ],
    dots: &[],
};

/// `timeline.svg`.
static TIMELINE: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (602, 858), (629, 879), (660, 892), (693, 896), (1216, 896), (1236, 893), (1254, 884), (1268, 870), (1277, 852), (1280, 832),
            (1280, 704), (1277, 684), (1268, 666), (1254, 652), (1236, 643), (1216, 640), (693, 640), (660, 644), (629, 657), (602, 678),
            (512, 768),
        ] },
        IconLine { closed: true, points: &[
            (602, 1370), (629, 1391), (660, 1404), (693, 1408), (1216, 1408), (1236, 1405), (1254, 1396), (1268, 1382), (1277, 1364), (1280, 1344),
            (1280, 1216), (1277, 1196), (1268, 1178), (1254, 1164), (1236, 1155), (1216, 1152), (693, 1152), (660, 1156), (629, 1169), (602, 1190),
            (512, 1280),
        ] },
        IconLine { closed: true, points: &[
            (602, 346), (629, 367), (660, 380), (693, 384), (1216, 384), (1236, 381), (1254, 372), (1268, 358), (1277, 340), (1280, 320),
            (1280, 192), (1277, 172), (1268, 154), (1254, 140), (1236, 131), (1216, 128), (693, 128), (660, 132), (629, 145), (602, 166),
            (512, 256),
        ] },
    ],
    dots: &[
        (256, 768), (256, 1024), (256, 1280), (256, 256), (256, 512),
    ],
};

/// `triangle-alert.svg`.
static TRIANGLE_ALERT: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (1391, 1152), (879, 256), (858, 229), (832, 209), (801, 196), (767, 191), (734, 196), (703, 209), (677, 229), (656, 256),
            (144, 1152), (131, 1183), (127, 1216), (131, 1249), (144, 1281), (165, 1307), (192, 1327), (222, 1340), (256, 1344), (1280, 1344),
            (1314, 1340), (1344, 1327), (1370, 1307), (1391, 1280), (1404, 1249), (1408, 1216), (1404, 1183), (1391, 1152),
        ] },
        IconLine { closed: false, points: &[
            (768, 576), (768, 832),
        ] },
    ],
    dots: &[
        (768, 1088),
    ],
};

/// `undo-2.svg`.
static UNDO_2: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (576, 896), (256, 576), (576, 256),
        ] },
        IconLine { closed: false, points: &[
            (256, 576), (928, 576), (984, 580), (1037, 593), (1088, 614), (1135, 643), (1177, 679), (1213, 721), (1242, 768), (1263, 819),
            (1276, 872), (1280, 928), (1276, 984), (1263, 1037), (1242, 1088), (1213, 1135), (1177, 1177), (1135, 1213), (1088, 1242), (1037, 1263),
            (984, 1276), (928, 1280), (704, 1280),
        ] },
    ],
    dots: &[],
};

/// `unlink.svg`.
static UNLINK: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (1206, 784), (1316, 675), (1315, 675), (1347, 635), (1372, 592), (1390, 546), (1401, 497), (1404, 447), (1400, 397), (1387, 348),
            (1367, 302), (1341, 261), (1307, 222), (1268, 190), (1226, 165), (1181, 147), (1133, 136), (1084, 132), (1036, 136), (988, 147),
            (943, 165), (901, 190), (862, 222), (752, 332),
        ] },
        IconLine { closed: false, points: &[
            (331, 752), (221, 861), (189, 901), (164, 944), (146, 990), (135, 1039), (132, 1089), (136, 1139), (149, 1188), (169, 1234),
            (195, 1275), (229, 1314), (268, 1346), (310, 1371), (355, 1389), (403, 1400), (452, 1404), (500, 1400), (548, 1389), (593, 1371),
            (635, 1346), (674, 1314), (783, 1204),
        ] },
        IconLine { closed: false, points: &[
            (512, 128), (512, 320),
        ] },
        IconLine { closed: false, points: &[
            (128, 512), (320, 512),
        ] },
        IconLine { closed: false, points: &[
            (1024, 1216), (1024, 1408),
        ] },
        IconLine { closed: false, points: &[
            (1216, 1024), (1408, 1024),
        ] },
    ],
    dots: &[],
};

/// `user-plus.svg`.
static USER_PLUS: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (1024, 1344), (1024, 1216), (1020, 1170), (1009, 1128), (990, 1088), (964, 1051), (933, 1020), (896, 994), (856, 975), (814, 964),
            (768, 960), (384, 960), (338, 964), (296, 975), (256, 994), (219, 1020), (188, 1051), (162, 1088), (143, 1128), (132, 1170),
            (128, 1216), (128, 1344),
        ] },
        IconLine { closed: true, points: &[
            (832, 448), (828, 494), (817, 536), (798, 576), (772, 612), (740, 644), (704, 670), (664, 689), (622, 700), (576, 704),
            (530, 700), (488, 689), (448, 670), (412, 644), (380, 612), (354, 576), (335, 536), (324, 494), (320, 448), (324, 402),
            (335, 360), (354, 320), (380, 284), (412, 252), (448, 226), (488, 207), (530, 196), (576, 192), (622, 196), (664, 207),
            (704, 226), (740, 252), (772, 284), (798, 320), (817, 360), (828, 402),
        ] },
        IconLine { closed: false, points: &[
            (1216, 512), (1216, 896),
        ] },
        IconLine { closed: false, points: &[
            (1408, 704), (1024, 704),
        ] },
    ],
    dots: &[],
};

/// `users.svg`.
static USERS: IconData = IconData {
    lines: &[
        IconLine { closed: false, points: &[
            (1024, 1344), (1024, 1216), (1020, 1170), (1009, 1128), (990, 1088), (964, 1051), (933, 1020), (896, 994), (856, 975), (814, 964),
            (768, 960), (384, 960), (338, 964), (296, 975), (256, 994), (219, 1020), (188, 1051), (162, 1088), (143, 1128), (132, 1170),
            (128, 1216), (128, 1344),
        ] },
        IconLine { closed: false, points: &[
            (1024, 200), (1070, 217), (1111, 241), (1147, 273), (1176, 311), (1198, 354), (1211, 400), (1216, 448), (1211, 496), (1198, 542),
            (1176, 585), (1147, 623), (1111, 655), (1070, 679), (1024, 696),
        ] },
        IconLine { closed: false, points: &[
            (1408, 1344), (1408, 1216), (1403, 1168), (1390, 1122), (1368, 1079), (1339, 1041), (1303, 1009), (1262, 985), (1216, 968),
        ] },
        IconLine { closed: true, points: &[
            (832, 448), (828, 494), (817, 536), (798, 576), (772, 612), (740, 644), (704, 670), (664, 689), (622, 700), (576, 704),
            (530, 700), (488, 689), (448, 670), (412, 644), (380, 612), (354, 576), (335, 536), (324, 494), (320, 448), (324, 402),
            (335, 360), (354, 320), (380, 284), (412, 252), (448, 226), (488, 207), (530, 196), (576, 192), (622, 196), (664, 207),
            (704, 226), (740, 252), (772, 284), (798, 320), (817, 360), (828, 402),
        ] },
    ],
    dots: &[],
};

/// `zoom-in.svg`.
static ZOOM_IN: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (1216, 704), (1211, 772), (1199, 837), (1177, 900), (1147, 960), (1110, 1015), (1066, 1066), (1015, 1110), (960, 1147), (900, 1177),
            (837, 1199), (772, 1211), (704, 1216), (636, 1211), (571, 1199), (508, 1177), (448, 1147), (393, 1110), (342, 1066), (298, 1015),
            (261, 960), (231, 900), (209, 837), (197, 772), (192, 704), (197, 636), (209, 571), (231, 508), (261, 448), (298, 393),
            (342, 342), (393, 298), (448, 261), (508, 231), (571, 209), (636, 197), (704, 192), (772, 197), (837, 209), (900, 231),
            (960, 261), (1015, 298), (1066, 342), (1110, 393), (1147, 448), (1177, 508), (1199, 571), (1211, 636),
        ] },
        IconLine { closed: false, points: &[
            (1344, 1344), (1066, 1066),
        ] },
        IconLine { closed: false, points: &[
            (704, 512), (704, 896),
        ] },
        IconLine { closed: false, points: &[
            (512, 704), (896, 704),
        ] },
    ],
    dots: &[],
};

/// `zoom-out.svg`.
static ZOOM_OUT: IconData = IconData {
    lines: &[
        IconLine { closed: true, points: &[
            (1216, 704), (1211, 772), (1199, 837), (1177, 900), (1147, 960), (1110, 1015), (1066, 1066), (1015, 1110), (960, 1147), (900, 1177),
            (837, 1199), (772, 1211), (704, 1216), (636, 1211), (571, 1199), (508, 1177), (448, 1147), (393, 1110), (342, 1066), (298, 1015),
            (261, 960), (231, 900), (209, 837), (197, 772), (192, 704), (197, 636), (209, 571), (231, 508), (261, 448), (298, 393),
            (342, 342), (393, 298), (448, 261), (508, 231), (571, 209), (636, 197), (704, 192), (772, 197), (837, 209), (900, 231),
            (960, 261), (1015, 298), (1066, 342), (1110, 393), (1147, 448), (1177, 508), (1199, 571), (1211, 636),
        ] },
        IconLine { closed: false, points: &[
            (1344, 1344), (1066, 1066),
        ] },
        IconLine { closed: false, points: &[
            (512, 704), (896, 704),
        ] },
    ],
    dots: &[],
};
