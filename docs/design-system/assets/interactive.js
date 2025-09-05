/**
 * NodeSpace Design System Interactive Features
 * 
 * Provides theme switching, code copying, and interactive demonstrations
 * for the design system documentation.
 */

// Design tokens (will be loaded dynamically)
let designTokens = null;
let currentTheme = 'system';

// Initialize when DOM is loaded
document.addEventListener('DOMContentLoaded', async () => {
  await loadDesignTokens();
  initializeTheme();
  initializeInteractiveFeatures();
  initializeKeyboardShortcuts();
  updateAllExamples();
});

// Load design tokens
async function loadDesignTokens() {
  // In a real application, this would load from the compiled tokens
  // For the style guide, we'll embed the tokens directly
  designTokens = {
    light: {
      color: {
        primary: {
          50: '#eff6ff',
          100: '#dbeafe',
          200: '#bfdbfe',
          300: '#93c5fd',
          400: '#60a5fa',
          500: '#007acc',
          600: '#0066b3',
          700: '#005299',
          800: '#003d80',
          900: '#002966'
        },
        surface: {
          background: '#ffffff',
          elevated: '#ffffff',
          overlay: 'rgba(0, 0, 0, 0.1)',
          panel: '#f8f9fa',
          input: '#ffffff'
        },
        text: {
          primary: '#1a1a1a',
          secondary: '#333333',
          tertiary: '#666666',
          inverse: '#ffffff',
          disabled: '#999999',
          placeholder: '#9ca3af'
        },
        border: {
          subtle: '#f3f4f6',
          default: '#e1e5e9',
          strong: '#d1d5db',
          accent: '#007acc'
        }
      },
      typography: {
        fontFamily: {
          ui: '"SF Mono", Monaco, "Cascadia Code", "Roboto Mono", Consolas, monospace',
          mono: '"SF Mono", Monaco, "Cascadia Code", "Roboto Mono", Consolas, monospace'
        },
        fontSize: {
          xs: '0.75rem',
          sm: '0.875rem',
          base: '1rem',
          lg: '1.125rem',
          xl: '1.25rem',
          '2xl': '1.5rem',
          '3xl': '1.875rem'
        }
      },
      spacing: {
        1: '0.25rem',
        2: '0.5rem',
        3: '0.75rem',
        4: '1rem',
        5: '1.25rem',
        6: '1.5rem',
        8: '2rem'
      }
    },
    dark: {
      color: {
        primary: {
          50: '#002966',
          100: '#003d80',
          200: '#005299',
          300: '#0066b3',
          400: '#007acc',
          500: '#0099ff',
          600: '#33aaff',
          700: '#66bbff',
          800: '#99ccff',
          900: '#cce6ff'
        },
        surface: {
          background: '#1a1a1a',
          elevated: '#2a2a2a',
          overlay: 'rgba(255, 255, 255, 0.1)',
          panel: '#2a2a2a',
          input: '#374151'
        },
        text: {
          primary: '#ffffff',
          secondary: '#e0e0e0',
          tertiary: '#888888',
          inverse: '#1a1a1a',
          disabled: '#666666',
          placeholder: '#9ca3af'
        },
        border: {
          subtle: '#374151',
          default: '#4b5563',
          strong: '#6b7280',
          accent: '#0099ff'
        }
      },
      typography: {
        fontFamily: {
          ui: '"SF Mono", Monaco, "Cascadia Code", "Roboto Mono", Consolas, monospace',
          mono: '"SF Mono", Monaco, "Cascadia Code", "Roboto Mono", Consolas, monospace'
        },
        fontSize: {
          xs: '0.75rem',
          sm: '0.875rem',
          base: '1rem',
          lg: '1.125rem',
          xl: '1.25rem',
          '2xl': '1.5rem',
          '3xl': '1.875rem'
        }
      },
      spacing: {
        1: '0.25rem',
        2: '0.5rem',
        3: '0.75rem',
        4: '1rem',
        5: '1.25rem',
        6: '1.5rem',
        8: '2rem'
      }
    }
  };
}

