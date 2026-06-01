export type ThemeId = "midnight-steel" | "graphite-calm" | "ocean-contrast";
export type TemplateId = "command-center" | "compact-ops";

export type ThemeDefinition = {
  id: ThemeId;
  label: string;
  description: string;
  cssVars: Record<string, string>;
};

export type TemplateDefinition = {
  id: TemplateId;
  label: string;
  density: "comfortable" | "compact";
  radius: "soft" | "crisp";
  showRightRail: boolean;
};

export const discryptUiConfig = {
  activeTheme: "graphite-calm" as ThemeId,
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
    },
    {
      id: "compact-ops",
      label: "Compact ops",
      density: "compact",
      radius: "crisp",
      showRightRail: true,
    },
  ] satisfies TemplateDefinition[],
};

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
