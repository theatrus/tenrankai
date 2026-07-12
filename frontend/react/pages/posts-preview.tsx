import React, { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';

interface PreviewCategory {
  name: string;
  url: string;
}

interface PreviewPost {
  url: string;
  title: string;
  summary: string;
  date: string;
  date_formatted: string;
  reading_time_minutes: number;
  hero_image?: string | null;
  categories: PreviewCategory[];
}

interface PostsPreviewProps {
  postsName: string;
  count?: number;
  category?: string;
  host: HTMLElement;
}

const PostsPreview: React.FC<PostsPreviewProps> = ({ postsName, count, category, host }) => {
  const [posts, setPosts] = useState<PreviewPost[] | null>(null);

  useEffect(() => {
    const params = new URLSearchParams();
    if (count) params.set('count', String(count));
    if (category) params.set('category', category);
    const query = params.toString();

    fetch(`/api/posts/${encodeURIComponent(postsName)}/preview${query ? `?${query}` : ''}`)
      .then((response) => {
        if (!response.ok) throw new Error(`Failed to fetch posts preview: ${response.status}`);
        return response.json();
      })
      .then((data: { posts: PreviewPost[] }) => {
        if (!data.posts || data.posts.length === 0) {
          host.style.display = 'none';
        } else {
          setPosts(data.posts);
        }
      })
      .catch((error) => {
        console.error('Failed to initialize posts preview:', error);
        host.style.display = 'none';
      });
  }, [postsName, count, category, host]);

  if (!posts) return null;

  return (
    <>
      {posts.map((post) => (
        <a key={post.url} className="posts-preview-item" href={post.url}>
          {post.hero_image && (
            <span className="posts-preview-thumb">
              <img src={post.hero_image} alt="" loading="lazy" />
            </span>
          )}
          <span className="posts-preview-body">
            {post.categories.length > 0 && (
              <span className="posts-preview-categories">
                {post.categories.map((c) => (
                  <span key={c.url} className="posts-preview-category">
                    {c.name}
                  </span>
                ))}
              </span>
            )}
            <span className="posts-preview-title">{post.title}</span>
            <span className="posts-preview-meta">
              <time dateTime={post.date}>{post.date_formatted}</time>
              {' · '}
              {post.reading_time_minutes} min read
            </span>
            <span className="posts-preview-summary">{post.summary}</span>
          </span>
        </a>
      ))}
    </>
  );
};

document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll<HTMLElement>('.posts-preview-component').forEach((host) => {
    const list = host.querySelector<HTMLElement>('.posts-preview-list');
    const postsName = host.dataset.postsName;
    if (!list || !postsName) return;

    const count = parseInt(host.dataset.count || '', 10) || undefined;
    const category = host.dataset.category || undefined;

    const root = createRoot(list);
    root.render(
      <PostsPreview postsName={postsName} count={count} category={category} host={host} />,
    );
  });
});