// Initialize theme system
function initializeTheme() {
  // Load saved theme preference
  const savedTheme = localStorage.getItem('nodespace-style-guide-theme') || 'system';
  currentTheme = savedTheme;
  
  // Create theme toggle buttons
  const themeToggle = document.querySelector('.theme-toggle');
  if (themeToggle) {
    themeToggle.innerHTML = `
      <span style="font-size: 0.875rem; color: hsl(var(--muted-foreground, 215 16% 47%));">Theme:</span>
      <button data-theme="light" title="Light theme">☀️</button>
      <button data-theme="dark" title="Dark theme">🌙</button>
      <button data-theme="system" title="System theme">🖥️</button>
    `;
    
    // Add event listeners
    themeToggle.querySelectorAll('button').forEach(button => {
      button.addEventListener('click', () => {
        const theme = button.dataset.theme;
        setTheme(theme);
      });
    });
  }
  
  // Apply initial theme
  applyTheme(currentTheme);
  updateThemeToggle();
  
  // Listen for system theme changes
  if (window.matchMedia) {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    mediaQuery.addListener(() => {
      if (currentTheme === 'system') {
        applyTheme('system');
      }
    });
  }
}

// Set theme
function setTheme(theme) {
  currentTheme = theme;
  localStorage.setItem('nodespace-style-guide-theme', theme);
  applyTheme(theme);
  updateThemeToggle();
}

// Apply theme
function applyTheme(theme) {
  const resolvedTheme = resolveTheme(theme);
  
  // Set theme class on document element for CSS to handle
  document.documentElement.className = resolvedTheme === 'dark' ? 'dark' : '';
  document.body.setAttribute('data-theme', resolvedTheme);
  
  // Update all examples
  updateAllExamples();
}

