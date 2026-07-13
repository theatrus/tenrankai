#[cfg(test)]
mod posts_tests {
    use super::super::*;
    use crate::storage::{DynStorage, FilesystemStorage};
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn setup_test_posts_dir() -> (TempDir, PostsConfig, DynStorage) {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        // Create test markdown files
        let post1_content = r#"+++
title = "First Post"
summary = "This is the first test post"
date = "2024-01-01"
+++

# First Post

This is the content of the first post.

It has multiple paragraphs."#;

        let post2_content = r#"+++
title = "Second Post"
summary = "This is the second test post"
date = "2024-01-02"
+++

# Second Post

This is the content of the second post.

## With a subsection

And some more content."#;

        fs::write(posts_dir.join("first-post.md"), post1_content).unwrap();
        fs::write(posts_dir.join("second-post.md"), post2_content).unwrap();

        // Create a subdirectory with another post
        let subdir = posts_dir.join("tutorials");
        fs::create_dir(&subdir).unwrap();

        let post3_content = r#"+++
title = "Tutorial Post"
summary = "This is a tutorial"
date = "2024-01-03"
+++

# Tutorial

This is a tutorial post in a subdirectory."#;

        fs::write(subdir.join("tutorial.md"), post3_content).unwrap();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));

        let config = PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        };

        (temp_dir, config, storage)
    }

    #[tokio::test]
    async fn test_posts_manager_creation() {
        let (_temp_dir, config, storage) = setup_test_posts_dir().await;
        let manager = PostsManager::new(config.clone(), storage);
        assert_eq!(manager.get_config().url_prefix, "/posts");
    }

    #[tokio::test]
    async fn test_refresh_posts() {
        let (_temp_dir, config, storage) = setup_test_posts_dir().await;
        let manager = PostsManager::new(config, storage);

        let result = manager.refresh_posts().await;
        assert!(result.is_ok());

        let posts = manager.get_posts_page(0, None, None).await;
        assert_eq!(posts.len(), 3);

        // Check that posts are sorted by date (newest first)
        assert_eq!(posts[0].title, "Tutorial Post");
        assert_eq!(posts[1].title, "Second Post");
        assert_eq!(posts[2].title, "First Post");
    }

    #[tokio::test]
    async fn test_get_post() {
        let (_temp_dir, config, storage) = setup_test_posts_dir().await;
        let manager = PostsManager::new(config, storage);
        manager.refresh_posts().await.unwrap();

        let post = manager.get_post("first-post").await;
        assert!(post.is_some());
        let post = post.unwrap();
        assert_eq!(post.title, "First Post");
        assert_eq!(post.summary, "This is the first test post");
        assert!(post.html_content.contains("<h1>First Post</h1>"));
    }

    #[tokio::test]
    async fn test_get_post_from_subdirectory() {
        let (_temp_dir, config, storage) = setup_test_posts_dir().await;
        let manager = PostsManager::new(config, storage);
        manager.refresh_posts().await.unwrap();

        let post = manager.get_post("tutorials/tutorial").await;
        assert!(post.is_some());
        let post = post.unwrap();
        assert_eq!(post.title, "Tutorial Post");
        assert_eq!(post.slug, "tutorials/tutorial");
    }

    #[tokio::test]
    async fn test_pagination() {
        let (_temp_dir, config_orig, storage) = setup_test_posts_dir().await;
        let mut config = config_orig;
        config.posts_per_page = 2;

        let manager = PostsManager::new(config, storage);
        manager.refresh_posts().await.unwrap();

        let page1 = manager.get_posts_page(0, None, None).await;
        assert_eq!(page1.len(), 2);

        let page2 = manager.get_posts_page(1, None, None).await;
        assert_eq!(page2.len(), 1);

        let total_pages = manager.get_total_pages(None, None).await;
        assert_eq!(total_pages, 2);
    }

    #[tokio::test]
    async fn test_categories_and_filtering() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        let travel_post = r#"+++
