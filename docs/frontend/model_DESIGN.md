---
name: Obsidian Grove
colors:
  surface: '#111317'
  surface-dim: '#111317'
  surface-bright: '#37393d'
  surface-container-lowest: '#0c0e11'
  surface-container-low: '#1a1c1f'
  surface-container: '#1e2023'
  surface-container-high: '#282a2d'
  surface-container-highest: '#333538'
  on-surface: '#e2e2e6'
  on-surface-variant: '#bbcabf'
  inverse-surface: '#e2e2e6'
  inverse-on-surface: '#2f3034'
  outline: '#86948a'
  outline-variant: '#3c4a42'
  surface-tint: '#4edea3'
  primary: '#4edea3'
  on-primary: '#003824'
  primary-container: '#10b981'
  on-primary-container: '#00422b'
  inverse-primary: '#006c49'
  secondary: '#45dfa4'
  on-secondary: '#003825'
  secondary-container: '#00bd85'
  on-secondary-container: '#00452e'
  tertiary: '#68dba9'
  on-tertiary: '#003825'
  tertiary-container: '#3eb686'
  on-tertiary-container: '#00422c'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#6ffbbe'
  primary-fixed-dim: '#4edea3'
  on-primary-fixed: '#002113'
  on-primary-fixed-variant: '#005236'
  secondary-fixed: '#68fcbf'
  secondary-fixed-dim: '#45dfa4'
  on-secondary-fixed: '#002114'
  on-secondary-fixed-variant: '#005137'
  tertiary-fixed: '#85f8c4'
  tertiary-fixed-dim: '#68dba9'
  on-tertiary-fixed: '#002114'
  on-tertiary-fixed-variant: '#005137'
  background: '#111317'
  on-background: '#e2e2e6'
  surface-variant: '#333538'
typography:
  headline-xl:
    fontFamily: Inter
    fontSize: 24px
    fontWeight: '600'
    lineHeight: 32px
    letterSpacing: -0.02em
  headline-lg:
    fontFamily: Inter
    fontSize: 18px
    fontWeight: '600'
    lineHeight: 24px
    letterSpacing: -0.015em
  headline-md:
    fontFamily: Inter
    fontSize: 15px
    fontWeight: '500'
    lineHeight: 20px
    letterSpacing: -0.01em
  body-md:
    fontFamily: Inter
    fontSize: 13px
    fontWeight: '400'
    lineHeight: 18px
    letterSpacing: 0em
  body-sm:
    fontFamily: Inter
    fontSize: 12px
    fontWeight: '400'
    lineHeight: 16px
    letterSpacing: 0em
  label-md:
    fontFamily: JetBrains Mono
    fontSize: 12px
    fontWeight: '500'
    lineHeight: 16px
    letterSpacing: -0.01em
  label-sm:
    fontFamily: JetBrains Mono
    fontSize: 11px
    fontWeight: '500'
    lineHeight: 14px
    letterSpacing: 0.02em
  label-xs:
    fontFamily: JetBrains Mono
    fontSize: 10px
    fontWeight: '400'
    lineHeight: 12px
    letterSpacing: 0.04em
rounded:
  sm: 0.125rem
  DEFAULT: 0.25rem
  md: 0.375rem
  lg: 0.5rem
  xl: 0.75rem
  full: 9999px
spacing:
  gutter: 0.75rem
  margin: 1rem
  space-xs: 0.25rem
  space-sm: 0.5rem
  space-md: 0.75rem
  space-lg: 1rem
  space-xl: 1.5rem
---

## Brand & Style

This design system is engineered for power users, digital artists, archivists, and creative technologists who manage tens of thousands of visual assets locally. The brand identity fuses developer-grade performance with the calm, polished precision of modern productivity tools like Raycast and Linear.

The aesthetic is characterized by:
- **Maximum Media Canvas:** Eliminates traditional heavy left-hand tree drawers to preserve viewport width for dynamic grids.
- **Deep Obsidian Surfaces:** Multi-tiered dark neutral backgrounds that recede visually, allowing high-gamut artwork, animations, and photography to stand out without chromatic competition.
- **Precision Neon Highlights:** High-contrast electric emerald and mint green signals status, selection states, active pills, and confirmed batch tasks.
- **Micro-tactility:** Ultra-fine 1px border lines, dark inner highlights, monospaced metadata badges, and precise interactive feedback.