// Resolve theme (system -> light/dark)
function resolveTheme(theme) {
  if (theme === 'system') {
    return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  return theme;
}

// Update theme toggle buttons
function updateThemeToggle() {
  const buttons = document.querySelectorAll('.theme-toggle button');
  buttons.forEach(button => {
    button.classList.toggle('active', button.dataset.theme === currentTheme);
  });
}

// Note: CSS properties are now handled by shadcn-variables.css
// The actual shadcn-svelte variables are used directly

// Initialize keyboard shortcuts
function initializeKeyboardShortcuts() {
  document.addEventListener('keydown', (e) => {
    // Cmd/Ctrl + \: Toggle between light and dark theme (backslash is safe from browser conflicts)
    if ((e.metaKey || e.ctrlKey) && e.key === '\\') {
      e.preventDefault();
      
      // Toggle between light and dark (skip system for this shortcut)
      const newTheme = currentTheme === 'dark' ? 'light' : 'dark';
      setTheme(newTheme);
    }
  });
}

// Initialize interactive features
function initializeInteractiveFeatures() {
  // Copy code buttons
  document.querySelectorAll('.copy-button').forEach(button => {
    button.addEventListener('click', () => {
      const codeElement = button.parentElement.querySelector('code');
      if (codeElement) {
        copyToClipboard(codeElement.textContent);
        showCopyFeedback(button);
      }
    });
  });
  
  // Add copy buttons to code blocks
  document.querySelectorAll('pre code').forEach(codeElement => {
    const pre = codeElement.parentElement;
    if (!pre.querySelector('.copy-button')) {
      const button = document.createElement('button');
      button.className = 'copy-button';
      button.textContent = 'Copy';
      button.addEventListener('click', () => {
        copyToClipboard(codeElement.textContent);
        showCopyFeedback(button);
      });
      pre.style.position = 'relative';
      pre.appendChild(button);
    }
  });
  
  // Interactive component demonstrations
  initializeComponentDemos();
}

// Copy to clipboard
async function copyToClipboard(text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch (err) {
    // Fallback for older browsers
    const textarea = document.createElement('textarea');
    textarea.value = text;
    document.body.appendChild(textarea);
    textarea.select();
    document.execCommand('copy');
    document.body.removeChild(textarea);
  }
}

// Show copy feedback
function showCopyFeedback(button) {
  const originalText = button.textContent;
  button.textContent = 'Copied!';
  button.style.backgroundColor = 'hsl(var(--node-text, 142 71% 45%))';
  button.style.color = 'white';
  
  setTimeout(() => {
    button.textContent = originalText;
    button.style.backgroundColor = '';
    button.style.color = '';
  }, 2000);
}

// Update all examples when theme changes
function updateAllExamples() {
  updateColorExamples();
  updateTypographyExamples();
  updateSpacingExamples();
  updateComponentExamples();
}

// Update color examples
function updateColorExamples() {
  const colorSwatches = document.querySelectorAll('.color-swatch');
  const tokens = designTokens[resolveTheme(currentTheme)];
  
  colorSwatches.forEach(swatch => {
    const colorName = swatch.dataset.color;
    if (colorName && tokens.color) {
      const colorPath = colorName.split('-');
      let colorValue = tokens.color;
      
      colorPath.forEach(path => {
        if (colorValue && colorValue[path]) {
          colorValue = colorValue[path];
        }
      });
      
      if (typeof colorValue === 'string') {
        const preview = swatch.querySelector('.color-preview');
        const valueElement = swatch.querySelector('.color-value');
        
        if (preview) {
          preview.style.backgroundColor = colorValue;
          
          // Update text color for contrast
          const isLight = isLightColor(colorValue);
          preview.style.color = isLight ? '#000' : '#fff';
          preview.textContent = colorValue;
        }
        
        if (valueElement) {
          valueElement.textContent = colorValue;
        }
      }
    }
  });
}

// Update typography examples
function updateTypographyExamples() {
  const specimens = document.querySelectorAll('.typography-specimen');
  const tokens = designTokens[resolveTheme(currentTheme)];
  
  specimens.forEach(specimen => {
    const preview = specimen.querySelector('.specimen-preview');
    const fontSize = specimen.dataset.fontSize;
    const fontWeight = specimen.dataset.fontWeight;
    
    if (preview && tokens.typography) {
      if (fontSize && tokens.typography.fontSize[fontSize]) {
        preview.style.fontSize = tokens.typography.fontSize[fontSize];
      }
      
      if (fontWeight && tokens.typography.fontWeight) {
        preview.style.fontWeight = tokens.typography.fontWeight[fontWeight] || fontWeight;
      }
    }
  });
}

// Update spacing examples
function updateSpacingExamples() {
  const spacingItems = document.querySelectorAll('.spacing-item');
  const tokens = designTokens[resolveTheme(currentTheme)];
  
  spacingItems.forEach(item => {
    const visual = item.querySelector('.spacing-visual');
    const spacing = item.dataset.spacing;
    
    if (visual && spacing && tokens.spacing && tokens.spacing[spacing]) {
      visual.style.width = tokens.spacing[spacing];
    }
  });
}

// Initialize component demos
function initializeComponentDemos() {
  // Add interactivity to button examples
  document.querySelectorAll('.demo-button').forEach(button => {
    button.addEventListener('click', (e) => {
      e.preventDefault();
      button.style.transform = 'scale(0.98)';
      setTimeout(() => {
        button.style.transform = '';
      }, 150);
    });
  });
  
  // Add interactivity to node examples
  document.querySelectorAll('.demo-node').forEach(node => {
    node.addEventListener('click', () => {
      node.classList.toggle('selected');
    });
  });
}

// Update component examples
function updateComponentExamples() {
  // This will be called when theme changes to update any dynamic content
  // Most components will update automatically via CSS custom properties
}

// Check if color is light (for contrast calculation)
function isLightColor(color) {
  // Convert hex to RGB
  const hex = color.replace('#', '');
  const r = parseInt(hex.substr(0, 2), 16);
  const g = parseInt(hex.substr(2, 2), 16);
  const b = parseInt(hex.substr(4, 2), 16);
  
  // Calculate relative luminance
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  
  return luminance > 0.5;
}

// Generate example code
function generateExampleCode(type, variant) {
  const examples = {
    button: {
      primary: `<button class="ns-button ns-button--primary">Primary Button</button>`,
      secondary: `<button class="ns-button">Secondary Button</button>`,
      disabled: `<button class="ns-button" disabled>Disabled Button</button>`
    },
    input: {
      default: `<input class="ns-input" placeholder="Enter text...">`,
      disabled: `<input class="ns-input" placeholder="Disabled" disabled>`
    },
    node: {
      text: `<div class="ns-node ns-node--text">
  <div class="ns-node__header">
    <div class="ns-node__type-indicator">
      <span class="ns-node__icon">📝</span>
    </div>
    <div class="ns-node__title-section">
      <h3 class="ns-node__title">Text Node</h3>
    </div>
  </div>
  <div class="ns-node__content">
    <p>This is a text node with some content.</p>
  </div>
</div>`
    }
  };
  
  return examples[type] && examples[type][variant] || '';
}

// Export functions for global access
window.NodeSpaceStyleGuide = {
  setTheme,
  copyToClipboard,
  generateExampleCode,
  updateAllExamples
};