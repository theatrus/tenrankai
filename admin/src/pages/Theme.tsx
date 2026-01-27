import { useState, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api, ThemeConfig, ThemeColorSet, GoogleFontConfig } from '@api/client';
import {
  FontCategory,
  CATEGORY_LABELS,
  CATEGORY_ORDER,
  CURATED_FONTS,
  buildGoogleFontsUrl,
  getCssFallback,
  extractFontFamily,
} from '../constants/curated-fonts';
import {
  DEFAULT_DARK_COLORS,
  DEFAULT_LIGHT_COLORS,
  DEFAULT_FONTS,
  COLOR_FIELDS,
  COLOR_GROUP_LABELS,
  COLOR_GROUP_ORDER,
  ColorFieldGroup,
} from '../constants/theme-defaults';

const FONT_LOADER_ID = 'theme-editor-fonts';

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

function useFontLoader() {
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (document.getElementById(FONT_LOADER_ID)) {
      setLoaded(true);
      return;
    }

    const link = document.createElement('link');
    link.id = FONT_LOADER_ID;
    link.rel = 'stylesheet';
    link.href = buildGoogleFontsUrl(CURATED_FONTS);
    link.onload = () => setLoaded(true);
    document.head.appendChild(link);
  }, []);

  return loaded;
}

type FontSelectCategory = FontCategory | 'all';

interface FontSelectProps {
  label: string;
  category: FontSelectCategory;
  value?: string;
  defaultFont: string;
  sampleText?: string;
  onChange: (value: string | undefined, googleFont?: GoogleFontConfig) => void;
}

function FontSelect({ label, category, value, defaultFont, sampleText, onChange }: FontSelectProps) {
  const fonts = CURATED_FONTS.filter(f =>
    category === 'all' || f.category === category
  );

  const selectedFamily = value ? extractFontFamily(value) : defaultFont;
  const currentFont = CURATED_FONTS.find(f => f.family === selectedFamily) || CURATED_FONTS.find(f => f.family === defaultFont);
  const cssFallback = currentFont ? getCssFallback(currentFont.category) : 'sans-serif';

  const sample = sampleText || (category === 'monospace'
    ? 'const x = 42; // code sample'
    : 'The quick brown fox jumps over the lazy dog');

  const fontsByCategory = CATEGORY_ORDER
    .filter(cat => category === 'all' || cat === category)
    .map(cat => ({
      category: cat,
      label: CATEGORY_LABELS[cat],
      fonts: fonts.filter(f => f.category === cat),
    }))
    .filter(group => group.fonts.length > 0);

  return (
    <div className="font-select-group">
      <div className="font-input-row">
        <label>{label}</label>
        <select
          value={selectedFamily}
          onChange={(e) => {
            const fontFamily = e.target.value;
            if (fontFamily === defaultFont) {
              onChange(undefined, undefined);
              return;
            }
            const font = CURATED_FONTS.find(f => f.family === fontFamily);
            if (font) {
              onChange(`'${font.family}', ${getCssFallback(font.category)}`, { family: font.family, weights: font.weights });
            }
          }}
        >
          {fontsByCategory.map(group => (
            <optgroup key={group.category} label={group.label}>
              {group.fonts.map(f => (
                <option key={f.family} value={f.family}>
                  {f.family}{f.family === defaultFont ? ' (Default)' : ''}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
      </div>
      <div
        className="font-preview"
        style={{ fontFamily: `'${selectedFamily}', ${cssFallback}` }}
      >
        {sample}
      </div>
    </div>
  );
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

  const renderGroup = (group: ColorFieldGroup) => {
    const fields = COLOR_FIELDS.filter(f => f.group === group);
    return (
      <section key={group} className="theme-section">
        <h3>{COLOR_GROUP_LABELS[group]}</h3>
        {fields.map(field => (
          <ColorInput
            key={field.key}
            label={field.label}
            value={colors[field.key]}
            defaultValue={defaults[field.key]}
            onChange={handleChange(field.key)}
          />
        ))}
      </section>
    );
  };

  return (
    <div className="theme-sections">
      {COLOR_GROUP_ORDER.map(renderGroup)}
    </div>
  );
}

function collectUsedFonts(theme: ThemeConfig): GoogleFontConfig[] {
  const usedFamilies = new Set<string>();
  for (const key of ['font_body', 'font_heading', 'font_mono'] as const) {
    const family = theme[key] && extractFontFamily(theme[key]!);
    if (family) usedFamilies.add(family);
  }
  return CURATED_FONTS
    .filter(f => usedFamilies.has(f.family))
    .map(f => ({ family: f.family, weights: f.weights }));
}

type ColorMode = 'dark' | 'light';

export function Theme() {
  const queryClient = useQueryClient();
  const [theme, setTheme] = useState<ThemeConfig>({});
  const [hasChanges, setHasChanges] = useState(false);
  const [activeTab, setActiveTab] = useState<ColorMode>('dark');

  useFontLoader();

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
    mutationFn: (themeToSave: ThemeConfig) => {
      const withFonts = { ...themeToSave, google_fonts: collectUsedFonts(themeToSave) };
      return api.updateTheme(withFonts);
    },
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

  const handleFontChange = (key: 'font_body' | 'font_heading' | 'font_mono') => (value: string | undefined) => {
    setTheme((prev) => ({ ...prev, [key]: value }));
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
