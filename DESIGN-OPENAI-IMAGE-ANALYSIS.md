# OpenAI Image Analysis Design Document

## Overview

This document outlines the design for adding AI-powered image analysis to Tenrankai using OpenAI's Vision API. The feature will automatically generate keywords and accessibility alt-text for images in the gallery.

## Goals

1. Generate descriptive keywords for images to enable future search/filtering
2. Generate accessibility-friendly alt-text for screen readers
3. Support multiple trigger methods: CLI, API, and background processing
4. Respect rate limits and provide configurable delays between API calls
5. Only process images that don't already have AI-generated metadata

## Non-Goals

- Real-time analysis during image upload (may be added later)
- Support for multiple AI providers (OpenAI only for initial implementation)
- Image categorization or taxonomy beyond keywords

## Architecture

### Data Model Changes

Add new fields to `ImageUserMetadata` (stored in TOML sidecar files):

```rust
pub struct ImageUserMetadata {
    // Existing fields...
    pub comments: Vec<Comment>,
    pub highlighted: bool,
    pub pick_status: Option<PickStatus>,
    pub tags: Vec<String>,

    // New AI-generated fields
    pub ai_keywords: Vec<String>,
    pub ai_alt_text: Option<String>,
    pub ai_analyzed_at: Option<DateTime<Utc>>,
}
```

Example sidecar file after analysis:
```toml
highlighted = false
tags = ["vacation"]
ai_keywords = ["sunset", "beach", "ocean", "silhouette", "tropical"]
ai_alt_text = "A silhouette of a person walking along a tropical beach at sunset, with orange and purple clouds reflected in the calm ocean water"
ai_analyzed_at = "2026-01-11T08:00:00Z"
```

### New Module: `src/openai/`

Following the pattern established by the email module:

```
src/openai/
├── mod.rs           # Module exports, ImageAnalyzer trait
├── config.rs        # OpenAIConfig struct
├── types.rs         # Request/response types
├── error.rs         # Error types (thiserror)
├── client.rs        # OpenAI API client implementation
└── rate_limiter.rs  # Rate limiting logic
```

### Configuration

```toml
[openai]
# Required: OpenAI API key
api_key = "sk-..."

# Model for vision analysis (default: "gpt-5.2")
# Options: "gpt-5.2" (recommended), "gpt-4.1" (legacy), "gpt-4.1-mini" (cost-effective)
model = "gpt-5.2"

# Delay between API calls in milliseconds (default: 1000)
rate_limit_ms = 1000

# Maximum tokens for response (default: 300)
max_tokens = 300

# Enable automatic background analysis (default: false)
enable_background_analysis = false

# Interval for background runs in minutes (default: 60)
background_interval_minutes = 60

# Max images per background run (default: 50)
background_batch_size = 50
```

### Trigger Methods

#### 1. CLI Command

```bash
# Analyze all images in a gallery
cargo run -- analyze-images --gallery main

# Analyze specific folder
cargo run -- analyze-images --gallery main --folder vacation/2024

# Dry run (show what would be analyzed)
cargo run -- analyze-images --gallery main --dry-run

# Force re-analysis of images that already have AI data
cargo run -- analyze-images --gallery main --force

# Limit number of images to process
cargo run -- analyze-images --gallery main --limit 100
```

#### 2. API Endpoints

New endpoints with `can_analyze_images` permission:

```
POST /api/gallery/{name}/analyze/{*image_path}
  - Analyze a single image
  - Body: { "force": false }
  - Returns: { "keywords": [...], "alt_text": "...", "analyzed_at": "..." }

POST /api/gallery/{name}/analyze-folder/{*folder_path}
  - Start background analysis of folder
  - Body: { "force": false, "limit": 50 }
  - Returns: { "task_id": "...", "total_images": 42 }
```

#### 3. Background Processing

When `enable_background_analysis = true`:
- Runs every `background_interval_minutes`
- Processes up to `background_batch_size` images per run
- Only processes images without existing AI metadata
- Respects rate limiting between API calls
- Gracefully handles server shutdown

### Image Processing Flow

```
1. Identify image needing analysis
   └─ Check if ai_analyzed_at is None (unless --force)

2. Generate medium-sized JPEG
   └─ Use existing image processing pipeline
   └─ Target: 1200x1200 max dimensions
   └─ Quality: 85 (configurable)

3. Encode image for API
   └─ Base64 encode the JPEG bytes

4. Call OpenAI Vision API
   └─ Wait for rate limiter
   └─ Send structured prompt requesting keywords and alt-text
   └─ Parse JSON response

5. Update metadata
   └─ Set ai_keywords, ai_alt_text, ai_analyzed_at
   └─ Save to sidecar TOML file

6. Log progress
   └─ Report success/failure
   └─ Track statistics
```

### OpenAI API Request

Using GPT-5.2 with the Responses API (recommended over legacy Chat Completions API):

**Endpoint:** `POST https://api.openai.com/v1/responses`