title = "Travel Post"
summary = "A post about travel"
date = "2024-02-01"
categories = ["Travel", "Photo Gear"]
+++

Content about travel."#;

        let gear_post = r#"+++
title = "Gear Post"
summary = "A post about gear"
date = "2024-02-02"
categories = ["Photo Gear"]
+++

Content about gear."#;

        let uncategorized_post = r#"+++
title = "Plain Post"
summary = "No categories here"
date = "2024-02-03"
+++

Plain content."#;

        fs::write(posts_dir.join("travel.md"), travel_post).unwrap();
        fs::write(posts_dir.join("gear.md"), gear_post).unwrap();
        fs::write(posts_dir.join("plain.md"), uncategorized_post).unwrap();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let config = PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        };

        let manager = PostsManager::new(config, storage);
        manager.refresh_posts().await.unwrap();

        // Unfiltered listing includes all posts with their categories
        let all = manager.get_posts_page(0, None, None).await;
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].title, "Plain Post");
        assert!(all[0].categories.is_empty());
        assert_eq!(all[2].categories, vec!["Travel", "Photo Gear"]);

        // Filtering by slugified category
        let gear = manager.get_posts_page(0, Some("photo-gear"), None).await;
        assert_eq!(gear.len(), 2);
        assert_eq!(gear[0].title, "Gear Post");
        assert_eq!(gear[1].title, "Travel Post");

        let travel = manager.get_posts_page(0, Some("travel"), None).await;
        assert_eq!(travel.len(), 1);
        assert_eq!(travel[0].title, "Travel Post");

        let none = manager.get_posts_page(0, Some("nonexistent"), None).await;
        assert!(none.is_empty());

        // Page counts respect the filter
        assert_eq!(manager.get_total_pages(None, None).await, 1);
        assert_eq!(manager.get_total_pages(Some("nonexistent"), None).await, 0);

        // Category summary with counts, sorted by name
        let categories = manager.get_categories(None).await;
        assert_eq!(categories.len(), 2);
        assert_eq!(categories[0].name, "Photo Gear");
        assert_eq!(categories[0].slug, "photo-gear");
        assert_eq!(categories[0].count, 2);
        assert_eq!(categories[1].name, "Travel");
        assert_eq!(categories[1].slug, "travel");
        assert_eq!(categories[1].count, 1);
    }

    fn test_config(posts_dir: &std::path::Path) -> PostsConfig {
        PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_folder_permissions_filtering() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        let public_post = r#"+++
title = "Public Post"
summary = "Everyone can see this"
date = "2024-05-01"
categories = ["Open"]
+++

Public content."#;

        let private_post = r#"+++
title = "Private Post"
summary = "Restricted"
date = "2024-05-02"
categories = ["Secret"]
+++

Private content."#;

        let folder_md = r#"+++
[permissions]
public_role = "none"
default_authenticated_role = "none"

[permissions.roles.owner]
name = "owner"
permissions = { can_view = true, can_edit_content = true }

[[permissions.user_roles]]
username = "alice"
roles = ["owner"]
+++
"#;

        fs::write(posts_dir.join("public-post.md"), public_post).unwrap();
        let private_dir = posts_dir.join("private");
        fs::create_dir(&private_dir).unwrap();
        fs::write(private_dir.join("secret.md"), private_post).unwrap();
        fs::write(private_dir.join("_folder.md"), folder_md).unwrap();

        // Nested subdirectory inherits the nearest ancestor's _folder.md
        let nested_dir = private_dir.join("nested");
        fs::create_dir(&nested_dir).unwrap();
        fs::write(nested_dir.join("deep.md"), private_post).unwrap();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let manager = PostsManager::new(test_config(posts_dir), storage);
        manager.refresh_posts().await.unwrap();

        // Anonymous users only see the public post
        let anon = manager.get_posts_page(0, None, None).await;
        assert_eq!(anon.len(), 1);
        assert_eq!(anon[0].title, "Public Post");
        assert_eq!(manager.get_total_pages(None, None).await, 1);

        // Anonymous categories exclude hidden posts
        let anon_categories = manager.get_categories(None).await;
        assert_eq!(anon_categories.len(), 1);
        assert_eq!(anon_categories[0].name, "Open");

        // Other authenticated users are also excluded
        let bob = manager.get_posts_page(0, None, Some("bob")).await;
        assert_eq!(bob.len(), 1);

        // Alice sees everything, including the nested subdirectory post
        let alice = manager.get_posts_page(0, None, Some("alice")).await;
        assert_eq!(alice.len(), 3);

        // Permission resolution for individual posts
        let anon_perms = manager.resolve_permissions("private/secret", None).await;
        assert!(!anon_perms.can_view);
        assert!(!anon_perms.can_edit_content);

        let alice_perms = manager
            .resolve_permissions("private/secret", Some("alice"))
            .await;
        assert!(alice_perms.can_view);
        assert!(alice_perms.can_edit_content);

        let alice_nested = manager
            .resolve_permissions("private/nested/deep", Some("alice"))
            .await;
        assert!(alice_nested.can_view);

        // Root posts use system defaults (no _folder.md at root)
        let root_perms = manager.resolve_permissions("public-post", None).await;
        assert!(root_perms.can_view);
        assert!(!root_perms.can_edit_content);
    }

    #[test]
    fn test_validate_slug() {
        assert!(PostsManager::validate_slug("my-post").is_ok());
        assert!(PostsManager::validate_slug("2024/travel/my_post-2").is_ok());

        // "category" is reserved for category index routes
        assert!(PostsManager::validate_slug("category").is_err());
        assert!(PostsManager::validate_slug("category/nested").is_err());
        assert!(PostsManager::validate_slug("categories").is_ok());

        assert!(PostsManager::validate_slug("").is_err());
        assert!(PostsManager::validate_slug("/leading").is_err());
        assert!(PostsManager::validate_slug("trailing/").is_err());
        assert!(PostsManager::validate_slug("a//b").is_err());
        assert!(PostsManager::validate_slug("_hidden").is_err());
        assert!(PostsManager::validate_slug("a/_hidden").is_err());
        assert!(PostsManager::validate_slug("../escape").is_err());
        assert!(PostsManager::validate_slug("has space").is_err());
        assert!(PostsManager::validate_slug("dot.md").is_err());
    }

    #[tokio::test]
    async fn test_reserved_category_slug_posts_are_skipped() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        let post = r#"+++
