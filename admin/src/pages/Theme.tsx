import { useState, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api, ThemeConfig, ThemeColorSet, GoogleFontConfig } from '@api/client';

interface CuratedFont {
  family: string;
  weights: string[];
  category: 'sans-serif' | 'serif' | 'monospace';
}

const DEFAULT_DARK_COLORS: Required<ThemeColorSet> = {
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

const DEFAULT_LIGHT_COLORS: Required<ThemeColorSet> = {
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

const DEFAULT_FONTS = {
  font_body: 'Poppins',
  font_heading: 'Poppins',
  font_mono: 'Consolas',
};

const CURATED_FONTS: CuratedFont[] = [
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
  { family: 'Merriweather', weights: ['300', '400', '700'], category: 'serif' },
  { family: 'Playfair Display', weights: ['400', '500', '600', '700'], category: 'serif' },
  { family: 'Lora', weights: ['400', '500', '600', '700'], category: 'serif' },
  { family: 'Libre Baskerville', weights: ['400', '700'], category: 'serif' },
  { family: 'Source Code Pro', weights: ['400', '500', '600', '700'], category: 'monospace' },
  { family: 'Fira Code', weights: ['400', '500', '600', '700'], category: 'monospace' },
  { family: 'JetBrains Mono', weights: ['400', '500', '600', '700'], category: 'monospace' },
  { family: 'IBM Plex Mono', weights: ['400', '500', '600', '700'], category: 'monospace' },
];

interface ColorInputProps {
  label: string;
  value?: string;
  defaultValue: string;
  onChange: (value: string | undefined) => void;
}

function ColorInput({ label, value, defaultValue, onChange }: ColorInputProps) {
  const [localValue, setLocalValue] = useState(value || '');

  useEffect(() => {
    setLocalValue(value || '');
  }, [value]);

  const displayColor = localValue || defaultValue;

  return (
    <div className="color-input-row">
      <label>{label}</label>
      <div className="color-input-controls">
        <input
          type="color"
          value={displayColor}
          onChange={(e) => {
            setLocalValue(e.target.value);
            onChange(e.target.value);
          }}
        />
        <input
          type="text"
          value={localValue}
          placeholder={`Default: ${defaultValue}`}
          onChange={(e) => {
            setLocalValue(e.target.value);
            onChange(e.target.value || undefined);
          }}
        />
        {localValue ? (
          <button
            type="button"
            className="btn-clear"
            onClick={() => {
              setLocalValue('');
              onChange(undefined);
            }}
          >
            Clear
          </button>
        ) : (
          <span className="color-default-badge">default</span>
        )}
      </div>
    </div>
  );
}

interface FontSelectProps {
  label: string;
  category: 'sans-serif' | 'serif' | 'monospace' | 'all';
  value?: string;
  defaultFont: string;
  onChange: (value: string | undefined, googleFont?: GoogleFontConfig) => void;
}

function FontSelect({ label, category, value, defaultFont, onChange }: FontSelectProps) {
  const fonts = CURATED_FONTS.filter(f =>
    category === 'all' || f.category === category
  );

  const selectedFamily = value ? extractFontFamily(value) : undefined;

  return (
    <div className="font-input-row">
      <label>{label}</label>
      <select
        value={selectedFamily || ''}
        onChange={(e) => {
          const fontFamily = e.target.value;
          if (!fontFamily) {
            onChange(undefined, undefined);
            return;
          }
          const font = fonts.find(f => f.family === fontFamily);
          if (font) {
            onChange(`'${font.family}', ${font.category}`, { family: font.family, weights: font.weights });
          }
        }}
      >
        <option value="">{defaultFont} (Default)</option>
        {fonts.map(f => (
          <option key={f.family} value={f.family}>{f.family}</option>
        ))}
      </select>
    </div>
  );
}

function extractFontFamily(cssValue: string): string | undefined {
  const match = cssValue.match(/^'([^']+)'/);
  return match ? match[1] : undefined;
}

interface ColorSectionProps {
  colors: ThemeColorSet;
  defaults: Required<ThemeColorSet>;
  onChange: (colors: ThemeColorSet) => void;
}

function ColorSection({ colors, defaults, onChange }: ColorSectionProps) {
  const handleChange = (key: keyof ThemeColorSet) => (value: string | undefined) => {
    const updated = { ...colors };
    if (value === undefined) {
      delete updated[key];
    } else {
      updated[key] = value;
    }
    onChange(updated);
  };

  return (
    <div className="theme-sections">
      <section className="theme-section">
        <h3>Background Colors</h3>
        <ColorInput
          label="Primary Background"
          value={colors.bg_primary}
          defaultValue={defaults.bg_primary}
          onChange={handleChange('bg_primary')}
        />
        <ColorInput
          label="Secondary Background"
          value={colors.bg_secondary}
          defaultValue={defaults.bg_secondary}
          onChange={handleChange('bg_secondary')}
        />
        <ColorInput
          label="Card Background"
          value={colors.bg_card}
          defaultValue={defaults.bg_card}
          onChange={handleChange('bg_card')}
        />
        <ColorInput
          label="Hover Background"
          value={colors.bg_hover}
          defaultValue={defaults.bg_hover}
          onChange={handleChange('bg_hover')}
        />
        <ColorInput
          label="Header Background"
          value={colors.header_bg}
          defaultValue={defaults.header_bg}
          onChange={handleChange('header_bg')}
        />
      </section>

      <section className="theme-section">
        <h3>Text Colors</h3>
        <ColorInput
          label="Primary Text"
          value={colors.text_primary}
          defaultValue={defaults.text_primary}
          onChange={handleChange('text_primary')}
        />
        <ColorInput
          label="Secondary Text"
          value={colors.text_secondary}
          defaultValue={defaults.text_secondary}
          onChange={handleChange('text_secondary')}
        />
        <ColorInput
          label="Muted Text"
          value={colors.text_muted}
          defaultValue={defaults.text_muted}
          onChange={handleChange('text_muted')}
        />
      </section>

      <section className="theme-section">
        <h3>Link Colors</h3>
        <ColorInput
          label="Link Color"
          value={colors.link_color}
          defaultValue={defaults.link_color}
          onChange={handleChange('link_color')}
        />
        <ColorInput
          label="Link Hover"
          value={colors.link_hover}
          defaultValue={defaults.link_hover}
          onChange={handleChange('link_hover')}
        />
      </section>

      <section className="theme-section">
        <h3>Border &amp; Accent Colors</h3>
        <ColorInput
          label="Border Color"
          value={colors.border_color}
          defaultValue={defaults.border_color}
          onChange={handleChange('border_color')}
        />
        <ColorInput
          label="Accent Color"
          value={colors.accent_color}
          defaultValue={defaults.accent_color}
          onChange={handleChange('accent_color')}
        />
        <ColorInput
          label="Danger Button"
          value={colors.btn_danger_bg}
          defaultValue={defaults.btn_danger_bg}
          onChange={handleChange('btn_danger_bg')}
        />
      </section>
    </div>
  );
}

type ColorMode = 'dark' | 'light';

export function Theme() {
  const queryClient = useQueryClient();
  const [theme, setTheme] = useState<ThemeConfig>({});
  const [hasChanges, setHasChanges] = useState(false);
  const [activeTab, setActiveTab] = useState<ColorMode>('dark');

  const { data, isLoading, error } = useQuery({
    queryKey: ['theme'],
    queryFn: api.getTheme,
  });

  useEffect(() => {
    if (data) {
      setTheme(data);
      setHasChanges(false);
    }
  }, [data]);

  const updateMutation = useMutation({
    mutationFn: api.updateTheme,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['theme'] });
      setHasChanges(false);
    },
  });

  const resetMutation = useMutation({
    mutationFn: api.resetTheme,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['theme'] });
      setTheme({});
      setHasChanges(false);
    },
  });

  const handleColorSetChange = (mode: ColorMode) => (colors: ThemeColorSet) => {
    const isEmpty = Object.keys(colors).length === 0;
    setTheme((prev) => ({
      ...prev,
      [mode]: isEmpty ? undefined : colors,
    }));
    setHasChanges(true);
  };

  const handleForceColorSchemeChange = (value: string) => {
    setTheme((prev) => ({
      ...prev,
      force_color_scheme: value || undefined,
    }));
    setHasChanges(true);
  };

  const handleFontChange = (key: 'font_body' | 'font_heading' | 'font_mono') => (value: string | undefined, googleFont?: GoogleFontConfig) => {
    setTheme((prev) => {
      const updated = { ...prev, [key]: value };

      // Update google_fonts array
      const existingFonts = prev.google_fonts || [];

      if (googleFont) {
        // Check if this font is already in the list
        const hasFont = existingFonts.some(f => f.family === googleFont.family);
        if (!hasFont) {
          updated.google_fonts = [...existingFonts, googleFont];
        }
      }

      // Clean up google_fonts: only keep fonts that are actually in use
      const usedFamilies = new Set<string>();
      const fontKeys: ('font_body' | 'font_heading' | 'font_mono')[] = ['font_body', 'font_heading', 'font_mono'];
      for (const k of fontKeys) {
        const fontValue = k === key ? value : prev[k];
        if (fontValue) {
          const family = extractFontFamily(fontValue);
          if (family) usedFamilies.add(family);
        }
      }

      updated.google_fonts = (updated.google_fonts || []).filter(f => usedFamilies.has(f.family));

      return updated;
    });
    setHasChanges(true);
  };

  const handleSave = () => {
    updateMutation.mutate(theme);
  };

  const handleReset = () => {
    if (confirm('Reset theme to defaults? This will remove all custom colors and fonts.')) {
      resetMutation.mutate();
    }
  };

  if (isLoading) {
    return <div className="loading">Loading theme...</div>;
  }

  if (error) {
    return <div className="error">Error loading theme: {(error as Error).message}</div>;
  }

  const currentColors = activeTab === 'dark' ? (theme.dark || {}) : (theme.light || {});
  const currentDefaults = activeTab === 'dark' ? DEFAULT_DARK_COLORS : DEFAULT_LIGHT_COLORS;

  return (
    <div className="theme-editor">
      <div className="page-header">
        <h2>Theme Editor</h2>
        <div className="header-actions">
          <button
            className="btn btn-secondary"
            onClick={handleReset}
            disabled={resetMutation.isPending}
          >
            Reset to Defaults
          </button>
          <button
            className="btn btn-primary"
            onClick={handleSave}
            disabled={!hasChanges || updateMutation.isPending}
          >
            {updateMutation.isPending ? 'Saving...' : 'Save Changes'}
          </button>
        </div>
      </div>

      {updateMutation.isSuccess && (
        <div className="message success">Theme saved successfully. Reload the page to see changes.</div>
      )}

      {updateMutation.isError && (
        <div className="message error">
          Error saving theme: {(updateMutation.error as Error).message}
        </div>
      )}

      <div className="theme-settings-section">
        <section className="theme-section">
          <h3>Color Scheme Settings</h3>
          <div className="form-row">
            <label htmlFor="force-color-scheme">Force Color Scheme</label>
            <select
              id="force-color-scheme"
              value={theme.force_color_scheme || ''}
              onChange={(e) => handleForceColorSchemeChange(e.target.value)}
            >
              <option value="">User Choice (Auto)</option>
              <option value="dark">Always Dark</option>
              <option value="light">Always Light</option>
            </select>
            <span className="form-help">
              When set, users cannot switch between dark and light mode.
            </span>
          </div>
        </section>
      </div>

      <div className="theme-tabs">
        <button
          className={`theme-tab ${activeTab === 'dark' ? 'active' : ''}`}
          onClick={() => setActiveTab('dark')}
        >
          Dark Mode Colors
        </button>
        <button
          className={`theme-tab ${activeTab === 'light' ? 'active' : ''}`}
          onClick={() => setActiveTab('light')}
        >
          Light Mode Colors
        </button>
      </div>

      <ColorSection
        colors={currentColors}
        defaults={currentDefaults}
        onChange={handleColorSetChange(activeTab)}
      />

      <div className="theme-sections">
        <section className="theme-section">
          <h3>Fonts</h3>
          <FontSelect
            label="Body Font"
            category="all"
            value={theme.font_body}
            defaultFont={DEFAULT_FONTS.font_body}
            onChange={handleFontChange('font_body')}
          />
          <FontSelect
            label="Heading Font"
            category="all"
            value={theme.font_heading}
            defaultFont={DEFAULT_FONTS.font_heading}
            onChange={handleFontChange('font_heading')}
          />
          <FontSelect
            label="Monospace Font"
            category="monospace"
            value={theme.font_mono}
            defaultFont={DEFAULT_FONTS.font_mono}
            onChange={handleFontChange('font_mono')}
          />
        </section>
      </div>
    </div>
  );
}
