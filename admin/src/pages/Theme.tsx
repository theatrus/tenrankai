import { useState, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api, ThemeConfig } from '@api/client';

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

export function Theme() {
  const queryClient = useQueryClient();
  const [theme, setTheme] = useState<ThemeConfig>({});
  const [hasChanges, setHasChanges] = useState(false);

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

  const handleFieldChange = <K extends keyof ThemeConfig>(
    field: K,
    value: ThemeConfig[K]
  ) => {
    setTheme((prev) => ({ ...prev, [field]: value }));
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

      <div className="theme-sections">
        <section className="theme-section">
          <h3>Background Colors</h3>
          <ColorInput
            label="Primary Background"
            value={theme.bg_primary}
            onChange={(v) => handleFieldChange('bg_primary', v)}
          />
          <ColorInput
            label="Secondary Background"
            value={theme.bg_secondary}
            onChange={(v) => handleFieldChange('bg_secondary', v)}
          />
          <ColorInput
            label="Card Background"
            value={theme.bg_card}
            onChange={(v) => handleFieldChange('bg_card', v)}
          />
          <ColorInput
            label="Hover Background"
            value={theme.bg_hover}
            onChange={(v) => handleFieldChange('bg_hover', v)}
          />
          <ColorInput
            label="Header Background"
            value={theme.header_bg}
            onChange={(v) => handleFieldChange('header_bg', v)}
          />
        </section>

        <section className="theme-section">
          <h3>Text Colors</h3>
          <ColorInput
            label="Primary Text"
            value={theme.text_primary}
            onChange={(v) => handleFieldChange('text_primary', v)}
          />
          <ColorInput
            label="Secondary Text"
            value={theme.text_secondary}
            onChange={(v) => handleFieldChange('text_secondary', v)}
          />
          <ColorInput
            label="Muted Text"
            value={theme.text_muted}
            onChange={(v) => handleFieldChange('text_muted', v)}
          />
        </section>

        <section className="theme-section">
          <h3>Link Colors</h3>
          <ColorInput
            label="Link Color"
            value={theme.link_color}
            onChange={(v) => handleFieldChange('link_color', v)}
          />
          <ColorInput
            label="Link Hover"
            value={theme.link_hover}
            onChange={(v) => handleFieldChange('link_hover', v)}
          />
        </section>

        <section className="theme-section">
          <h3>Border &amp; Accent Colors</h3>
          <ColorInput
            label="Border Color"
            value={theme.border_color}
            onChange={(v) => handleFieldChange('border_color', v)}
          />
          <ColorInput
            label="Accent Color"
            value={theme.accent_color}
            onChange={(v) => handleFieldChange('accent_color', v)}
          />
          <ColorInput
            label="Danger Button"
            value={theme.btn_danger_bg}
            onChange={(v) => handleFieldChange('btn_danger_bg', v)}
          />
        </section>

        <section className="theme-section">
          <h3>Fonts</h3>
          <FontInput
            label="Body Font"
            value={theme.font_body}
            onChange={(v) => handleFieldChange('font_body', v)}
          />
          <FontInput
            label="Heading Font"
            value={theme.font_heading}
            onChange={(v) => handleFieldChange('font_heading', v)}
          />
          <FontInput
            label="Monospace Font"
            value={theme.font_mono}
            onChange={(v) => handleFieldChange('font_mono', v)}
          />
        </section>
      </div>
    </div>
  );
}
