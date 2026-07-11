// API client for the posts editor

export interface PostSource {
  slug: string;
  title: string;
  summary: string;
  date: string;
  categories: string[];
  hero_image: string | null;
  content: string;
}

export interface PostWriteRequest {
  title: string;
  summary: string;
  categories: string[];
  hero_image?: string;
  content: string;
}

export interface PostWriteResponse {
  slug: string;
  url: string;
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    credentials: 'same-origin',
    ...init,
  });

  if (!response.ok) {
    const errorData = await response.json().catch(() => ({}));
    throw new PostsApiError(
      errorData.message || `HTTP ${response.status}: ${response.statusText}`,
      response.status
    );
  }

  // Some endpoints (e.g. delete) reply with plain text rather than JSON
  if (!response.headers.get('content-type')?.includes('application/json')) {
    return undefined as T;
  }
  return (await response.json()) as T;
}

export class PostsApiClient {
  async getSource(postsName: string, slug: string): Promise<PostSource> {
    return request(`/api/posts/${encodeURIComponent(postsName)}/source/${slug}`);
  }

  async createPost(
    postsName: string,
    slug: string,
    post: PostWriteRequest
  ): Promise<PostWriteResponse> {
    return request(`/api/posts/${encodeURIComponent(postsName)}/source`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ slug, ...post }),
    });
  }

  async updatePost(
    postsName: string,
    slug: string,
    post: PostWriteRequest
  ): Promise<PostWriteResponse> {
    return request(`/api/posts/${encodeURIComponent(postsName)}/source/${slug}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(post),
    });
  }

  async deletePost(postsName: string, slug: string): Promise<void> {
    await request(`/api/posts/${encodeURIComponent(postsName)}/source/${slug}`, {
      method: 'DELETE',
    });
  }
}

export class PostsApiError extends Error {
  constructor(
    public override message: string,
    public status: number
  ) {
    super(message);
    this.name = 'PostsApiError';
  }
}

export const postsApi = new PostsApiClient();