title = "Shadowed"
summary = "Would be unreachable behind the category routes"
date = "2024-06-01"
+++

Content."#;

        let category_dir = posts_dir.join("category");
        fs::create_dir(&category_dir).unwrap();
        fs::write(category_dir.join("shadowed.md"), post).unwrap();
        fs::write(posts_dir.join("visible.md"), post).unwrap();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let manager = PostsManager::new(test_config(posts_dir), storage);
        manager.refresh_posts().await.unwrap();

        assert!(manager.get_post("category/shadowed").await.is_none());
        assert_eq!(manager.get_posts_page(0, None, None).await.len(), 1);
    }

    #[tokio::test]
    async fn test_get_recent_posts() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        for i in 1..=5 {
            let categories = if i % 2 == 0 {
                r#"["Even"]"#
            } else {
                r#"["Odd"]"#
            };
            let content = format!(
                r#"+++
title = "Post {i}"
summary = "Summary {i}"
date = "2024-07-0{i}"
categories = {categories}
+++

Body {i}."#
            );
            fs::write(posts_dir.join(format!("post-{i}.md")), content).unwrap();
        }

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let manager = PostsManager::new(test_config(posts_dir), storage);
        manager.refresh_posts().await.unwrap();

        // Limited, newest first, with full content
        let recent = manager.get_recent_posts(3, None, None).await;
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].title, "Post 5");
        assert_eq!(recent[2].title, "Post 3");
        assert!(recent[0].html_content.contains("Body 5."));

        // Category filtering
        let even = manager.get_recent_posts(10, Some("even"), None).await;
        assert_eq!(even.len(), 2);
        assert_eq!(even[0].title, "Post 4");
        assert_eq!(even[1].title, "Post 2");
    }

    #[tokio::test]
    async fn test_create_update_delete_post() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let manager = PostsManager::new(test_config(posts_dir), storage);
        manager.refresh_posts().await.unwrap();

        let metadata = PostMetadata {
            title: "Created Post".to_string(),
            summary: "Made via the API".to_string(),
            date: PostsManager::parse_date("2024-06-01").unwrap(),
            categories: vec!["News".to_string()],
            hero_image: Some("https://example.com/hero.jpg".to_string()),
        };

        // Create
        manager
            .create_post("news/created-post", &metadata, "# Hello\n\nBody text.")
            .await
            .unwrap();
        assert!(posts_dir.join("news/created-post.md").exists());

        let post = manager.get_post("news/created-post").await.unwrap();
        assert_eq!(post.title, "Created Post");
        assert_eq!(post.categories, vec!["News"]);
        assert_eq!(
            post.hero_image.as_deref(),
            Some("https://example.com/hero.jpg")
        );
        assert!(post.html_content.contains("<h1>Hello</h1>"));

        // Creating over an existing slug is rejected
        let conflict = manager
            .create_post("news/created-post", &metadata, "other")
            .await;
        assert!(matches!(conflict, Err(PostsError::PostAlreadyExists(_))));

        // Update
        let updated_metadata = PostMetadata {
            title: "Updated Post".to_string(),
            summary: "Now updated".to_string(),
            categories: vec!["News".to_string(), "Updates".to_string()],
            hero_image: None,
            ..metadata
        };
        manager
            .update_post("news/created-post", &updated_metadata, "New **body**.")
            .await
            .unwrap();

        let post = manager.get_post("news/created-post").await.unwrap();
        assert_eq!(post.title, "Updated Post");
        assert_eq!(post.categories, vec!["News", "Updates"]);
        assert_eq!(post.hero_image, None);
        assert!(post.html_content.contains("<strong>body</strong>"));

        // Delete
        manager.delete_post("news/created-post").await.unwrap();
        assert!(!posts_dir.join("news/created-post.md").exists());
        assert!(manager.get_post("news/created-post").await.is_none());

        let missing = manager.delete_post("news/created-post").await;
        assert!(matches!(missing, Err(PostsError::PostNotFound(_))));
    }

    #[test]
    fn test_category_slug() {
        assert_eq!(PostsManager::category_slug("Travel"), "travel");
        assert_eq!(PostsManager::category_slug("Photo Gear"), "photo-gear");
        assert_eq!(PostsManager::category_slug("C++ & Rust!"), "c-rust");
        assert_eq!(PostsManager::category_slug("  spaced  "), "spaced");
        assert_eq!(PostsManager::category_slug("日本語"), "日本語");
    }

    #[tokio::test]
    async fn test_category_pagination() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        for i in 0..3 {
            let content = format!(
                r#"+++
title = "Tagged Post {i}"
summary = "Post {i}"
date = "2024-03-0{}"
categories = ["news"]
+++

Content."#,
                i + 1
            );
            fs::write(posts_dir.join(format!("tagged-{i}.md")), content).unwrap();
        }

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let config = PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 2,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        };

        let manager = PostsManager::new(config, storage);
        manager.refresh_posts().await.unwrap();

        assert_eq!(manager.get_total_pages(Some("news"), None).await, 2);
        assert_eq!(manager.get_posts_page(0, Some("news"), None).await.len(), 2);
        assert_eq!(manager.get_posts_page(1, Some("news"), None).await.len(), 1);
    }

    #[tokio::test]
    async fn test_hero_image_and_reading_time() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        let explicit_hero = r#"+++
