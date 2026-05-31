---
name: DevFlow Dark Blue
colors:
  surface: '#0b1326'
  surface-dim: '#0b1326'
  surface-bright: '#31394d'
  surface-container-lowest: '#060e20'
  surface-container-low: '#131b2e'
  surface-container: '#171f33'
  surface-container-high: '#222a3d'
  surface-container-highest: '#2d3449'
  on-surface: '#dae2fd'
  on-surface-variant: '#c2c6d6'
  inverse-surface: '#dae2fd'
  inverse-on-surface: '#283044'
  outline: '#8c909f'
  outline-variant: '#424754'
  surface-tint: '#adc6ff'
  primary: '#adc6ff'
  on-primary: '#002e6a'
  primary-container: '#4d8eff'
  on-primary-container: '#00285d'
  inverse-primary: '#005ac2'
  secondary: '#bcc7de'
  on-secondary: '#263143'
  secondary-container: '#3e495d'
  on-secondary-container: '#aeb9d0'
  tertiary: '#b7c8e1'
  on-tertiary: '#213145'
  tertiary-container: '#8292aa'
  on-tertiary-container: '#1a2b3e'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#d8e2ff'
  primary-fixed-dim: '#adc6ff'
  on-primary-fixed: '#001a42'
  on-primary-fixed-variant: '#004395'
  secondary-fixed: '#d8e3fb'
  secondary-fixed-dim: '#bcc7de'
  on-secondary-fixed: '#111c2d'
  on-secondary-fixed-variant: '#3c475a'
  tertiary-fixed: '#d3e4fe'
  tertiary-fixed-dim: '#b7c8e1'
  on-tertiary-fixed: '#0b1c30'
  on-tertiary-fixed-variant: '#38485d'
  background: '#0b1326'
  on-background: '#dae2fd'
  surface-variant: '#2d3449'
typography:
  headline-lg:
    fontFamily: Geist
    fontSize: 32px
    fontWeight: '600'
    lineHeight: '1.2'
    letterSpacing: -0.02em
  headline-lg-mobile:
    fontFamily: Geist
    fontSize: 24px
    fontWeight: '600'
    lineHeight: '1.2'
    letterSpacing: -0.01em
  headline-md:
    fontFamily: Geist
    fontSize: 24px
    fontWeight: '500'
    lineHeight: '1.3'
  body-lg:
    fontFamily: Geist
    fontSize: 16px
    fontWeight: '400'
    lineHeight: '1.6'
  body-md:
    fontFamily: Geist
    fontSize: 14px
    fontWeight: '400'
    lineHeight: '1.5'
  label-md:
    fontFamily: JetBrains Mono
    fontSize: 13px
    fontWeight: '500'
    lineHeight: '1.4'
    letterSpacing: 0.02em
  label-sm:
    fontFamily: JetBrains Mono
    fontSize: 11px
    fontWeight: '400'
    lineHeight: '1.4'
    letterSpacing: 0.05em
rounded:
  sm: 0.125rem
  DEFAULT: 0.25rem
  md: 0.375rem
  lg: 0.5rem
  xl: 0.75rem
  full: 9999px
spacing:
  base: 4px
  xs: 0.25rem
  sm: 0.5rem
  md: 1rem
  lg: 1.5rem
  xl: 2.5rem
  gutter: 1rem
  margin-mobile: 1rem
  margin-desktop: 2rem
---

## Brand & Style
The brand personality is precise, technical, and high-performance. It targets software engineers and technical operators who value speed and clarity over ornamentation. The UI evokes a sense of "flow state"—unobtrusive, functional, and reliable.

The design style is **Minimalist / Developer-Centric**. It leans heavily on systematic spacing, a strictly dark environment to reduce eye strain, and a focused "Electric Blue" to highlight primary actions and active states. Surfaces are kept flat or subtly layered to maintain a lightweight feel, avoiding heavy skeuomorphism or unnecessary decorative elements.

## Colors
This design system utilizes a high-contrast dark palette optimized for code environments and technical dashboards. 

- **Primary (#3b82f6):** A clean Electric Blue used for interactive elements, focus states, and primary call-to-actions. It replaces all previous purple tones to provide a more institutional, "Pro" aesthetic.
- **Surface Palette:** The foundation is built on deep navies and charcoal grays. The base background is `#0f172a`, with container surfaces rising to `#1e293b`.
- **Accents:** Success, warning, and error states should use desaturated versions of green, amber, and red to ensure they do not compete with the primary blue for visual attention.
- **Typography:** Primary text is off-white (`#f8fafc`) to prevent the harsh "vibration" of pure white on black, while secondary text uses `#94a3b8`.

## Typography
The typography system prioritizes legibility and technical character. 

- **Geist** is used for the primary UI and body copy, providing a clean, modern, and neutral grotesque that feels home in a developer tool.
- **JetBrains Mono** is utilized for labels, metadata, and status indicators to emphasize the technical nature of the product.
- **Scale:** Headlines use tight letter spacing and heavier weights to create hierarchy against dense data. Labels are often uppercase with slight tracking to increase readability at small sizes.

## Layout & Spacing
The layout follows a **Fluid Grid** model with strict adherence to a 4px baseline grid. 

- **Desktop:** A 12-column grid with 16px (1rem) gutters. Sidebars are typically fixed at 240px or 280px, while the main content area expands to fill the viewport.
- **Mobile:** A 4-column grid with 16px margins. Complex data tables should transition to card-based layouts or horizontal scrolling with sticky headers.
- **Padding:** Use the 8px/16px/24px rhythm for internal container padding to maintain a spacious, professional feel despite the dark color scheme.

## Elevation & Depth
In this dark UI, depth is conveyed through **Tonal Layering** and **Low-Contrast Outlines** rather than heavy shadows.

- **Level 0 (Background):** `#0f172a`. The lowest layer.
- **Level 1 (Cards/Containers):** `#1e293b`. Separated by a 1px border of `#334155`.
- **Level 2 (Popovers/Modals):** `#1e293b`. These utilize a subtle, diffused shadow (0px 10px 15px -3px rgba(0,0,0,0.5)) and a slightly brighter border (`#475569`) to indicate focus.
- **Interactive States:** Use a 1px solid primary color outline (`#3b82f6`) for focused input fields and active button states.

## Shapes
The shape language is "Soft" yet disciplined. 

- **Base Radius (4px):** Applied to buttons, input fields, and small UI components. This creates a modern, slightly rounded look without feeling overly consumer-oriented or "bubbly."
- **Large Radius (8px - 12px):** Reserved for modals and large dashboard cards to soften the overall structure.
- **Pills:** Used exclusively for status tags (e.g., "Active", "Complete") to distinguish them from interactive buttons.

## Components
- **Buttons:** 
  - *Primary:* Solid `#3b82f6` with `#ffffff` text. 4px corner radius.
  - *Secondary:* Ghost style with 1px border of `#334155` and white text.
- **Input Fields:** 
  - Background: `#0f172a`. Border: `#334155`. 
  - Focus state: Border color changes to `#3b82f6` with a 2px outer glow.
- **Chips/Status Tags:** Small, semi-transparent backgrounds with high-contrast text. (e.g., Active state uses a 10% opacity blue background with a solid blue text).
- **Cards:** Flat surfaces using `#1e293b` with a 1px border. No shadows for standard grid cards to keep the UI "light."
- **Data Tables:** Zebra striping is avoided. Instead, use thin borders (`#1e293b`) and a highlight on row-hover using `#1e293b` with a slight brightness increase.
- **Code Snippets:** Use a slightly darker background than the container (`#020617`) with JetBrains Mono typography to provide a distinct "Editor" feel.