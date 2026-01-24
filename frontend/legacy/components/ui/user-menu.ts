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
      
      if (data.authorized && data.username) {
        const adminLink = data.is_admin ? '<a href="/_admin/">Admin</a>' : '';
        contentDiv.innerHTML = `
          <div class="user-info">
            Signed in as
            <span class="username">${this.escapeHtml(data.username)}</span>
          </div>
          ${adminLink}
          <a href="/_login/profile">Profile</a>
          <a href="/_login/logout">Sign out</a>
        `;
      } else {
        contentDiv.innerHTML = `
          <a href="/_login">Sign in</a>
        `;
      }
    } catch (error) {
      console.error('Error checking auth status:', error);
      // Default to showing login
      contentDiv.innerHTML = `
        <a href="/_login">Sign in</a>
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