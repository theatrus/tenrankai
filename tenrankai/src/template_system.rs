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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_type_path_mapping() {
        // Test page templates
        assert_eq!(TemplateType::Index.path(), "pages/index.html.liquid");
        assert_eq!(TemplateType::About.path(), "pages/about.html.liquid");
        assert_eq!(TemplateType::Contact.path(), "pages/contact.html.liquid");
        assert_eq!(TemplateType::NotFound.path(), "pages/404.html.liquid");

        // Test module templates
        assert_eq!(TemplateType::Gallery.path(), "modules/gallery.html.liquid");
        assert_eq!(
            TemplateType::ImageDetail.path(),
            "modules/image_detail.html.liquid"
        );
        assert_eq!(
            TemplateType::PostsIndex.path(),
            "modules/posts_index.html.liquid"
        );
        assert_eq!(
            TemplateType::PostDetail.path(),
            "modules/post_detail.html.liquid"
        );
        assert_eq!(TemplateType::Login.path(), "modules/login.html.liquid");
        assert_eq!(
            TemplateType::LoginSuccess.path(),
            "modules/login_success.html.liquid"
        );
        assert_eq!(
            TemplateType::PasskeyEnrollment.path(),
            "modules/passkey_enrollment.html.liquid"
        );
        assert_eq!(TemplateType::Profile.path(), "modules/profile.html.liquid");

        // Test partial templates
        assert_eq!(TemplateType::Header.path(), "partials/_header.html.liquid");
        assert_eq!(TemplateType::Footer.path(), "partials/_footer.html.liquid");
        assert_eq!(
            TemplateType::GalleryPreview.path(),
            "partials/_gallery_preview.html.liquid"
        );
        assert_eq!(
            TemplateType::UserMenu.path(),
            "partials/_user_menu.html.liquid"
        );
    }

    #[test]
    fn test_template_type_categories() {
        // Test page categories
        assert_eq!(TemplateType::Index.category(), TemplateCategory::Page);
        assert_eq!(TemplateType::About.category(), TemplateCategory::Page);
        assert_eq!(TemplateType::Contact.category(), TemplateCategory::Page);
        assert_eq!(TemplateType::NotFound.category(), TemplateCategory::Page);

        // Test module categories
        assert_eq!(TemplateType::Gallery.category(), TemplateCategory::Module);
        assert_eq!(
            TemplateType::ImageDetail.category(),
            TemplateCategory::Module
        );
        assert_eq!(
            TemplateType::PostsIndex.category(),
            TemplateCategory::Module
        );
        assert_eq!(
            TemplateType::PostDetail.category(),
            TemplateCategory::Module
        );
        assert_eq!(TemplateType::Login.category(), TemplateCategory::Module);
        assert_eq!(
            TemplateType::LoginSuccess.category(),
            TemplateCategory::Module
        );
        assert_eq!(
            TemplateType::PasskeyEnrollment.category(),
            TemplateCategory::Module
        );
        assert_eq!(TemplateType::Profile.category(), TemplateCategory::Module);

        // Test partial categories
        assert_eq!(TemplateType::Header.category(), TemplateCategory::Partial);
        assert_eq!(TemplateType::Footer.category(), TemplateCategory::Partial);
        assert_eq!(
            TemplateType::GalleryPreview.category(),
            TemplateCategory::Partial
        );
        assert_eq!(TemplateType::UserMenu.category(), TemplateCategory::Partial);
    }

    #[test]
    fn test_template_type_partial_check() {
        // Pages and modules are not partials
        assert!(!TemplateType::Index.is_partial());
        assert!(!TemplateType::Gallery.is_partial());
        assert!(!TemplateType::Login.is_partial());

        // Partials are partials
        assert!(TemplateType::Header.is_partial());
        assert!(TemplateType::Footer.is_partial());
        assert!(TemplateType::GalleryPreview.is_partial());
        assert!(TemplateType::UserMenu.is_partial());
    }

    #[test]
    fn test_template_type_constants() {
        // Test PAGES constant
        assert!(TemplateType::PAGES.contains(&TemplateType::Index));
        assert!(TemplateType::PAGES.contains(&TemplateType::About));
        assert!(TemplateType::PAGES.contains(&TemplateType::Contact));
        assert!(TemplateType::PAGES.contains(&TemplateType::NotFound));
        assert_eq!(TemplateType::PAGES.len(), 4);

        // Test MODULES constant
        assert!(TemplateType::MODULES.contains(&TemplateType::Gallery));
        assert!(TemplateType::MODULES.contains(&TemplateType::ImageDetail));
        assert!(TemplateType::MODULES.contains(&TemplateType::PostsIndex));
        assert!(TemplateType::MODULES.contains(&TemplateType::PostDetail));
        assert!(TemplateType::MODULES.contains(&TemplateType::Login));
        assert!(TemplateType::MODULES.contains(&TemplateType::LoginSuccess));
        assert!(TemplateType::MODULES.contains(&TemplateType::PasskeyEnrollment));
        assert!(TemplateType::MODULES.contains(&TemplateType::Profile));
        assert_eq!(TemplateType::MODULES.len(), 8);

        // Test PARTIALS constant
        assert!(TemplateType::PARTIALS.contains(&TemplateType::Header));
        assert!(TemplateType::PARTIALS.contains(&TemplateType::Footer));
        assert!(TemplateType::PARTIALS.contains(&TemplateType::GalleryPreview));
        assert!(TemplateType::PARTIALS.contains(&TemplateType::UserMenu));
        assert_eq!(TemplateType::PARTIALS.len(), 4);

        // Test ALL_STANDARD constant
        assert_eq!(TemplateType::ALL_STANDARD.len(), 16); // 4 pages + 8 modules + 4 partials
        assert!(TemplateType::ALL_STANDARD.contains(&TemplateType::Index));
        assert!(TemplateType::ALL_STANDARD.contains(&TemplateType::Gallery));
        assert!(TemplateType::ALL_STANDARD.contains(&TemplateType::Header));
    }

    #[test]
    fn test_template_type_parsing() {
        // Test successful parsing
        assert_eq!(
            TemplateType::parse_from_path("pages/index.html.liquid"),
            Some(TemplateType::Index)
        );
        assert_eq!(
            TemplateType::parse_from_path("modules/gallery.html.liquid"),
            Some(TemplateType::Gallery)
        );
        assert_eq!(
            TemplateType::parse_from_path("partials/_header.html.liquid"),
            Some(TemplateType::Header)
        );

        // Test failed parsing
        assert_eq!(
            TemplateType::parse_from_path("invalid/path.html.liquid"),
            None
        );
        assert_eq!(TemplateType::parse_from_path(""), None);
    }

    #[test]
    fn test_template_type_dynamic_page_path() {
        let dynamic_path = TemplateType::dynamic_page_path("custom");
        assert_eq!(
            dynamic_path,
            TemplatePath::Dynamic("pages/custom.html.liquid".to_string())
        );

        let dynamic_path = TemplateType::dynamic_page_path("admin");
        assert_eq!(
            dynamic_path,
            TemplatePath::Dynamic("pages/admin.html.liquid".to_string())
        );
    }

    #[test]
    fn test_template_path_functionality() {
        // Test typed template path
        let typed_path = TemplatePath::Typed(TemplateType::Gallery);
        assert_eq!(typed_path.path(), "modules/gallery.html.liquid");

        // Test dynamic template path
        let dynamic_path = TemplatePath::Dynamic("custom/template.html.liquid".to_string());
        assert_eq!(dynamic_path.path(), "custom/template.html.liquid");

        // Test creation helpers
        let typed_helper = TemplatePath::typed(TemplateType::About);
        assert_eq!(typed_helper, TemplatePath::Typed(TemplateType::About));

        let dynamic_helper = TemplatePath::dynamic("custom/path.html.liquid".to_string());
        assert_eq!(
            dynamic_helper,
            TemplatePath::Dynamic("custom/path.html.liquid".to_string())
        );

        let dynamic_page_helper = TemplatePath::dynamic_page("test");
        assert_eq!(
            dynamic_page_helper,
            TemplatePath::Dynamic("pages/test.html.liquid".to_string())
        );
    }

    #[test]
    fn test_template_display() {
        // Test TemplateType display
        assert_eq!(
            format!("{}", TemplateType::Index),
            "pages/index.html.liquid"
        );
        assert_eq!(
            format!("{}", TemplateType::Gallery),
            "modules/gallery.html.liquid"
        );
        assert_eq!(
            format!("{}", TemplateType::Header),
            "partials/_header.html.liquid"
        );

        // Test TemplatePath display
        let typed_path = TemplatePath::Typed(TemplateType::About);
        assert_eq!(format!("{}", typed_path), "pages/about.html.liquid");

        let dynamic_path = TemplatePath::Dynamic("custom/template.html.liquid".to_string());
        assert_eq!(format!("{}", dynamic_path), "custom/template.html.liquid");
    }

    #[test]
    fn test_template_category_classification() {
        // Test that all pages are correctly classified
        for &page_type in TemplateType::PAGES {
            assert_eq!(page_type.category(), TemplateCategory::Page);
            assert!(!page_type.is_partial());
        }

        // Test that all modules are correctly classified
        for &module_type in TemplateType::MODULES {
            assert_eq!(module_type.category(), TemplateCategory::Module);
            assert!(!module_type.is_partial());
        }

        // Test that all partials are correctly classified
        for &partial_type in TemplateType::PARTIALS {
            assert_eq!(partial_type.category(), TemplateCategory::Partial);
            assert!(partial_type.is_partial());
        }
    }

    #[test]
    fn test_template_path_equality() {
        let typed1 = TemplatePath::Typed(TemplateType::Gallery);
        let typed2 = TemplatePath::Typed(TemplateType::Gallery);
        let typed3 = TemplatePath::Typed(TemplateType::Index);

        assert_eq!(typed1, typed2);
        assert_ne!(typed1, typed3);

        let dynamic1 = TemplatePath::Dynamic("test.html.liquid".to_string());
        let dynamic2 = TemplatePath::Dynamic("test.html.liquid".to_string());
        let dynamic3 = TemplatePath::Dynamic("other.html.liquid".to_string());

        assert_eq!(dynamic1, dynamic2);
        assert_ne!(dynamic1, dynamic3);

        // Typed and dynamic should never be equal even with same path
        let typed = TemplatePath::Typed(TemplateType::Index);
        let dynamic = TemplatePath::Dynamic("pages/index.html.liquid".to_string());
        assert_ne!(typed, dynamic);
    }
}