## Colors

The palette is tuned specifically for deep-dark environments where visual content fidelity and low eye strain are critical.

### Color Tiers
- **Canvas Base (`#0c0e11`):** The primary window frame, background foundation, and modal scrim layer.
- **Surface Elevation 1 (`#14171d`):** Primary card bodies, inspector side panels, search bar containers, and table row groupings.
- **Surface Elevation 2 (`#1a1e26`):** Hover states, input fields, popovers, and elevated chip containers.
- **Surface Elevation 3 (`#222733`):** Active toggles, selected card backgrounds, drag-and-drop targets, and modal dialog bodies.
- **Subtle Borders (`#262b35` to `#2f3642`):** Crisp 1px structural outlines separating controls, card edges, and top app bars without high-contrast friction.
- **Accent Primary (`#10b981`):** Electric Emerald for primary action buttons, focused outlines, active filter badges, and slider tracks.
- **Accent Glow & Hover (`#34d399`):** Mint highlight for icon tooltips, interactive hover states, and active tag pills.
- **Accent Muted (`rgba(16, 185, 129, 0.12)`): Translucent emerald wash for selected media borders, multi-selection bounding boxes, and active segment backgrounds.
- **Destructive/Danger (`#f87171`):** Subdued coral red for deletions, reset caches, and critical warnings, paired with translucent backdrop washes (`rgba(248, 113, 113, 0.1)`).
- **Text & Glyphs:** High-contrast primary text (`#f1f5f9`), secondary metadata (`#94a3b8`), and tertiary disabled hints (`#475569`).

## Typography

Typography prioritizes information density, scanning clarity, and geometric balance.

- **Primary Interface (Inter):** Applied across headers, button text, search inputs, modal titles, and descriptive settings copy. Tight negative letter spacing at sizes above 15px gives the chrome a technical, desktop-native feel.
- **Technical Badges & Metadata (JetBrains Mono):** Applied for total item counts, file path strings, dimensions, hex codes, keyboard shortcut anchors (e.g., `⌘K`, `Esc`), and file size metrics (`24.8 MB`). Monospaced characters prevent layout shifts as values, indexes, and bulk counts increment dynamically.

## Layout & Spacing

The layout is built around a full-bleed application shell designed to maximize visual workspace:

- **Top Application Command Bar:** Fixed at `48px` to `52px` height containing the logo mark, universal search query box with parameter syntax, collection format filters (All, Images, GIF, Video), item count telemetry, and quick-action utility icons.
- **No-Sidebar Architecture:** Left drawer navigation is eliminated. Filtering and categorization occur via horizontal pill clusters, contextual command palettes, and an on-demand slide-over Inspector sheet.
- **Dynamic Media Matrix:** A fluid grid canvas that auto-calculates thumbnail column spans based on a continuous zoom slider (ranging from small `80px` contact sheets to large `420px` inspection cards). Standard gap between media cards is locked to `gutter` (`0.75rem` / `12px`).
- **Inspector Panel:** An optional floating or docked right-hand rail (`320px` fixed width) containing bulk tags, group keys, and metadata fields without resizing the viewport structure awkwardly.
- **Responsive Adaptations:** Desktop resolutions (`>= 1440px`) display the gallery alongside an open inspector sheet. On intermediate displays (`1024px - 1439px`), the inspector transforms into a floating overlay card that slides smoothly over the right boundary.

## Elevation & Depth

Visual hierarchy uses tonal surface layering bounded by crisp micro-borders rather than aggressive drop shadows:

- **Level 0 (Canvas Base):** Deepest level `#0c0e11`. Serves as the backdrop for thumbnails and root application layout.
- **Level 1 (Panels & Headers):** Background `#14171d` with `1px solid #262b35` perimeter border. Used for the top application header, floating bottom action docks, and settings group wrappers.
- **Level 2 (Active Cards & Floating Menus):** Background `#1a1e26` with `1px solid #2f3642` outline. Paired with a soft ambient shadow: `0 8px 24px rgba(0, 0, 0, 0.45)`. Applied to context dropdown menus, autocomplete popovers, and floating tooltips.
- **Level 3 (Modal Media Overlay):** Background `#14171d` flanked by a backdrop blur (`backdrop-filter: blur(16px); background-color: rgba(12, 14, 17, 0.85)`). Highlights focused assets in full preview mode with an outer border `1px solid rgba(255, 255, 255, 0.08)`.
- **Accent Selection Glow:** Selected thumbnails or focused inputs receive a crisp `1px solid #10b981` border with a subtle concentric ring `0 0 0 1px rgba(16, 185, 129, 0.25)`.

## Shapes

The geometric framework favors tight, modern corner radii (Soft / Level 1) to convey precision and tool-like efficiency:

- **Thumbnails & Media Cards:** `rounded` (`4px` / `0.25rem`) to maintain sharp card definitions and minimize dead space between adjacent visual media.
- **Inputs, Buttons, and Inspector Rows:** `rounded-md` (`6px` / `0.375rem`), balancing touch/click affordance with compact utility density.
- **Filter Pills & Tag Tokens:** `rounded-full` (`9999px`) exclusively for categorizing metadata, tags, format badges, and count markers.
- **Settings Groups & Modals:** `rounded-lg` (`8px` / `0.5rem`) for high-level container boundaries and floating dialog panels.

## Components

### Buttons
- **Primary Action:** Solid background `#10b981`, label in `#0c0e11` (semi-bold `Inter`), subtle hover transition to `#34d399`. Focus state adds `0 0 0 2px rgba(16, 185, 129, 0.35)`.
- **Secondary Action:** Transparent background with `#262b35` border and `#f1f5f9` text. Hover shifts background to `#1a1e26` and border to `#2f3642`.
- **Destructive Action:** Low-contrast coral container (`rgba(248, 113, 113, 0.08)`), text `#f87171`, border `1px solid rgba(248, 113, 113, 0.2)`. On hover: `rgba(248, 113, 113, 0.16)`.
- **Icon Utility Button:** Ghost `28x28px` square with `rounded-md`, housing centered `16px` SVG icons in `#94a3b8`. Hover transitions to `#f1f5f9` text and `#1a1e26` background.

### Search Bar & Tag Token Input
- Centered in the top bar with a fixed height of `32px` and maximum width of `640px`.
- Background `#14171d`, border `1px solid #262b35`, text `#f1f5f9`.
- Supports inline tag tokens: pills rendered directly in the input stream with `#1a1e26` background, `#34d399` text, and a micro dismiss `×` icon.
- Leading search icon (`#94a3b8`) and trailing shortcut chip (`⌘K` rendered in JetBrains Mono).

### Media Cards
- Zero-margin visual containment with a persistent `1px solid #1a1e26` micro-border preventing light images from bleeding into the dark background.
- Hover state shows an elevated highlight border (`#2f3642`) and displays overlay action triggers (favorite heart, quick tag button, selection checkbox).
- Selected state activates a `2px solid #10b981` boundary, inset inner glow, and a checked emerald badge in the upper right corner.

### Chips & Filter Pills
- Filter pills (`All`, `Images`, `GIF`, `Video`) feature a compact height of `26px`, padding `0 10px`, font size `12px`.
- Inactive pills: background `#14171d`, text `#94a3b8`, border `1px solid #262b35`.
- Active pills: background `#10b981`, text `#0c0e11`, font weight `500`, border `1px solid #10b981`.

### Sliders & Grid Scaling Controls
- Thin track height of `4px` in `#262b35`, filled active track in `#10b981`.
- Thumb slider: `12px` circular indicator in `#f1f5f9` with subtle drop shadow `0 2px 4px rgba(0,0,0,0.5)`. Hover expands thumb to `14px` with a `#34d399` halo.

### Modals & Media Inspector
- Modal asset viewer spans full screen with a `16px` outer margin. Divided into a large fluid media viewport (left) and an integrated tagging and details panel (right, `340px`).
- Right inspector provides quick group assignment inputs, tag clouds, re-scan buttons, and EXIF/metadata listings in structured monospaced rows.
