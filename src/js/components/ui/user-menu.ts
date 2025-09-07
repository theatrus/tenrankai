// User menu dropdown functionality

export class UserMenu {
  private dropdown: HTMLElement;
  private isOpen = false;

  constructor() {
    const dropdown = document.getElementById('userMenuDropdown');
    if (!dropdown) {
      throw new Error('User menu dropdown not found');
    }
    
    this.dropdown = dropdown;
    this.init();
  }

  private init(): void {
    this.loadUserInfo();
    this.setupClickOutsideHandler();
  }

  public toggle(event: Event): void {
    event.stopPropagation();
    this.isOpen = !this.isOpen;
    
    if (this.isOpen) {
      this.dropdown.classList.add('show');
      // Add click outside listener
      setTimeout(() => {
        document.addEventListener('click', this.handleClickOutside.bind(this));
      }, 0);
    } else {
      this.dropdown.classList.remove('show');
      document.removeEventListener('click', this.handleClickOutside.bind(this));
    }
  }

  private handleClickOutside(event: Event): void {
    const userMenu = document.querySelector('.user-menu');
    if (userMenu && !userMenu.contains(event.target as Node)) {
      this.close();
    }
  }

  private close(): void {
    this.isOpen = false;
    this.dropdown.classList.remove('show');
    document.removeEventListener('click', this.handleClickOutside.bind(this));
  }

  private async loadUserInfo(): Promise<void> {
    const contentDiv = document.getElementById('userMenuContent');
    if (!contentDiv) return;
    
    try {
      const response = await fetch('/api/verify');
      const data = await response.json();
      
      if (data.authenticated) {
        contentDiv.innerHTML = `
          <div class="user-info">
            <div class="user-name">${this.escapeHtml(data.username)}</div>
            <div class="user-email">${this.escapeHtml(data.email)}</div>
          </div>
          <div class="menu-items">
            <a href="/_login/profile" class="menu-item">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="8" r="3"/>
                <path d="M12 14c-4 0-7 2-7 4v1h14v-1c0-2-3-4-7-4z"/>
              </svg>
              Profile
            </a>
            <a href="/_login/logout" class="menu-item">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/>
                <polyline points="16,17 21,12 16,7"/>
                <line x1="21" y1="12" x2="9" y2="12"/>
              </svg>
              Sign Out
            </a>
          </div>
        `;
      } else {
        contentDiv.innerHTML = `
          <div class="menu-items">
            <a href="/_login" class="menu-item">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/>
                <polyline points="10,17 15,12 10,7"/>
                <line x1="15" y1="12" x2="3" y2="12"/>
              </svg>
              Sign In
            </a>
          </div>
        `;
      }
    } catch (error) {
      console.error('Error loading user info:', error);
      contentDiv.innerHTML = `
        <div class="menu-items">
          <a href="/_login" class="menu-item">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/>
              <polyline points="10,17 15,12 10,7"/>
              <line x1="15" y1="12" x2="3" y2="12"/>
            </svg>
            Sign In
          </a>
        </div>
      `;
    }
  }

  private setupClickOutsideHandler(): void {
    // This method is called during initialization but the actual
    // click outside handler is set up dynamically in toggle()
  }

  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}

// Global function for template compatibility
declare global {
  interface Window {
    toggleUserMenu: (event: Event) => void;
    initUserMenu: () => void;
  }
}

let userMenuInstance: UserMenu | null = null;

window.toggleUserMenu = function(event: Event) {
  if (userMenuInstance) {
    userMenuInstance.toggle(event);
  }
};

window.initUserMenu = function() {
  if (!userMenuInstance) {
    userMenuInstance = new UserMenu();
  }
};

// Auto-initialize on DOMContentLoaded
document.addEventListener('DOMContentLoaded', () => {
  if (document.querySelector('.user-menu')) {
    window.initUserMenu();
  }
});