export type FontCategory = 'sans-serif' | 'serif' | 'monospace' | 'display' | 'script' | 'slab-serif' | 'rounded';

export interface CuratedFont {
  family: string;
  weights: string[];
  category: FontCategory;
}

export const CATEGORY_LABELS: Record<FontCategory, string> = {
  'sans-serif': 'Sans Serif',
  'serif': 'Serif',
  'monospace': 'Monospace',
  'display': 'Display',
  'script': 'Script & Handwriting',
  'slab-serif': 'Slab Serif',
  'rounded': 'Rounded',
};

export const CATEGORY_ORDER: FontCategory[] = [
  'sans-serif', 'serif', 'slab-serif', 'display', 'script', 'rounded', 'monospace'
];

export const CURATED_FONTS: CuratedFont[] = [
  // Sans Serif
  { family: 'Poppins', weights: ['300', '400', '500', '600', '700'], category: 'sans-serif' },
  { family: 'Roboto', weights: ['300', '400', '500', '700'], category: 'sans-serif' },
  { family: 'Open Sans', weights: ['300', '400', '600', '700'], category: 'sans-serif' },
  { family: 'Lato', weights: ['300', '400', '700'], category: 'sans-serif' },
  { family: 'Montserrat', weights: ['300', '400', '500', '600', '700'], category: 'sans-serif' },
  { family: 'Inter', weights: ['300', '400', '500', '600', '700'], category: 'sans-serif' },
  { family: 'Raleway', weights: ['300', '400', '500', '600', '700'], category: 'sans-serif' },
  { family: 'Nunito', weights: ['300', '400', '600', '700'], category: 'sans-serif' },
  { family: 'Source Sans Pro', weights: ['300', '400', '600', '700'], category: 'sans-serif' },
  { family: 'PT Sans', weights: ['400', '700'], category: 'sans-serif' },

  // Serif
  { family: 'Merriweather', weights: ['300', '400', '700'], category: 'serif' },
  { family: 'Playfair Display', weights: ['400', '500', '600', '700'], category: 'serif' },
  { family: 'Lora', weights: ['400', '500', '600', '700'], category: 'serif' },
  { family: 'Libre Baskerville', weights: ['400', '700'], category: 'serif' },
  { family: 'Crimson Text', weights: ['400', '600', '700'], category: 'serif' },
  { family: 'EB Garamond', weights: ['400', '500', '600', '700'], category: 'serif' },
  { family: 'Cormorant Garamond', weights: ['300', '400', '500', '600', '700'], category: 'serif' },
  { family: 'Spectral', weights: ['300', '400', '500', '600', '700'], category: 'serif' },
  { family: 'PT Serif', weights: ['400', '700'], category: 'serif' },
  { family: 'Source Serif Pro', weights: ['300', '400', '600', '700'], category: 'serif' },

  // Slab Serif
  { family: 'Roboto Slab', weights: ['300', '400', '500', '700'], category: 'slab-serif' },
  { family: 'Zilla Slab', weights: ['300', '400', '500', '600', '700'], category: 'slab-serif' },
  { family: 'Bitter', weights: ['400', '500', '600', '700'], category: 'slab-serif' },

  // Display
  { family: 'Oswald', weights: ['300', '400', '500', '600', '700'], category: 'display' },
  { family: 'Bebas Neue', weights: ['400'], category: 'display' },
  { family: 'Abril Fatface', weights: ['400'], category: 'display' },
  { family: 'Josefin Sans', weights: ['300', '400', '500', '600', '700'], category: 'display' },

  // Script & Handwriting
  { family: 'Dancing Script', weights: ['400', '500', '600', '700'], category: 'script' },
  { family: 'Pacifico', weights: ['400'], category: 'script' },
  { family: 'Great Vibes', weights: ['400'], category: 'script' },
  { family: 'Caveat', weights: ['400', '500', '600', '700'], category: 'script' },
  { family: 'Sacramento', weights: ['400'], category: 'script' },

  // Rounded
  { family: 'Quicksand', weights: ['300', '400', '500', '600', '700'], category: 'rounded' },
  { family: 'Comfortaa', weights: ['300', '400', '500', '600', '700'], category: 'rounded' },
  { family: 'Varela Round', weights: ['400'], category: 'rounded' },

  // Monospace
  { family: 'Source Code Pro', weights: ['400', '500', '600', '700'], category: 'monospace' },
  { family: 'Fira Code', weights: ['400', '500', '600', '700'], category: 'monospace' },
  { family: 'JetBrains Mono', weights: ['400', '500', '600', '700'], category: 'monospace' },
  { family: 'IBM Plex Mono', weights: ['400', '500', '600', '700'], category: 'monospace' },
];

export function buildGoogleFontsUrl(fonts: CuratedFont[]): string {
  const families = fonts.map(f => {
    const weights = f.weights.join(';');
    return `family=${encodeURIComponent(f.family)}:wght@${weights}`;
  });
  return `https://fonts.googleapis.com/css2?${families.join('&')}&display=swap`;
}

export function getCssFallback(category: FontCategory): string {
  switch (category) {
    case 'monospace': return 'monospace';
    case 'serif':
    case 'slab-serif': return 'serif';
    case 'script': return 'cursive';
    default: return 'sans-serif';
  }
}

export function extractFontFamily(cssValue: string): string | undefined {
  const match = cssValue.match(/^'([^']+)'/);
  return match ? match[1] : undefined;
}
