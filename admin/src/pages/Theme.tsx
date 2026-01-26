import { useState, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api, ThemeConfig, ThemeColorSet } from '@api/client';

interface ColorInputProps {
  label: string;
  value?: string;
  onChange: (value: string | undefined) => void;
}

function ColorInput({ label, value, onChange }: ColorInputProps) {
  const [localValue, setLocalValue] = useState(value || '');

  useEffect(() => {
    setLocalValue(value || '');
  }, [value]);

  return (
    <div className="color-input-row">
      <label>{label}</label>
      <div className="color-input-controls">
        <input
          type="color"
          value={localValue || '#000000'}
          onChange={(e) => {
            setLocalValue(e.target.value);
            onChange(e.target.value);
          }}
        />
        <input
          type="text"
          value={localValue}
          placeholder="e.g., #1a1a1a"
          onChange={(e) => {
            setLocalValue(e.target.value);
            onChange(e.target.value || undefined);
          }}
        />
        {localValue && (
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
        )}
      </div>
    </div>
  );
}

interface FontInputProps {
  label: string;
  value?: string;
  onChange: (value: string | undefined) => void;
}

function FontInput({ label, value, onChange }: FontInputProps) {
  return (
    <div className="font-input-row">
      <label>{label}</label>
      <input
        type="text"
        value={value || ''}
        placeholder="e.g., 'Poppins', sans-serif"
        onChange={(e) => onChange(e.target.value || undefined)}
      />
    </div>
  );
}

interface ColorSectionProps {
  colors: ThemeColorSet;
  onChange: (colors: ThemeColorSet) => void;
}

function ColorSection({ colors, onChange }: ColorSectionProps) {
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
          onChange={handleChange('bg_primary')}
        />
        <ColorInput
          label="Secondary Background"
          value={colors.bg_secondary}
          onChange={handleChange('bg_secondary')}
        />
        <ColorInput
          label="Card Background"
          value={colors.bg_card}
          onChange={handleChange('bg_card')}
        />
        <ColorInput
          label="Hover Background"
          value={colors.bg_hover}
          onChange={handleChange('bg_hover')}
        />
        <ColorInput
          label="Header Background"
          value={colors.header_bg}
          onChange={handleChange('header_bg')}
        />
      </section>

      <section className="theme-section">
        <h3>Text Colors</h3>
        <ColorInput
          label="Primary Text"
          value={colors.text_primary}
          onChange={handleChange('text_primary')}
        />
        <ColorInput
          label="Secondary Text"
          value={colors.text_secondary}
          onChange={handleChange('text_secondary')}
        />
        <ColorInput
          label="Muted Text"
          value={colors.text_muted}
          onChange={handleChange('text_muted')}
        />
      </section>

      <section className="theme-section">
        <h3>Link Colors</h3>
        <ColorInput
          label="Link Color"
          value={colors.link_color}
          onChange={handleChange('link_color')}
        />
        <ColorInput
          label="Link Hover"
          value={colors.link_hover}
          onChange={handleChange('link_hover')}
        />
      </section>

      <section className="theme-section">
        <h3>Border &amp; Accent Colors</h3>
        <ColorInput
          label="Border Color"
          value={colors.border_color}
          onChange={handleChange('border_color')}
        />
        <ColorInput
          label="Accent Color"
          value={colors.accent_color}
          onChange={handleChange('accent_color')}
        />
        <ColorInput
          label="Danger Button"
          value={colors.btn_danger_bg}
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
        onChange={handleColorSetChange(activeTab)}
      />

      <div className="theme-sections">
        <section className="theme-section">
          <h3>Fonts</h3>
          <FontInput
            label="Body Font"
            value={theme.font_body}
            onChange={handleFontChange('font_body')}
          />
          <FontInput
            label="Heading Font"
            value={theme.font_heading}
            onChange={handleFontChange('font_heading')}
          />
          <FontInput
            label="Monospace Font"
            value={theme.font_mono}
            onChange={handleFontChange('font_mono')}
          />
        </section>
      </div>
    </div>
  );
}