title = "Explicit Hero"
summary = "Post with explicit hero image"
date = "2024-04-01"
hero_image = "https://example.com/hero.jpg"
+++

![Some other image](https://example.com/body.jpg)

Content."#;

        let first_image_hero = r#"+++
title = "First Image Hero"
summary = "Hero falls back to first content image"
date = "2024-04-02"
+++

Intro paragraph.

![An image](https://example.com/first.jpg)

More content."#;

        let no_image = r#"+++
title = "No Image"
summary = "No images at all"
date = "2024-04-03"
+++

Just text."#;

        fs::write(posts_dir.join("explicit-hero.md"), explicit_hero).unwrap();
        fs::write(posts_dir.join("first-image.md"), first_image_hero).unwrap();
        fs::write(posts_dir.join("no-image.md"), no_image).unwrap();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let config = PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        };

        let manager = PostsManager::new(config, storage);
        manager.refresh_posts().await.unwrap();

        let post = manager.get_post("explicit-hero").await.unwrap();
        assert_eq!(
            post.hero_image.as_deref(),
            Some("https://example.com/hero.jpg")
        );
        assert_eq!(post.reading_time_minutes, 1);

        let post = manager.get_post("first-image").await.unwrap();
        assert_eq!(
            post.hero_image.as_deref(),
            Some("https://example.com/first.jpg")
        );

        let post = manager.get_post("no-image").await.unwrap();
        assert_eq!(post.hero_image, None);
    }

    #[tokio::test]
    async fn test_invalid_front_matter() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        // Create post with invalid front matter
        let invalid_content = r#"This post has no front matter

