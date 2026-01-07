/**
 * Theme Toggle Functionality
 * 
 * This script provides:
 * 1. OS-based automatic theme detection
 * 2. Manual theme toggle with persistence
 * 3. Smooth theme transitions
 */

type Theme = 'light' | 'dark' | 'auto';

class ThemeManager {
    private currentTheme: Theme;
    constructor() {
        this.currentTheme = this.getStoredTheme() || 'light';  // Default to light mode
        this.init();
    }

    /**
     * Get the stored theme preference from localStorage
     */
    private getStoredTheme(): Theme | null {
        const stored = localStorage.getItem('theme-preference');
        if (stored === 'light' || stored === 'dark' || stored === 'auto') {
            return stored;
        }
        return null;
    }

    /**
     * Get the OS theme preference
     */
    private getOSTheme(): 'light' | 'dark' {
        return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
    }

    /**
     * Set the theme on the document
     */
    private setTheme(theme: Theme): void {
        const root = document.documentElement;
        
        // Add transitioning class to prevent flash
        root.classList.add('theme-transitioning');
        
        if (theme === 'auto') {
            // Remove any explicit theme, let OS preference take over
            root.removeAttribute('data-theme');
            this.currentTheme = 'auto';
        } else {
            // Set explicit theme
            root.setAttribute('data-theme', theme);
            this.currentTheme = theme;
        }
        
        // Store the preference
        localStorage.setItem('theme-preference', this.currentTheme);
        
        // Remove transitioning class after animation
        setTimeout(() => {
            root.classList.remove('theme-transitioning');
        }, 300);
        
        // Update toggle button
        this.updateToggleButton();
    }

    /**
     * Toggle between light, dark, and auto themes
     */
    public toggleTheme(): void {
        const currentTheme = this.getCurrentEffectiveTheme();
        
        if (this.currentTheme === 'auto') {
            // From auto -> opposite of current OS theme
            this.setTheme(currentTheme === 'light' ? 'dark' : 'light');
        } else if (this.currentTheme === 'light') {
            // From light -> dark
            this.setTheme('dark');
        } else {
            // From dark -> auto
            this.setTheme('auto');
        }
    }

    /**
     * Get the effective theme (what's actually being displayed)
     */
    private getCurrentEffectiveTheme(): 'light' | 'dark' {
        if (this.currentTheme === 'auto') {
            return this.getOSTheme();
        }
        return this.currentTheme;
    }

    /**
     * Update the toggle button icon and title
     */
    private updateToggleButton(): void {
        const button = document.getElementById('theme-toggle-btn');
        if (!button) return;

        const effectiveTheme = this.getCurrentEffectiveTheme();
        const isAuto = this.currentTheme === 'auto';
        
        // Update button text
        let buttonText = '';
        if (isAuto) {
            buttonText = 'AUTO';
        } else if (effectiveTheme === 'light') {
            buttonText = '☀️';
        } else {
            buttonText = '🌙';
        }
        button.textContent = buttonText;

        // Update title/tooltip
        let title;
        if (isAuto) {
            title = `Auto theme (currently ${effectiveTheme}). Click for ${effectiveTheme === 'light' ? 'dark' : 'light'} mode.`;
        } else if (effectiveTheme === 'light') {
            title = 'Light mode. Click for dark mode.';
        } else {
            title = 'Dark mode. Click for auto mode.';
        }
        button.title = title;
    }

    /**
     * Initialize the theme system
     */
    private init(): void {
        // Set initial theme
        this.setTheme(this.currentTheme);
        
        // Listen for OS theme changes when in auto mode
        window.matchMedia('(prefers-color-scheme: light)').addEventListener('change', (e) => {
            if (this.currentTheme === 'auto') {
                // Just update the button, the CSS will handle the theme change
                this.updateToggleButton();
            }
        });
        
        // Create and insert toggle button when DOM is ready
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', () => this.createToggleButton());
        } else {
            this.createToggleButton();
        }
    }

    /**
     * Create and insert the theme toggle button
     */
    private createToggleButton(): void {
        const nav = document.querySelector('nav ul');
        if (!nav) {
            return;
        }

        const toggleContainer = document.createElement('li');
        toggleContainer.className = 'theme-toggle';
        
        const button = document.createElement('button');
        button.id = 'theme-toggle-btn';
        button.className = 'theme-toggle-button';
        button.setAttribute('aria-label', 'Toggle theme');
        
        // Use simple text for now to ensure visibility
        button.style.fontSize = '14px';
        button.style.fontWeight = 'bold';
        toggleContainer.appendChild(button);
        nav.appendChild(toggleContainer);
        
        // Add click listener
        button.addEventListener('click', () => this.toggleTheme());
        
        // Initial update
        this.updateToggleButton();
    }

    /**
     * SVG icons for different states
     */
    private getSunIcon(): string {
        return `<circle cx="12" cy="12" r="4" fill="currentColor"/>`;
    }

    private getMoonIcon(): string {
        return `<path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" fill="currentColor"/>`;
    }

    private getAutoIcon(): string {
        return `<rect x="6" y="6" width="12" height="12" fill="currentColor"/>`;
    }
}

// Initialize theme manager
new ThemeManager();