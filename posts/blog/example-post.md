+++
title = "Welcome to Tenrankai Blog"
summary = "This is an example blog post showing the markdown format for Tenrankai's posts system."
date = "2024-08-24"
+++

# Welcome to Tenrankai Blog

This is an example blog post demonstrating the format for posts in Tenrankai. Each post is a markdown file with YAML front matter containing metadata.

## Front Matter

Every post must start with TOML front matter between `+++` delimiters. The required fields are:

- **title**: The title of your post
- **summary**: A brief summary that appears in the post listing
- **date**: The publication date (supports both YYYY-MM-DD and full RFC3339 format)

## Markdown Features

Posts support all standard markdown features:

### Text Formatting

You can use **bold text**, *italic text*, and ~~strikethrough text~~.

### Lists

Unordered lists:
- Item one
- Item two
- Item three

Ordered lists:
1. First item
2. Second item
3. Third item

### Code Blocks

```rust
fn main() {
    println!("Hello from Tenrankai!");
}
```

### Blockquotes

> This is a blockquote. It's useful for highlighting important information or quotes.

### Tables

| Feature | Description |
|---------|-------------|
| Markdown | Full CommonMark support |
| Extensions | Tables, strikethrough, footnotes |
| Syntax Highlighting | Code blocks with language hints |

### Links and Images

You can include [links to other sites](https://example.com) and images:

![gallery:main:2009-hawaii/_MG_3149.jpg](gallery)

## Directory Structure

Posts can be organized in subdirectories. The URL slug will include the directory path. For example:

- `posts/blog/2024/my-post.md` → `/blog/2024/my-post`
- `posts/blog/tutorials/rust-basics.md` → `/blog/tutorials/rust-basics`

## Multiple Post Systems

Tenrankai supports multiple independent post systems. You might have:
- `/blog` for your blog posts
- `/stories` for creative writing
- `/instructions` for documentation
- `/recipes` for cooking recipes

Each system can have its own templates and configuration.

## Refreshing Posts

Posts are loaded and cached when the server starts. To refresh posts while the server is running, you can use the refresh API:

```bash
curl -X POST http://localhost:3000/api/posts/blog/refresh
```

This will scan the directory and update the cache with any new or modified posts.

---

*This is an example post. Feel free to delete or modify it for your own use!*