Just content."#;

        fs::write(posts_dir.join("invalid.md"), invalid_content).unwrap();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let config = PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        };

        let manager = PostsManager::new(config, storage);
        let result = manager.refresh_posts().await;
        assert!(result.is_ok()); // Should not fail completely

        let posts = manager.get_posts_page(0, None, None).await;
        assert_eq!(posts.len(), 0); // Invalid post should be skipped
    }

    #[tokio::test]
    async fn test_date_formats() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        // Test different date formats
        let post_with_full_date = r#"+++
title = "Full Date Post"
summary = "Post with full RFC3339 date"
date = "2024-01-15T10:30:00Z"
+++

Content"#;

        let post_with_simple_date = r#"+++
title = "Simple Date Post"
summary = "Post with simple date"
date = "2024-01-16"
+++

Content"#;

        fs::write(posts_dir.join("full-date.md"), post_with_full_date).unwrap();
        fs::write(posts_dir.join("simple-date.md"), post_with_simple_date).unwrap();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let config = PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        };

        let manager = PostsManager::new(config, storage);
        let result = manager.refresh_posts().await;
        assert!(result.is_ok());

        let posts = manager.get_posts_page(0, None, None).await;
        assert_eq!(posts.len(), 2);
    }

    #[tokio::test]
    async fn test_markdown_rendering() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        let markdown_content = r#"+++
title = "Markdown Test"
summary = "Testing markdown features"
date = "2024-01-01"
+++

# Heading 1

## Heading 2

This is a paragraph with **bold** and *italic* text.

- List item 1
- List item 2

1. Numbered item 1
2. Numbered item 2

```rust
fn main() {
    println!("Hello, world!");
}
```

> This is a blockquote

