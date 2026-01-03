/// Template types with path resolution and categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateType {
    // Pages - Static page templates
    Index,
    About,
    Contact,
    NotFound,

    // Modules - Feature-specific templates
    Gallery,
    ImageDetail,
    PostsIndex,
    PostDetail,
    Login,
    LoginSuccess,
    PasskeyEnrollment,
    Profile,

    // Partials - Reusable components
    Header,
    Footer,
    GalleryPreview,
    UserMenu,
}

/// Template path that can be either a type-safe enum or a dynamic string
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TemplatePath {
    /// Type-safe template reference
    Typed(TemplateType),
    /// Dynamic template path (for runtime-determined templates)
    Dynamic(String),
}

impl TemplateType {
    /// Get the template file path
    pub fn path(&self) -> &'static str {
        match self {
            // Pages
            TemplateType::Index => "pages/index.html.liquid",
            TemplateType::About => "pages/about.html.liquid",
            TemplateType::Contact => "pages/contact.html.liquid",
            TemplateType::NotFound => "pages/404.html.liquid",

            // Modules
            TemplateType::Gallery => "modules/gallery.html.liquid",
            TemplateType::ImageDetail => "modules/image_detail.html.liquid",
            TemplateType::PostsIndex => "modules/posts_index.html.liquid",
            TemplateType::PostDetail => "modules/post_detail.html.liquid",
            TemplateType::Login => "modules/login.html.liquid",
            TemplateType::LoginSuccess => "modules/login_success.html.liquid",
            TemplateType::PasskeyEnrollment => "modules/passkey_enrollment.html.liquid",
            TemplateType::Profile => "modules/profile.html.liquid",

            // Partials
            TemplateType::Header => "partials/_header.html.liquid",
            TemplateType::Footer => "partials/_footer.html.liquid",
            TemplateType::GalleryPreview => "partials/_gallery_preview.html.liquid",
            TemplateType::UserMenu => "partials/_user_menu.html.liquid",
        }
    }

    /// Get the template category
    pub fn category(&self) -> TemplateCategory {
        match self {
            TemplateType::Index
            | TemplateType::About
            | TemplateType::Contact
            | TemplateType::NotFound => TemplateCategory::Page,

            TemplateType::Gallery
            | TemplateType::ImageDetail
            | TemplateType::PostsIndex
            | TemplateType::PostDetail
            | TemplateType::Login
            | TemplateType::LoginSuccess
            | TemplateType::PasskeyEnrollment
            | TemplateType::Profile => TemplateCategory::Module,

            TemplateType::Header
            | TemplateType::Footer
            | TemplateType::GalleryPreview
            | TemplateType::UserMenu => TemplateCategory::Partial,
        }
    }

    /// Check if this is a partial template (for caching purposes)
    pub fn is_partial(&self) -> bool {
        matches!(self.category(), TemplateCategory::Partial)
    }

    /// Page template types
    pub const PAGES: &'static [TemplateType] = &[
        TemplateType::Index,
        TemplateType::About,
        TemplateType::Contact,
        TemplateType::NotFound,
    ];

    /// Module template types
    pub const MODULES: &'static [TemplateType] = &[
        TemplateType::Gallery,
        TemplateType::ImageDetail,
        TemplateType::PostsIndex,
        TemplateType::PostDetail,
        TemplateType::Login,
        TemplateType::LoginSuccess,
        TemplateType::PasskeyEnrollment,
        TemplateType::Profile,
    ];

    /// Partial template types
    pub const PARTIALS: &'static [TemplateType] = &[
        TemplateType::Header,
        TemplateType::Footer,
        TemplateType::GalleryPreview,
        TemplateType::UserMenu,
    ];

    /// All standard template types (excludes dynamic templates)
    pub const ALL_STANDARD: &'static [TemplateType] = &[
        // Pages
        TemplateType::Index,
        TemplateType::About,
        TemplateType::Contact,
        TemplateType::NotFound,
        // Modules
        TemplateType::Gallery,
        TemplateType::ImageDetail,
        TemplateType::PostsIndex,
        TemplateType::PostDetail,
        TemplateType::Login,
        TemplateType::LoginSuccess,
        TemplateType::PasskeyEnrollment,
        TemplateType::Profile,
        // Partials
        TemplateType::Header,
        TemplateType::Footer,
        TemplateType::GalleryPreview,
        TemplateType::UserMenu,
    ];

    /// Parse a template type from a template path
    pub fn parse_from_path(path: &str) -> Option<TemplateType> {
        Self::ALL_STANDARD
            .iter()
            .find(|&template_type| template_type.path() == path)
            .copied()
    }

    /// Create a dynamic template path for pages (helper for common case)
    pub fn dynamic_page_path(name: &str) -> TemplatePath {
        TemplatePath::Dynamic(format!("pages/{}.html.liquid", name))
    }
}

impl TemplatePath {
    /// Get the template path string
    pub fn path(&self) -> String {
        match self {
            TemplatePath::Typed(template_type) => template_type.path().to_string(),
            TemplatePath::Dynamic(path) => path.clone(),
        }
    }

    /// Create a typed template path
    pub fn typed(template_type: TemplateType) -> TemplatePath {
        TemplatePath::Typed(template_type)
    }

    /// Create a dynamic template path
    pub fn dynamic(path: String) -> TemplatePath {
        TemplatePath::Dynamic(path)
    }

    /// Create a dynamic page template path (helper for common case)
    pub fn dynamic_page(name: &str) -> TemplatePath {
        TemplatePath::Dynamic(format!("pages/{}.html.liquid", name))
    }
}

/// Template category classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateCategory {
    Page,
    Module,
    Partial,
}

impl std::fmt::Display for TemplateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path())
    }
}

impl std::fmt::Display for TemplatePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path())
    }
}