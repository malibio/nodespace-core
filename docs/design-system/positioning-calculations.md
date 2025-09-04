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
  --circle-offset: 26px;        /* Circle position from left edge */
  --circle-diameter: 20px;      /* Circle container size */
  --circle-text-gap: 8px;       /* Gap between circle and text */
  --node-indent: 2.5rem;        /* Parent-child indentation */
}
```

**Content Padding Calculation:**
```
padding-left = circle-offset + circle-diameter + circle-text-gap
padding-left = 26px + 20px + 8px = 54px
```

### Chevron Positioning Formula

**Mathematical Formula:**
```
left = var(--circle-offset) + (var(--circle-diameter) / 2) - (var(--node-indent) / 2)
left = 26px + (20px / 2) - (2.5rem / 2) = 16px
```

**Purpose:** Positions chevrons exactly halfway between parent and child circle centers.

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

## Usage Guidelines

1. **Always use calculated values** - Don't estimate or guess positions
2. **Document calculations** - Include formulas in CSS comments
3. **Test across browsers** - Verify positioning in Chrome, Firefox, Safari
4. **Maintain consistency** - Use same measurement approach across components
5. **Update this document** - When adding new positioning calculations

## Version History

- **v1.0** (2024-09-04): Initial positioning system documentation
  - Autocomplete dropdown positioning: 9.713rem after "@" symbol
  - Node hierarchy mathematical system
  - Chevron positioning formula