[Link to example](https://example.com)

| Column 1 | Column 2 |
|----------|----------|
| Cell 1   | Cell 2   |

~~Strikethrough text~~

Footnote[^1]

[^1]: This is a footnote
"#;

        fs::write(posts_dir.join("markdown-test.md"), markdown_content).unwrap();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let config = PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        };

        let manager = PostsManager::new(config, storage);
        manager.refresh_posts().await.unwrap();

        let post = manager.get_post("markdown-test").await.unwrap();

        // Check various markdown features are rendered
        assert!(post.html_content.contains("<h1>Heading 1</h1>"));
        assert!(post.html_content.contains("<h2>Heading 2</h2>"));
        assert!(post.html_content.contains("<strong>bold</strong>"));
        assert!(post.html_content.contains("<em>italic</em>"));
        assert!(post.html_content.contains("<ul>"));
        assert!(post.html_content.contains("<ol>"));
        assert!(post.html_content.contains("<pre><code"));
        assert!(post.html_content.contains("<blockquote>"));
        assert!(
            post.html_content
                .contains("<a href=\"https://example.com\"")
        );
        assert!(post.html_content.contains("<table>"));
        assert!(post.html_content.contains("<del>Strikethrough text</del>"));
        assert!(post.html_content.contains("sup")); // Footnote reference
    }

    #[tokio::test]
    async fn test_gallery_image_references() {
        use crate::GallerySystemConfig;
        use crate::gallery::Gallery;
        use std::collections::HashMap;

        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        // Create a post with gallery references
        let post_content = r#"+++
title = "Gallery Test Post"
summary = "Testing gallery image references"
date = "2024-01-15"
+++

# Gallery Test Post

Here's a thumbnail from the main gallery:
![gallery:main:vacation/beach.jpg](thumbnail)

And here's a gallery-sized image:
![gallery:main:vacation/sunset.jpg](gallery)

And a medium-sized image with a link:
![gallery:portfolio:projects/app-screenshot.png](medium)

Regular markdown image (not a gallery reference):
![Regular Image](https://example.com/image.jpg)
"#;

        fs::write(posts_dir.join("gallery-test.md"), post_content).unwrap();

        // Set up mock galleries
        let mut galleries = HashMap::new();

        // Create main gallery
        let main_photos_dir = temp_dir.path().join("photos");
        let main_gallery_config = GallerySystemConfig {
            name: "main".to_string(),
            source_directory: main_photos_dir.to_string_lossy().to_string(),
            cache_directory: temp_dir
                .path()
                .join("cache/main")
                .to_string_lossy()
                .to_string(),
            ..Default::default()
        };

        let main_cache_dir = temp_dir.path().join("cache/main");
        fs::create_dir_all(&main_cache_dir).unwrap();
        let main_source_storage: DynStorage = Arc::new(FilesystemStorage::new(&main_photos_dir));
        let main_cache_storage: DynStorage = Arc::new(FilesystemStorage::new(main_cache_dir));
        let main_gallery = Arc::new(Gallery::new(
            main_gallery_config.clone(),
            main_source_storage,
            main_cache_storage,
        ));
        galleries.insert("main".to_string(), main_gallery);

        // Create portfolio gallery
        let portfolio_photos_dir = temp_dir.path().join("portfolio");
        let portfolio_gallery_config = GallerySystemConfig {
            name: "portfolio".to_string(),
            url_prefix: "/my-portfolio".to_string(),
            source_directory: portfolio_photos_dir.to_string_lossy().to_string(),
            cache_directory: temp_dir
                .path()
                .join("cache/portfolio")
                .to_string_lossy()
                .to_string(),
            jpeg_quality: Some(90),
            webp_quality: Some(90.0),
            ..Default::default()
        };

        let portfolio_cache_dir = temp_dir.path().join("cache/portfolio");
        fs::create_dir_all(&portfolio_cache_dir).unwrap();
        let portfolio_source_storage: DynStorage =
            Arc::new(FilesystemStorage::new(&portfolio_photos_dir));
        let portfolio_cache_storage: DynStorage =
            Arc::new(FilesystemStorage::new(portfolio_cache_dir));
        let portfolio_gallery = Arc::new(Gallery::new(
            portfolio_gallery_config,
            portfolio_source_storage,
            portfolio_cache_storage,
        ));
        galleries.insert("portfolio".to_string(), portfolio_gallery);

        // Create posts config
        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let config = PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        };

        // Create posts manager with galleries
        let mut posts_manager = PostsManager::new(config, storage);
        posts_manager.set_galleries(Arc::new(galleries));

        // Load posts
        posts_manager.refresh_posts().await.unwrap();

        // Get the gallery test post
        let post = posts_manager.get_post("gallery-test").await.unwrap();

        // Check that gallery references were converted to HTML
        assert!(post.html_content.contains(
            r#"<a href="/gallery/detail/vacation/beach.jpg" class="gallery-image-link" data-gallery="main" data-image-path="vacation/beach.jpg">"#
        ));
        assert!(
            post.html_content
                .contains(r#"<img src="/gallery/_image/vacation/beach.jpg/thumbnail""#)
        );
        assert!(
            post.html_content
                .contains(r#"class="gallery-image gallery-image-thumbnail""#)
        );

        assert!(post.html_content.contains(
            r#"<a href="/gallery/detail/vacation/sunset.jpg" class="gallery-image-link" data-gallery="main" data-image-path="vacation/sunset.jpg">"#
        ));
        assert!(
            post.html_content
                .contains(r#"<img src="/gallery/_image/vacation/sunset.jpg/gallery""#)
        );
        assert!(
            post.html_content
                .contains(r#"class="gallery-image gallery-image-gallery""#)
        );

        assert!(post.html_content.contains(r#"<a href="/my-portfolio/detail/projects/app-screenshot.png" class="gallery-image-link" data-gallery="portfolio" data-image-path="projects/app-screenshot.png">"#));
        assert!(
            post.html_content
                .contains(r#"<img src="/my-portfolio/_image/projects/app-screenshot.png/medium""#)
        );
        assert!(
            post.html_content
                .contains(r#"class="gallery-image gallery-image-medium""#)
        );

        // Check that regular images are not converted
        assert!(
            post.html_content
                .contains(r#"<img src="https://example.com/image.jpg" alt="Regular Image""#)
        );
    }

    #[tokio::test]
    async fn test_post_reload_on_change() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        // Create initial post
        let initial_content = r#"+++
title = "Test Post"
summary = "Initial summary"
date = "2024-01-01"
+++

# Initial Content

This is the initial content."#;

        let post_path = posts_dir.join("test-post.md");
        fs::write(&post_path, initial_content).unwrap();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let config = PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        };

        let manager = PostsManager::new(config, storage);
        manager.refresh_posts().await.unwrap();

        // Get the initial post
        let post1 = manager.get_post("test-post").await.unwrap();
        assert_eq!(post1.title, "Test Post");
        assert_eq!(post1.summary, "Initial summary");
        assert!(post1.html_content.contains("Initial Content"));

        // Sleep briefly to ensure file modification time differs
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Modify the post
        let updated_content = r#"+++
title = "Updated Test Post"
summary = "Updated summary"
date = "2024-01-01"
+++

# Updated Content

This is the updated content with **bold text**."#;

        fs::write(&post_path, updated_content).unwrap();

        // Get the post again - it should automatically reload
        let post2 = manager.get_post("test-post").await.unwrap();
        assert_eq!(post2.title, "Updated Test Post");
        assert_eq!(post2.summary, "Updated summary");
        assert!(post2.html_content.contains("Updated Content"));
        assert!(post2.html_content.contains("<strong>bold text</strong>"));
    }

    #[tokio::test]
    async fn test_post_not_reloaded_when_unchanged() {
        let temp_dir = TempDir::new().unwrap();
        let posts_dir = temp_dir.path();

        let content = r#"+++
title = "Stable Post"
summary = "This post won't change"
date = "2024-01-01"
+++

# Stable Content"#;

        let post_path = posts_dir.join("stable-post.md");
        fs::write(&post_path, content).unwrap();

        let storage: DynStorage = Arc::new(FilesystemStorage::new(posts_dir));
        let config = PostsConfig {
            source_directory: posts_dir.to_string_lossy().to_string(),
            url_prefix: "/posts".to_string(),
            index_template: "modules/posts_index.html.liquid".to_string(),
            post_template: "modules/post_detail.html.liquid".to_string(),
            posts_per_page: 10,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        };

        let manager = PostsManager::new(config, storage);
        manager.refresh_posts().await.unwrap();

        // Get the post twice without modifications
        let post1 = manager.get_post("stable-post").await.unwrap();
        let post2 = manager.get_post("stable-post").await.unwrap();

        // The modification times should be the same (not reloaded)
        assert_eq!(post1.last_modified, post2.last_modified);
        assert_eq!(post1.title, post2.title);
    }
}
