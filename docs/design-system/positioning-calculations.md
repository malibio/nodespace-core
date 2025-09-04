# UI Element Positioning Calculations

This document maintains the precise positioning calculations used throughout NodeSpace's design system to ensure consistent alignment and spacing.

## Autocomplete Dropdown Positioning

### @Mention System

**Base Calculation Method:**
- Measure character width using browser font metrics
- Calculate cumulative position from reference point
- Use rem units for consistent scaling

**Current Implementation:**
```
Base position (after 'a' in "contact"): 7.65rem
Width of "ct @" (4 characters): 2.063rem  
Final position: 7.65rem + 2.063rem = 9.713rem
```

**CSS Implementation:**
```css
.autocomplete-dropdown {
  position: absolute;
  left: 9.713rem;
  top: 2.8rem;
}
```

## Node Hierarchy Positioning

### Mathematical Indentation System

**CSS Variables Architecture:**
```css
.node-viewer {
  --circle-offset: 22px;        /* Circle position from left edge - reserves space for chevrons */
  --circle-diameter: 20px;      /* Circle container size */
  --circle-text-gap: 8px;       /* Gap between circle and text */
  --node-indent: 2.5rem;        /* Parent-child indentation */
}
```

**Content Padding Calculation:**
```
padding-left = circle-offset + circle-diameter + circle-text-gap
padding-left = 22px + 20px + 8px = 50px
```

### Chevron Positioning Formula

**Horizontal Formula:**
```
left = var(--circle-offset) + (var(--circle-diameter) / 2) - (var(--node-indent) / 2)
left = 22px + (20px / 2) - (2.5rem / 2) = 12px
```

**Vertical Formula:**
```
top = calc(0.25rem + (var(--line-height-px) / 2))
```

**Purpose:** Positions chevrons exactly halfway between parent and child circle centers, with perfect vertical alignment to corresponding text baselines.

### Vertical Alignment System

**Circle and Chevron Positioning:**
Both use the same CSS variable system for consistent vertical alignment:
```css
top: calc(0.25rem + (var(--line-height-px) / 2))
```

**Font-Size Responsive Variables:**
- **Default nodes**: `--line-height-px: 1.6rem` (1rem × 1.6)
- **H1 headers**: `--line-height-px: 2.4rem` (2rem × 1.2)  
- **H2 headers**: `--line-height-px: 1.95rem` (1.5rem × 1.3)
- **H3 headers**: `--line-height-px: 1.75rem` (1.25rem × 1.4)

### Baseline Correction System

**Header Text Alignment:**
Different font sizes require different baseline corrections to prevent text floating:

```css
.hierarchy-node.h1 { transform: translateY(-0.05em); }
.hierarchy-node.h2 { transform: translateY(-0.03em); }
.hierarchy-node.h3 { transform: translateY(-0.02em); }
```

**Purpose:** Ensures text sits properly within its container regardless of font size, preventing visual misalignment between text and positioning indicators.

## Font Measurement Standards

**Default Font Assumptions:**
- Base font size: 16px = 1rem
- Font family: system-ui, -apple-system, sans-serif
- Character spacing: Measured dynamically in browser context

**Measurement Approach:**
1. Create hidden test element with matching styles
2. Measure `offsetWidth` for character strings
3. Convert pixels to rem (divide by 16)
4. Apply to positioning calculations

## Design Decisions

### Chevron Space Reservation (22px Circle Offset)

**Decision:** Maintain 22px circle offset even though chevrons only appear on hover.

**Rationale:**
- **Predictable Layout**: Users develop spatial memory for interactive elements. Consistent positioning reduces cognitive load and improves usability.
- **Accessibility**: Screen readers and keyboard navigation depend on predictable element positioning. Dynamic layouts can disrupt assistive technology.
- **Visual Rhythm**: Reserved space creates consistent visual hierarchy that helps users scan content quickly.
- **Industry Standards**: Follows patterns used by macOS Finder, VS Code Explorer, and other professional interfaces that reserve space for disclosure controls.
- **Future-Proofing**: Prevents layout shifts when interaction affordances are added or removed.

**UX Impact:** The 22px reservation ensures nodes maintain consistent alignment whether chevrons are visible or not, prioritizing interface predictability over space optimization.

## Systematic Hover State Implementation

### Global Contrast Fix (v1.3)

**Problem Solved:** Previous use of `--accent` color for hover states created poor 2.28:1 contrast ratios that failed WCAG accessibility standards.

**Solution:** Implemented systematic hover state variables in `shadcn-variables.css`:

```css
/* Systematic hover states - WCAG-compliant 6.47:1 contrast */
--hover-background: var(--muted);
--hover-foreground: var(--muted-foreground);
```

**Implementation Guidelines:**
1. **Always use hover variables** - Never use `--accent` for hover states
2. **Apply universally** - All interactive elements should use these variables
3. **Test contrast ratios** - Verify 4.5:1 minimum for normal text, 3:1 for large text
4. **Consistent behavior** - Hover states work identically across light/dark themes

**Example Usage:**
```css
.date-nav-button:hover {
  background: hsl(var(--hover-background));
  color: hsl(var(--hover-foreground));
}
```

## Usage Guidelines

1. **Always use calculated values** - Don't estimate or guess positions
2. **Document calculations** - Include formulas in CSS comments
3. **Test across browsers** - Verify positioning in Chrome, Firefox, Safari
4. **Maintain consistency** - Use same measurement approach across components
5. **Update this document** - When adding new positioning calculations

## Version History

- **v1.3** (2025-01-04): Container alignment standardization and systematic hover states
  - **Critical Fix**: Resolved 1px horizontal alignment discrepancy between pattern sections
  - **Browser Tab Demo**: Changed from `.pattern-demo` to `.node-viewer` container class for consistency
  - **Systematic Hover States**: Added permanent `--hover-background` and `--hover-foreground` CSS variables
  - **Contrast Compliance**: Fixed poor 2.28:1 contrast ratio to WCAG-compliant 6.47:1 ratio
  - **Date Navigation**: Implemented proper button styling with cohesive 0.5rem gap and consistent hover states
  - **Perfect Alignment**: All pattern section containers now positioned at exactly 101.5px
  - **Layered Architecture**: Established clear separation between outer container styling and inner content consistency

- **v1.2** (2025-01-09): Optimized node alignment with chevron space reservation
  - Updated circle-offset from 26px to 22px for improved root node positioning
  - Adjusted content padding calculation: 50px total (22px + 20px + 8px)
  - Refined chevron positioning formula: 12px horizontal offset
  - Enhanced consistency across @Mention, Slash Commands, and Mathematical sections
  - All root nodes now reserve consistent space for potential chevrons
  
- **v1.1** (2025-01-08): Enhanced positioning system with vertical alignment
  - Added vertical alignment system for chevrons and circles
  - Introduced baseline correction system for header nodes
  - CSS variable-based responsive font sizing
  - Consolidated hierarchy display patterns
  
- **v1.0** (2024-09-04): Initial positioning system documentation
  - Autocomplete dropdown positioning: 9.713rem after "@" symbol
  - Node hierarchy mathematical system
  - Chevron positioning formula