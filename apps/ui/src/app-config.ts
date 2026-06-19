import type { CSSProperties } from "react";

export type ThemeId = "midnight-steel" | "graphite-calm" | "ocean-contrast";
export type TemplateId = "command-center" | "compact-ops";
export type CssVariableMap = Record<`--${string}`, string>;
export const DEFAULT_THEME_ID: ThemeId = "graphite-calm";

export const shadcnThemeTokenNames = [
  "--background",
  "--foreground",
  "--card",
  "--card-foreground",
  "--popover",
  "--popover-foreground",
  "--primary",
  "--primary-foreground",
  "--secondary",
  "--secondary-foreground",
  "--muted",
  "--muted-foreground",
  "--accent",
  "--accent-foreground",
  "--destructive",
  "--destructive-foreground",
  "--border",
  "--input",
  "--ring",
] as const;

export type ShadcnThemeTokenName = (typeof shadcnThemeTokenNames)[number];
export type ShadcnThemeCssVariables = Record<ShadcnThemeTokenName, string>;

export type ShadcnComponentInventoryItem = {
  name: string;
  path: `src/components/ui/${string}.tsx`;
  exports: readonly string[];
  role: string;
};


export type ThemeDefinition = {
  id: ThemeId;
  label: string;
  description: string;
  cssVars: ShadcnThemeCssVariables;
};

export type TemplateDefinition = {
  id: TemplateId;
  label: string;
  density: "comfortable" | "compact";
  radius: "soft" | "crisp";
  showRightRail: boolean;
  cssVars: CssVariableMap;
};

export const shadcnComponentInventory = [
  {
    name: "Avatar",
    path: "src/components/ui/avatar.tsx",
    exports: ["Avatar", "AvatarImage", "AvatarFallback"],
    role: "identity and participant presence marks",
  },
  {
    name: "Badge",
    path: "src/components/ui/badge.tsx",
    exports: ["Badge", "badgeVariants"],
    role: "status, runtime, and safety evidence labels",
  },
  {
    name: "Button",
    path: "src/components/ui/button.tsx",
    exports: ["Button", "buttonVariants"],
    role: "command-backed actions and shell navigation controls",
  },
  {
    name: "Card",
    path: "src/components/ui/card.tsx",
    exports: [
      "Card",
      "CardHeader",
      "CardFooter",
      "CardTitle",
      "CardDescription",
      "CardContent",
    ],
    role: "content panels, setup sections, and inspector surfaces",
  },
  {
    name: "Dialog",
    path: "src/components/ui/dialog.tsx",
    exports: [
      "Dialog",
      "DialogPortal",
      "DialogOverlay",
      "DialogTrigger",
      "DialogClose",
      "DialogContent",
      "DialogHeader",
      "DialogFooter",
      "DialogTitle",
      "DialogDescription",
    ],
    role: "modal-compatible primitives retained for flows that need focusable overlays",
  },
  {
    name: "Input",
    path: "src/components/ui/input.tsx",
    exports: ["Input"],
    role: "profile, invite, message, recovery, and connectivity form fields",
  },
  {
    name: "Label",
    path: "src/components/ui/label.tsx",
    exports: ["Label"],
    role: "accessible form labels for command inputs",
  },
  {
    name: "ScrollArea",
    path: "src/components/ui/scroll-area.tsx",
    exports: ["ScrollArea", "ScrollBar"],
    role: "channel, message, activity, and diagnostics scrolling regions",
  },
  {
    name: "Select",
    path: "src/components/ui/select.tsx",
    exports: [
      "Select",
      "SelectGroup",
      "SelectValue",
      "SelectTrigger",
      "SelectContent",
      "SelectLabel",
      "SelectItem",
      "SelectSeparator",
      "SelectScrollUpButton",
      "SelectScrollDownButton",
    ],
    role: "theme/template and policy selection controls",
  },
  {
    name: "Separator",
    path: "src/components/ui/separator.tsx",
    exports: ["Separator"],
    role: "sidebar and panel boundary rules",
  },
  {
    name: "Slider",
    path: "src/components/ui/slider.tsx",
    exports: ["Slider"],
    role: "voice activity and speaker controls",
  },
  {
    name: "Switch",
    path: "src/components/ui/switch.tsx",
    exports: ["Switch"],
    role: "boolean privacy, mute, and transport toggles",
  },
  {
    name: "Tabs",
    path: "src/components/ui/tabs.tsx",
    exports: ["Tabs", "TabsList", "TabsTrigger", "TabsContent"],
    role: "tab-compatible workspace grouping primitive",
  },
  {
    name: "Tooltip",
    path: "src/components/ui/tooltip.tsx",
    exports: ["Tooltip", "TooltipTrigger", "TooltipContent", "TooltipProvider"],
    role: "screen-reader-safe explanatory hints",
  },
] as const satisfies readonly ShadcnComponentInventoryItem[];