```json
{
  "model": "gpt-5.2",
  "input": [
    {
      "role": "user",
      "content": [
        {
          "type": "input_text",
          "text": "Analyze this image and provide descriptive keywords and alt-text for accessibility."
        },
        {
          "type": "input_image",
          "image_url": "data:image/jpeg;base64,{base64_image}",
          "detail": "auto"
        }
      ]
    }
  ],
  "text": {
    "format": {
      "type": "json_schema",
      "name": "image_analysis",
      "schema": {
        "type": "object",
        "properties": {
          "keywords": {
            "type": "array",
            "items": { "type": "string" },
            "description": "5-10 descriptive keywords for the image"
          },
          "alt_text": {
            "type": "string",
            "description": "1-2 sentence description suitable for screen readers"
          }
        },
        "required": ["keywords", "alt_text"],
        "additionalProperties": false
      },
      "strict": true
    }
  },
  "max_output_tokens": 300
}
```

**Response Format:**
```json
{
  "id": "resp_...",
  "output": [
    {
      "type": "message",
      "role": "assistant",
      "content": [
        {
          "type": "output_text",
          "text": "{\"keywords\": [\"sunset\", \"beach\", \"ocean\", \"silhouette\", \"tropical\"], \"alt_text\": \"A silhouette of a person walking along a tropical beach at sunset, with orange and purple clouds reflected in calm ocean water.\"}"
        }
      ]
    }
  ]
}
```

**Note:** The Responses API uses `input` instead of `messages`, `input_image`/`input_text` content types, and `max_output_tokens` instead of `max_tokens`. Structured outputs ensure consistent JSON parsing.

### Permission System

Add new permission to `RolePermissions`:

```rust
pub can_analyze_images: bool,
```

This permission:
- Required for API endpoints
- Typically granted to admin/owner roles
- Not needed for CLI (runs with full access)

### Frontend Integration

#### TypeScript Types

```typescript
export interface ImageUserMetadata {
  // Existing fields...
  ai_keywords?: string[];
  ai_alt_text?: string;
  ai_analyzed_at?: string;
}
```

#### Template Usage

```html
<!-- Use AI alt-text for accessibility -->
<img
  src="{{ image.medium_url }}"
  alt="{% if image.user_metadata.ai_alt_text %}{{ image.user_metadata.ai_alt_text }}{% else %}{{ image.name }}{% endif %}"
/>

<!-- Display AI keywords -->
{% if image.user_metadata.ai_keywords.size > 0 %}
<div class="ai-keywords">
  {% for keyword in image.user_metadata.ai_keywords %}
    <span class="keyword-tag">{{ keyword }}</span>
  {% endfor %}
</div>
{% endif %}
```

## Implementation Phases

### Phase 1: Core Infrastructure
- Add AI fields to `ImageUserMetadata`
- Create OpenAI module structure
- Implement configuration parsing
- Add `can_analyze_images` permission

### Phase 2: OpenAI Client
- Implement rate limiter
- Create OpenAI Vision API client
- Add error handling and retries
- Write unit tests

### Phase 3: CLI Command
- Add `analyze-images` subcommand
- Implement batch processing
- Add progress reporting
- Support dry-run mode

### Phase 4: API Endpoints
- Implement single image analysis endpoint
- Implement folder analysis endpoint
- Add permission checks
- Write integration tests

### Phase 5: Background Processing
- Implement background task
- Integrate with server startup
- Add graceful shutdown handling
- Test with various configurations

### Phase 6: Frontend
- Update TypeScript types
- Update templates for alt-text
- Add keyword display in metadata section
- Style keyword tags

## Dependencies

New crates required:
- `reqwest` (already present) - HTTP client for OpenAI API
- `base64` (already present) - Image encoding

## Security Considerations

1. **API Key Storage**: Store in config file or environment variable, never commit
2. **Rate Limiting**: Prevent abuse of API endpoints with existing rate limiting
3. **Permission Control**: Require explicit permission for analysis operations
4. **Cost Control**: Batch size limits prevent runaway API costs

## Testing Strategy

1. **Unit Tests**
   - Rate limiter timing
   - Config parsing
   - Response parsing

2. **Integration Tests**
   - Mock OpenAI API responses
   - CLI command execution
   - API endpoint behavior

3. **Manual Testing**
   - Real API calls with test images
   - Background processing behavior
   - Frontend display verification

## Cost Estimation

GPT-5.2 pricing (check OpenAI pricing page for current rates):
- Estimated ~$0.005-0.02 per image depending on detail level
- 1000 images ≈ $5-20

Recommended approach:
- Start with CLI for controlled batch processing
- Use background processing sparingly
- Monitor API usage through OpenAI dashboard

## Future Enhancements

1. **Multiple AI Providers**: Abstract analyzer trait for Claude, Gemini, etc.
2. **Custom Prompts**: Per-gallery prompt customization
3. **Confidence Scores**: Store and display confidence for keywords
4. **Search Integration**: Use keywords for gallery search
5. **Automatic Triggers**: Analyze new images on upload
