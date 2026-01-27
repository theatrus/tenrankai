import { ThemeColorSet } from '@api/client';

export const DEFAULT_DARK_COLORS: Required<ThemeColorSet> = {
  bg_primary: '#1a1a1a',
  bg_secondary: '#2a2a2a',
  bg_card: '#606060',
  bg_hover: '#505050',
  header_bg: '#606060',
  text_primary: '#e0e0e0',
  text_secondary: '#b8b8b8',
  text_muted: '#cccccc',
  link_color: '#66b3ff',
  link_hover: '#99ccff',
  border_color: '#555555',
  accent_color: '#66b3ff',
  btn_danger_bg: '#dc3545',
};

export const DEFAULT_LIGHT_COLORS: Required<ThemeColorSet> = {
  bg_primary: '#ffffff',
  bg_secondary: '#f8f9fa',
  bg_card: '#ffffff',
  bg_hover: '#e9ecef',
  header_bg: '#f0f4f8',
  text_primary: '#212529',
  text_secondary: '#6c757d',
  text_muted: '#6c757d',
  link_color: '#0d6efd',
  link_hover: '#0a58ca',
  border_color: '#dee2e6',
  accent_color: '#0d6efd',
  btn_danger_bg: '#dc3545',
};

export const DEFAULT_FONTS = {
  font_body: 'Poppins',
  font_heading: 'Poppins',
  font_mono: 'Source Code Pro',
};

export type ColorFieldGroup = 'backgrounds' | 'text' | 'links' | 'accents';

export interface ColorFieldDef {
  key: keyof ThemeColorSet;
  label: string;
  group: ColorFieldGroup;
}

export const COLOR_FIELDS: ColorFieldDef[] = [
  // Backgrounds
  { key: 'bg_primary', label: 'Primary Background', group: 'backgrounds' },
  { key: 'bg_secondary', label: 'Secondary Background', group: 'backgrounds' },
  { key: 'bg_card', label: 'Card Background', group: 'backgrounds' },
  { key: 'bg_hover', label: 'Hover Background', group: 'backgrounds' },
  { key: 'header_bg', label: 'Header Background', group: 'backgrounds' },
  // Text
  { key: 'text_primary', label: 'Primary Text', group: 'text' },
  { key: 'text_secondary', label: 'Secondary Text', group: 'text' },
  { key: 'text_muted', label: 'Muted Text', group: 'text' },
  // Links
  { key: 'link_color', label: 'Link Color', group: 'links' },
  { key: 'link_hover', label: 'Link Hover', group: 'links' },
  // Accents
  { key: 'border_color', label: 'Border Color', group: 'accents' },
  { key: 'accent_color', label: 'Accent Color', group: 'accents' },
  { key: 'btn_danger_bg', label: 'Danger Button', group: 'accents' },
];

export const COLOR_GROUP_LABELS: Record<ColorFieldGroup, string> = {
  backgrounds: 'Background Colors',
  text: 'Text Colors',
  links: 'Link Colors',
  accents: 'Border & Accent Colors',
};

export const COLOR_GROUP_ORDER: ColorFieldGroup[] = ['backgrounds', 'text', 'links', 'accents'];