export const discryptUiConfig = {
  activeTheme: DEFAULT_THEME_ID,
  activeTemplate: "command-center" as TemplateId,
  productName: "discrypt",
  accentIntent: "one calm cyan/steel accent; no neon/purple gradients",
  themes: [
    {
      id: "midnight-steel",
      label: "Midnight steel",
      description:
        "Default: quiet Discord-like dark chrome with restrained cyan accents.",
      cssVars: {
        "--background": "222 47% 5%",
        "--foreground": "210 40% 96%",
        "--card": "222 42% 8%",
        "--card-foreground": "210 40% 96%",
        "--popover": "222 42% 7%",
        "--popover-foreground": "210 40% 96%",
        "--primary": "188 67% 48%",
        "--primary-foreground": "222 47% 6%",
        "--secondary": "221 33% 13%",
        "--secondary-foreground": "213 31% 91%",
        "--muted": "222 29% 12%",
        "--muted-foreground": "217 15% 70%",
        "--accent": "190 44% 16%",
        "--accent-foreground": "188 76% 82%",
        "--destructive": "0 72% 51%",
        "--destructive-foreground": "210 40% 98%",
        "--border": "218 28% 18%",
        "--input": "218 28% 18%",
        "--ring": "188 67% 48%",
      },
    },
    {
      id: "graphite-calm",
      label: "Graphite calm",
      description:
        "Lower-saturation professional dark UI for less glow and more editorial quiet.",
      cssVars: {
        "--background": "220 18% 6%",
        "--foreground": "210 20% 96%",
        "--card": "220 17% 9%",
        "--card-foreground": "210 20% 96%",
        "--popover": "220 18% 7%",
        "--popover-foreground": "210 20% 96%",
        "--primary": "196 38% 58%",
        "--primary-foreground": "220 18% 6%",
        "--secondary": "220 15% 14%",
        "--secondary-foreground": "210 18% 90%",
        "--muted": "220 13% 13%",
        "--muted-foreground": "217 12% 70%",
        "--accent": "202 24% 17%",
        "--accent-foreground": "197 42% 82%",
        "--destructive": "0 60% 46%",
        "--destructive-foreground": "210 40% 98%",
        "--border": "220 13% 20%",
        "--input": "220 13% 20%",
        "--ring": "196 38% 58%",
      },
    },
    {
      id: "ocean-contrast",
      label: "Ocean contrast",
      description:
        "More contrast for demos while keeping the same single-accent rule.",
      cssVars: {
        "--background": "215 54% 4%",
        "--foreground": "205 45% 96%",
        "--card": "216 48% 8%",
        "--card-foreground": "205 45% 96%",
        "--popover": "216 48% 7%",
        "--popover-foreground": "205 45% 96%",
        "--primary": "199 84% 56%",
        "--primary-foreground": "216 54% 5%",
        "--secondary": "214 38% 14%",
        "--secondary-foreground": "205 45% 94%",
        "--muted": "214 34% 13%",
        "--muted-foreground": "210 22% 72%",
        "--accent": "199 52% 18%",
        "--accent-foreground": "199 84% 86%",
        "--destructive": "0 68% 50%",
        "--destructive-foreground": "210 40% 98%",
        "--border": "212 35% 20%",
        "--input": "212 35% 20%",
        "--ring": "199 84% 56%",
      },
    },
  ] satisfies ThemeDefinition[],
  templates: [
    {
      id: "command-center",
      label: "Command center",
      density: "comfortable",
      radius: "soft",
      showRightRail: true,
      cssVars: {
        "--template-shell-grid": "72px 300px minmax(0,1fr)",
        "--template-shell-grid-inspector": "72px 300px minmax(0,1fr) 280px",
        "--template-font-size": "16px",
        "--template-panel-radius": "1rem",
      },
    },
    {
      id: "compact-ops",
      label: "Compact ops",
      density: "compact",
      radius: "crisp",
      showRightRail: true,
      cssVars: {
        "--template-shell-grid": "64px 272px minmax(0,1fr)",
        "--template-shell-grid-inspector": "64px 272px minmax(0,1fr) 260px",
        "--template-font-size": "14px",
        "--template-panel-radius": "0.9rem",
      },
    },
  ] satisfies TemplateDefinition[],
  shadcnComponentInventory,
};

export function getThemeDefinition(themeId: string): ThemeDefinition {
  return (
    discryptUiConfig.themes.find((theme) => theme.id === themeId) ??
    discryptUiConfig.themes.find((theme) => theme.id === DEFAULT_THEME_ID) ??
    discryptUiConfig.themes[0]
  );
}

export function createThemeStyle(
  theme: ThemeDefinition,
  extraVars: CssVariableMap = {},
): CSSProperties & CssVariableMap {
  return {
    ...theme.cssVars,
    ...extraVars,
  } as CSSProperties & CssVariableMap;
}

export const setupChecklist = [
  "Verify contact safety number",
  "Review two authorized devices",
  "Confirm invite requires MLS Welcome",
  "Choose retention preset",
];

export const activityFeed = [
  "Invite policy checked: expiry + max-use + revoke controls",
  "Android wake path is content-free",
  "Remote voice audio stays unavailable until a real audio route is confirmed",
  "Deletion copy includes offline-device caveat",
];
